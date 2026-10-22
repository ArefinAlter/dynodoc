//! A minimal ustar writer and reader (regular files only) with gzip, so a support bundle is one
//! standard `.tar.gz` that `tar -tzf` lists. Entry names are relative, `/`-separated and never
//! contain `..`.

use crate::error::{CliError, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::{Read, Write};

pub struct TarBuilder {
    buffer: Vec<u8>,
    mtime: u64,
}

fn octal(value: u64, width: usize) -> Vec<u8> {
    let text = format!("{value:0>w$o}", w = width - 1);
    let mut bytes = text.into_bytes();
    bytes.push(0);
    bytes
}

fn safe_name(name: &str) -> Result<()> {
    let bad = name.is_empty()
        || name.starts_with('/')
        || name.contains('\\')
        || name.split('/').any(|p| p == ".." || p.is_empty());
    if bad {
        return Err(CliError::failed(
            format!("`{name}` is not a safe name for an archive entry"),
            "this is a bug in dynodoctl; please report it",
        ));
    }
    Ok(())
}

impl TarBuilder {
    pub fn new(mtime: u64) -> Self {
        Self {
            buffer: Vec::new(),
            mtime,
        }
    }

    pub fn add_file(&mut self, name: &str, data: &[u8]) -> Result<()> {
        safe_name(name)?;
        let (prefix, short) = if name.len() <= 100 {
            (String::new(), name.to_string())
        } else {
            // Split at a '/' so that the prefix fits 155 bytes and the rest 100.
            let split = name
                .char_indices()
                .filter(|(i, c)| *c == '/' && *i <= 155 && name.len() - i - 1 <= 100)
                .map(|(i, _)| i)
                .next_back()
                .ok_or_else(|| {
                    CliError::failed(
                        format!("the archive name `{name}` is too long"),
                        "this is a bug in dynodoctl; please report it",
                    )
                })?;
            (name[..split].to_string(), name[split + 1..].to_string())
        };
        let mut header = [0u8; 512];
        header[..short.len()].copy_from_slice(short.as_bytes());
        header[100..108].copy_from_slice(&octal(0o600, 8));
        header[108..116].copy_from_slice(&octal(0, 8));
        header[116..124].copy_from_slice(&octal(0, 8));
        header[124..136].copy_from_slice(&octal(data.len() as u64, 12));
        header[136..148].copy_from_slice(&octal(self.mtime, 12));
        header[148..156].copy_from_slice(b"        ");
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        header[345..345 + prefix.len()].copy_from_slice(prefix.as_bytes());
        let sum: u32 = header.iter().map(|b| *b as u32).sum();
        let mut checksum = format!("{sum:06o}").into_bytes();
        checksum.push(0);
        checksum.push(b' ');
        header[148..156].copy_from_slice(&checksum);
        self.buffer.extend_from_slice(&header);
        self.buffer.extend_from_slice(data);
        let pad = (512 - data.len() % 512) % 512;
        self.buffer.extend(std::iter::repeat_n(0u8, pad));
        Ok(())
    }

    /// The finished archive, uncompressed.
    pub fn finish(mut self) -> Vec<u8> {
        self.buffer.extend(std::iter::repeat_n(0u8, 1024));
        self.buffer
    }

    /// The finished archive, gzip-compressed.
    pub fn finish_gz(self) -> Result<Vec<u8>> {
        let tar = self.finish();
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&tar)?;
        Ok(encoder.finish()?)
    }
}

/// The files of a tar archive (regular files only), for tests and for `support-bundle --verify`.
pub fn read_tar(mut data: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    let mut out = Vec::new();
    while data.len() >= 512 {
        let header = &data[..512];
        if header.iter().all(|b| *b == 0) {
            break;
        }
        let text = |range: std::ops::Range<usize>| {
            String::from_utf8_lossy(&header[range])
                .trim_end_matches('\0')
                .to_string()
        };
        let size = usize::from_str_radix(text(124..135).trim(), 8).map_err(|_| {
            CliError::failed(
                "damaged archive header",
                "the archive is not a dynodoctl bundle",
            )
        })?;
        let prefix = text(345..500);
        let name = if prefix.is_empty() {
            text(0..100)
        } else {
            format!("{prefix}/{}", text(0..100))
        };
        let end = 512 + size;
        if data.len() < end {
            return Err(CliError::failed(
                "truncated archive",
                "the archive is incomplete",
            ));
        }
        out.push((name, data[512..end].to_vec()));
        data = &data[(512 + size.div_ceil(512) * 512).min(data.len())..];
    }
    Ok(out)
}

pub fn read_tar_gz(data: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    let mut plain = Vec::new();
    GzDecoder::new(data).read_to_end(&mut plain)?;
    read_tar(&plain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_with_gzip() {
        let mut builder = TarBuilder::new(1_700_000_000);
        builder.add_file("a.txt", b"hello").unwrap();
        builder.add_file("dir/b.json", &vec![b'x'; 1500]).unwrap();
        builder.add_file("empty", b"").unwrap();
        let gz = builder.finish_gz().unwrap();
        assert_eq!(&gz[..2], &[0x1f, 0x8b]);
        let files = read_tar_gz(&gz).unwrap();
        assert_eq!(files.len(), 3);
        assert_eq!(files[0], ("a.txt".to_string(), b"hello".to_vec()));
        assert_eq!(files[1].1.len(), 1500);
        assert!(files[2].1.is_empty());
    }

    #[test]
    fn long_names_use_the_prefix_field() {
        let name = format!("{}/{}", "d".repeat(80), "f".repeat(60));
        let mut builder = TarBuilder::new(0);
        builder.add_file(&name, b"x").unwrap();
        let files = read_tar(&builder.finish()).unwrap();
        assert_eq!(files[0].0, name);
    }

    #[test]
    fn unsafe_names_are_refused() {
        let mut builder = TarBuilder::new(0);
        for bad in ["", "/abs", "../x", "a/../b", "a\\b", "a//b"] {
            assert!(builder.add_file(bad, b"x").is_err(), "{bad}");
        }
    }

    #[test]
    fn header_checksum_is_valid() {
        let mut builder = TarBuilder::new(5);
        builder.add_file("x", b"y").unwrap();
        let tar = builder.finish();
        let mut header = tar[..512].to_vec();
        let recorded =
            u32::from_str_radix(String::from_utf8_lossy(&header[148..154]).trim(), 8).unwrap();
        header[148..156].copy_from_slice(b"        ");
        assert_eq!(recorded, header.iter().map(|b| *b as u32).sum::<u32>());
    }
}
