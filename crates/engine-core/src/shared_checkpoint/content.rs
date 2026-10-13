//! Versioned, streaming byte content for future chunked values and assets.
//!
//! This is an internal storage primitive, not a published checkpoint/commit or an
//! authorization API. Callers authorize the document and own transaction rollback,
//! root publication and staged output. See docs/CHUNKED-CONTENT.md.

use ring::digest::{Context, SHA256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::{Address, Error, Limits, ObjectStore, WriteStats, Writer};
use engine_shared::DocumentId;

pub const CHUNK_BYTES: usize = 64 * 1024;
pub const FANOUT: usize = 128;
pub const MAX_HEIGHT: u8 = 4;
pub(super) const MAGIC: &[u8] = b"DYNOCNT\0";
const DOMAIN: &[u8] = b"dynodoc.content\0v1\0";
const HEADER: usize = 10;

#[derive(Debug, Clone)]
struct Reference {
    address: Address,
    length: u64,
}

enum Object<'a> {
    Chunk(&'a [u8]),
    Branch {
        height: u8,
        children: Vec<Reference>,
    },
    Root {
        height: u8,
        child: Reference,
    },
}

fn capacity(height: u8) -> u64 {
    (CHUNK_BYTES as u64) * (FANOUT as u64).pow(height.into())
}

fn height_for(length: u64) -> Result<u8, Error> {
    (0..=MAX_HEIGHT)
        .find(|height| length <= capacity(*height))
        .ok_or(Error::Limit("content tree height"))
}

fn address(bytes: &[u8]) -> Address {
    let mut hash = Context::new(&SHA256);
    hash.update(DOMAIN);
    hash.update(bytes);
    Address(super::hex(hash.finish().as_ref()))
}

fn header(tag: u8) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(HEADER);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&[1, tag]);
    bytes
}

fn put_address(bytes: &mut Vec<u8>, address: &Address) {
    // Address construction already guarantees 64 lowercase hex characters.
    for pair in address.as_str().as_bytes().chunks_exact(2) {
        let digit = |b: u8| if b <= b'9' { b - b'0' } else { b - b'a' + 10 };
        bytes.push(digit(pair[0]) * 16 + digit(pair[1]));
    }
}

fn reference(bytes: &[u8]) -> Reference {
    Reference {
        address: Address(super::hex(&bytes[..32])),
        length: u64::from_be_bytes(bytes[32..40].try_into().unwrap()),
    }
}

fn decode<'a>(expected: &Address, bytes: &'a [u8]) -> Result<Object<'a>, Error> {
    if bytes.len() > HEADER + CHUNK_BYTES {
        return Err(Error::Limit("content object bytes"));
    }
    if bytes.len() < HEADER || !bytes.starts_with(MAGIC) || bytes[8] != 1 {
        return Err(Error::Invalid("content header or version"));
    }
    if &address(bytes) != expected {
        return Err(Error::HashMismatch);
    }
    match bytes[9] {
        1 => Ok(Object::Chunk(&bytes[HEADER..])),
        2 if bytes.len() >= 21 => {
            let height = bytes[10];
            let length = u64::from_be_bytes(bytes[11..19].try_into().unwrap());
            let count = u16::from_be_bytes(bytes[19..21].try_into().unwrap()) as usize;
            if height == 0
                || height > MAX_HEIGHT
                || count == 0
                || count > FANOUT
                || bytes.len() != 21 + count * 40
            {
                return Err(Error::Invalid("content branch bounds"));
            }
            let children: Vec<_> = bytes[21..].chunks_exact(40).map(reference).collect();
            let maximum = capacity(height - 1);
            let mut total = 0;
            for (index, child) in children.iter().enumerate() {
                if child.length == 0
                    || child.length > maximum
                    || (index + 1 < count && child.length != maximum)
                {
                    return Err(Error::Invalid("content branch packing"));
                }
                total += child.length; // bounded by capacity(MAX_HEIGHT), below u64::MAX
            }
            if total != length {
                return Err(Error::Invalid("content branch length"));
            }
            Ok(Object::Branch { height, children })
        }
        3 if bytes.len() == 51 => {
            let height = bytes[10];
            let length = u64::from_be_bytes(bytes[11..19].try_into().unwrap());
            if height_for(length)? != height {
                return Err(Error::Invalid("content root height"));
            }
            Ok(Object::Root {
                height,
                child: Reference {
                    address: Address(super::hex(&bytes[19..51])),
                    length,
                },
            })
        }
        _ => Err(Error::Invalid("content object kind or size")),
    }
}

pub(super) fn validate(expected: &Address, bytes: &[u8]) -> Result<(), Error> {
    decode(expected, bytes).map(|_| ())
}

/// Direct child addresses of a verified content object (see `super::references`).
pub(super) fn references(expected: &Address, bytes: &[u8]) -> Result<Vec<Address>, Error> {
    Ok(match decode(expected, bytes)? {
        Object::Chunk(_) => Vec::new(),
        Object::Branch { children, .. } => {
            children.into_iter().map(|child| child.address).collect()
        }
        Object::Root { child, .. } => vec![child.address],
    })
}

struct Builder<'a, 'b, S> {
    writer: &'a mut Writer<'b, S>,
    levels: Vec<Vec<Reference>>,
}

impl<S: ObjectStore> Builder<'_, '_, S> {
    async fn branch(&mut self, height: u8, children: &[Reference]) -> Result<Reference, Error> {
        let length = children.iter().map(|child| child.length).sum::<u64>();
        let mut bytes = header(2);
        bytes.push(height);
        bytes.extend_from_slice(&length.to_be_bytes());
        bytes.extend_from_slice(&(children.len() as u16).to_be_bytes());
        for child in children {
            put_address(&mut bytes, &child.address);
            bytes.extend_from_slice(&child.length.to_be_bytes());
        }
        let address = self.writer.encoded(address(&bytes), bytes).await?;
        Ok(Reference { address, length })
    }

    async fn push(&mut self, mut level: usize, mut child: Reference) -> Result<(), Error> {
        loop {
            if level > MAX_HEIGHT as usize {
                return Err(Error::Limit("content tree height"));
            }
            self.levels[level].push(child);
            if self.levels[level].len() < FANOUT {
                return Ok(());
            }
            if level == MAX_HEIGHT as usize {
                return Err(Error::Limit("content tree height"));
            }
            let children = std::mem::take(&mut self.levels[level]);
            child = self.branch(level as u8 + 1, &children).await?;
            level += 1;
        }
    }

    async fn finish(&mut self) -> Result<(u8, Reference), Error> {
        for level in 0..=MAX_HEIGHT as usize {
            if self.levels[level].is_empty() {
                continue;
            }
            if self.levels[level].len() == 1 && self.levels[level + 1..].iter().all(Vec::is_empty) {
                return Ok((level as u8, self.levels[level].pop().unwrap()));
            }
            if level == MAX_HEIGHT as usize {
                return Err(Error::Limit("content tree height"));
            }
            let children = std::mem::take(&mut self.levels[level]);
            let child = self.branch(level as u8 + 1, &children).await?;
            self.push(level + 1, child).await?;
        }
        Err(Error::Invalid("empty content frontier"))
    }
}

/// Store a byte stream with deterministic 64 KiB chunks and a bounded radix
/// frontier. Repeated calls reuse identical objects only within this document.
/// Input errors/budget exhaustion may leave unpublished objects: the caller must
/// own a transaction or staging/cleanup policy. Returning a root does not publish
/// a checkpoint, commit, branch or native asset reference.
pub async fn write<S: ObjectStore, R: AsyncRead + Unpin + Send>(
    store: &mut S,
    document: DocumentId,
    input: &mut R,
    limits: Limits,
) -> Result<(Address, WriteStats), Error> {
    let mut writer = Writer {
        store,
        document,
        budget: limits,
        stats: WriteStats::default(),
        pending: Vec::new(),
        pending_bytes: 0,
    };
    let root = write_with_writer(&mut writer, input).await?;
    writer.flush().await?;
    Ok((root, writer.stats))
}

pub(super) async fn write_with_writer<S: ObjectStore, R: AsyncRead + Unpin + Send>(
    writer: &mut Writer<'_, S>,
    input: &mut R,
) -> Result<Address, Error> {
    let mut builder = Builder {
        writer,
        levels: (0..=MAX_HEIGHT).map(|_| Vec::new()).collect(),
    };
    let mut buffer = vec![0; CHUNK_BYTES];
    let mut first = true;
    loop {
        let mut count = 0;
        while count < CHUNK_BYTES {
            let n = input.read(&mut buffer[count..]).await?;
            if n == 0 {
                break;
            }
            count += n;
        }
        if count == 0 && !first {
            break;
        }
        first = false;
        let mut bytes = header(1);
        bytes.extend_from_slice(&buffer[..count]);
        let address = builder.writer.encoded(address(&bytes), bytes).await?;
        builder
            .push(
                0,
                Reference {
                    address,
                    length: count as u64,
                },
            )
            .await?;
        if count < CHUNK_BYTES {
            break;
        }
    }
    let (height, child) = builder.finish().await?;
    let mut bytes = header(3);
    bytes.push(height);
    bytes.extend_from_slice(&child.length.to_be_bytes());
    put_address(&mut bytes, &child.address);
    builder.writer.encoded(address(&bytes), bytes).await
}

async fn get<S: ObjectStore>(
    store: &mut S,
    document: DocumentId,
    address: &Address,
    budget: &mut Limits,
) -> Result<Vec<u8>, Error> {
    if budget.max_objects == 0 {
        return Err(Error::Limit("objects"));
    }
    let bytes = store.get(document, address).await?;
    budget.consume(bytes.len())?;
    Ok(bytes)
}

/// Verify and stream every byte in order. The sink may contain a verified prefix
/// on a later error. Use a temporary/staged sink and publish/apply only on success;
/// this function deliberately does not flush/close a caller-owned sink.
/// The byte budget counts encoded visits, including repeated chunk references.
pub async fn read_into<S: ObjectStore, W: AsyncWrite + Unpin + Send>(
    store: &mut S,
    document: DocumentId,
    root: &Address,
    output: &mut W,
    mut limits: Limits,
) -> Result<u64, Error> {
    read_with_budget(store, document, root, output, &mut limits).await
}

pub(super) async fn read_with_budget<S: ObjectStore, W: AsyncWrite + Unpin + Send>(
    store: &mut S,
    document: DocumentId,
    root: &Address,
    output: &mut W,
    limits: &mut Limits,
) -> Result<u64, Error> {
    let bytes = get(store, document, root, limits).await?;
    let Object::Root { height, child } = decode(root, &bytes)? else {
        return Err(Error::Invalid("expected content root"));
    };
    let total = child.length;
    if total > limits.max_bytes as u64 {
        return Err(Error::Limit("bytes"));
    }
    let mut stack = vec![(height, child)];
    while let Some((height, child)) = stack.pop() {
        let bytes = get(store, document, &child.address, limits).await?;
        match decode(&child.address, &bytes)? {
            Object::Chunk(data) if height == 0 && data.len() as u64 == child.length => {
                output.write_all(data).await?;
            }
            Object::Branch {
                height: actual,
                children,
            } if height > 0 && actual == height => {
                if children.iter().map(|entry| entry.length).sum::<u64>() != child.length {
                    return Err(Error::Invalid("content reference length"));
                }
                if stack.len() + children.len() > limits.max_objects {
                    return Err(Error::Limit("objects"));
                }
                stack.extend(children.into_iter().rev().map(|entry| (height - 1, entry)));
            }
            _ => return Err(Error::Invalid("content reference type, height or length")),
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_checkpoint::MemoryStore;
    use std::pin::Pin;
    use std::task::{Context as TaskContext, Poll};
    use tokio::io::ReadBuf;

    struct Fragmented<'a> {
        bytes: &'a [u8],
        maximum: usize,
    }
    impl AsyncRead for Fragmented<'_> {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _: &mut TaskContext<'_>,
            output: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            let n = self.bytes.len().min(output.remaining()).min(self.maximum);
            output.put_slice(&self.bytes[..n]);
            self.bytes = &self.bytes[n..];
            Poll::Ready(Ok(()))
        }
    }

    fn doc() -> DocumentId {
        DocumentId(uuid::Uuid::new_v4())
    }

    async fn roundtrip(bytes: &[u8]) -> (MemoryStore, DocumentId, Address, WriteStats) {
        let mut store = MemoryStore::default();
        let document = doc();
        let (root, stats) = write(&mut store, document, &mut &*bytes, Limits::default())
            .await
            .unwrap();
        let mut output = Vec::new();
        assert_eq!(
            read_into(&mut store, document, &root, &mut output, Limits::default())
                .await
                .unwrap(),
            bytes.len() as u64
        );
        assert_eq!(output, bytes);
        (store, document, root, stats)
    }

    #[tokio::test]
    async fn independent_hash_vectors_pin_the_binary_format() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/content-v1.json")).unwrap();
        for row in fixture["vectors"].as_array().unwrap() {
            let bytes = match row["name"].as_str().unwrap() {
                "empty" => Vec::new(),
                "abc" => b"abc".to_vec(),
                "full" => vec![b'a'; CHUNK_BYTES],
                "two_chunks" => {
                    let mut b = vec![b'a'; CHUNK_BYTES];
                    b.push(b'b');
                    b
                }
                _ => panic!("unexpected vector"),
            };
            let (store, document, root, _) = roundtrip(&bytes).await;
            assert_eq!(root.as_str(), row["root_hash"].as_str().unwrap());
            assert_eq!(bytes.len() as u64, row["length"].as_u64().unwrap());
            for field in ["leaf_hash", "branch_hash"] {
                if let Some(hash) = row[field].as_str() {
                    let encoded = &store.objects[&(document.0, Address(hash.to_owned()))];
                    assert_eq!(address(encoded).as_str(), hash);
                    if field == "branch_hash" {
                        assert_eq!(
                            super::super::hex(encoded),
                            row["branch_hex"].as_str().unwrap()
                        );
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn boundaries_and_short_reads_have_identical_roots() {
        for size in [
            0,
            1,
            CHUNK_BYTES - 1,
            CHUNK_BYTES,
            CHUNK_BYTES + 1,
            CHUNK_BYTES * FANOUT - 1,
            CHUNK_BYTES * FANOUT,
            CHUNK_BYTES * FANOUT + 1,
        ] {
            let data: Vec<u8> = (0..size)
                .map(|n| (n.wrapping_mul(131) >> 7) as u8)
                .collect();
            let (mut store, document, root, stats) = roundtrip(&data).await;
            let (again, reused) = write(
                &mut store,
                document,
                &mut Fragmented {
                    bytes: &data,
                    maximum: 731,
                },
                Limits::default(),
            )
            .await
            .unwrap();
            assert_eq!(root, again);
            assert_eq!(reused.new_objects, 0);
            assert_eq!(stats.visited_objects, reused.visited_objects);
            assert!(store
                .objects
                .values()
                .all(|bytes| bytes.len() <= HEADER + CHUNK_BYTES));
        }
    }

    #[tokio::test]
    async fn equal_length_edit_reuses_unaffected_content_and_scopes_documents() {
        let mut data = vec![b'a'; CHUNK_BYTES * 3];
        let (mut store, document, old, _) = roundtrip(&data).await;
        let before = store.stored_bytes();
        data[CHUNK_BYTES + 17] = b'z';
        let (new, stats) = write(
            &mut store,
            document,
            &mut data.as_slice(),
            Limits::default(),
        )
        .await
        .unwrap();
        assert_ne!(old, new);
        assert_eq!(stats.new_objects, 3); // one chunk, one branch, one root
        assert_eq!(
            store.stored_bytes() - before,
            CHUNK_BYTES + HEADER + 21 + 3 * 40 + 51
        );
        let mut output = Vec::new();
        read_into(&mut store, document, &old, &mut output, Limits::default())
            .await
            .unwrap();
        assert_eq!(output, vec![b'a'; data.len()]);
        let other = doc();
        assert!(matches!(
            read_into(
                &mut store,
                other,
                &new,
                &mut tokio::io::sink(),
                Limits::default()
            )
            .await,
            Err(Error::Missing(_))
        ));
        let (other_root, other_stats) =
            write(&mut store, other, &mut data.as_slice(), Limits::default())
                .await
                .unwrap();
        assert_eq!(new, other_root);
        assert_eq!(other_stats.new_objects, 4); // two distinct chunks, branch, root
    }

    #[tokio::test]
    async fn budgets_count_repeated_visits_and_fail_without_returning_a_root() {
        let data = vec![9; CHUNK_BYTES * 2];
        let (mut store, document, root, stats) = roundtrip(&data).await;
        assert_eq!(stats.visited_objects, 4);
        assert_eq!(stats.new_objects, 3);
        for limits in [
            Limits {
                max_objects: 3,
                max_bytes: usize::MAX,
            },
            Limits {
                max_objects: 4,
                max_bytes: stats.encoded_bytes - 1,
            },
        ] {
            assert!(matches!(
                write(&mut store, document, &mut data.as_slice(), limits).await,
                Err(Error::Limit(_))
            ));
            assert!(matches!(
                read_into(&mut store, document, &root, &mut tokio::io::sink(), limits).await,
                Err(Error::Limit(_))
            ));
        }
        let exact = Limits {
            max_objects: 4,
            max_bytes: stats.encoded_bytes,
        };
        read_into(&mut store, document, &root, &mut tokio::io::sink(), exact)
            .await
            .unwrap();
        let (again, _) = write(&mut store, document, &mut data.as_slice(), exact)
            .await
            .unwrap();
        assert_eq!(again, root);
    }

    fn store_raw(store: &mut MemoryStore, document: DocumentId, bytes: Vec<u8>) -> Address {
        let root = address(&bytes);
        store.objects.insert((document.0, root.clone()), bytes);
        root
    }

    #[tokio::test]
    async fn missing_corrupt_and_wrongly_typed_children_fail() {
        let (mut store, document, root, _) = roundtrip(b"abc").await;
        let bytes = store.objects[&(document.0, root.clone())].clone();
        let Object::Root { child, .. } = decode(&root, &bytes).unwrap() else {
            panic!()
        };
        let leaf = store
            .objects
            .remove(&(document.0, child.address.clone()))
            .unwrap();
        assert!(matches!(
            read_into(
                &mut store,
                document,
                &root,
                &mut tokio::io::sink(),
                Limits::default()
            )
            .await,
            Err(Error::Missing(_))
        ));
        store
            .objects
            .insert((document.0, child.address.clone()), b"corrupt".to_vec());
        assert!(read_into(
            &mut store,
            document,
            &root,
            &mut tokio::io::sink(),
            Limits::default()
        )
        .await
        .is_err());
        store
            .objects
            .insert((document.0, child.address.clone()), leaf);
        let mut bad_length = bytes.clone();
        bad_length[18] = 4;
        let bad = store_raw(&mut store, document, bad_length);
        assert!(matches!(
            read_into(
                &mut store,
                document,
                &bad,
                &mut tokio::io::sink(),
                Limits::default()
            )
            .await,
            Err(Error::Invalid(_))
        ));
        let mut wrong_type = bytes;
        wrong_type.truncate(19);
        put_address(&mut wrong_type, &root);
        let bad = store_raw(&mut store, document, wrong_type);
        assert!(matches!(
            read_into(
                &mut store,
                document,
                &bad,
                &mut tokio::io::sink(),
                Limits::default()
            )
            .await,
            Err(Error::Invalid(_))
        ));
    }

    #[tokio::test]
    async fn malformed_and_noncanonical_trees_are_rejected() {
        let (store, document, root, _) = roundtrip(&vec![1; CHUNK_BYTES + 1]).await;
        let root_bytes = &store.objects[&(document.0, root.clone())];
        let Object::Root { child, .. } = decode(&root, root_bytes).unwrap() else {
            panic!()
        };
        let branch = &store.objects[&(document.0, child.address)];
        for offset in [8, 9, 10, 18, 19, 20, 60] {
            let mut mutated = branch.clone();
            mutated[offset] = 255;
            assert!(
                validate(&address(&mutated), &mutated).is_err(),
                "offset {offset}"
            );
        }
        for len in [0, 1, 9, branch.len() - 1] {
            assert!(validate(&address(&branch[..len]), &branch[..len]).is_err());
        }
        let mut tall = root_bytes.clone();
        tall[10] += 1;
        assert!(validate(&address(&tall), &tall).is_err());
        let oversized = vec![0; HEADER + CHUNK_BYTES + 1];
        assert!(matches!(
            validate(&address(&oversized), &oversized),
            Err(Error::Limit(_))
        ));
        let mut wrong_hash = branch.clone();
        wrong_hash[21] ^= 1;
        assert!(matches!(
            validate(&address(branch), &wrong_hash),
            Err(Error::HashMismatch)
        ));
    }

    #[tokio::test]
    async fn input_and_output_failures_propagate() {
        struct Failed;
        impl AsyncRead for Failed {
            fn poll_read(
                self: Pin<&mut Self>,
                _: &mut TaskContext<'_>,
                _: &mut ReadBuf<'_>,
            ) -> Poll<std::io::Result<()>> {
                Poll::Ready(Err(std::io::Error::other("input failed")))
            }
        }
        assert!(matches!(
            write(
                &mut MemoryStore::default(),
                doc(),
                &mut Failed,
                Limits::default()
            )
            .await,
            Err(Error::Io(_))
        ));
        let (mut store, document, root, _) = roundtrip(b"abc").await;
        let (mut output, peer) = tokio::io::duplex(1);
        drop(peer);
        assert!(matches!(
            read_into(&mut store, document, &root, &mut output, Limits::default()).await,
            Err(Error::Io(_))
        ));
    }
}
