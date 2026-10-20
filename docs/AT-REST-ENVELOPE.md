# At-rest envelope: `DYNOENC`, key providers and the data-key cache

20 October 2026. Source only: a library module with tests and vectors. **Nothing in the
storage or API code uses it**, there is no migration, no `data_key` table, no store wrapper,
no backfill and no Vault or cloud KMS provider. This document describes what the module
does today; the design it implements is
[END-TO-END-ENCRYPTION.md](END-TO-END-ENCRYPTION.md) section 7 (Phase 0, tasks P0-2 and P0-3
in part).

Code: `crates/engine-core/src/at_rest/` (`envelope.rs`, `provider.rs`, `cache.rs`,
`secret.rs`). Tests: unit tests in each file, `crates/engine-core/tests/at_rest_vectors_test.rs`,
`at_rest_props_test.rs`. Vectors: `tests/fixtures/at-rest-v1.json` written by
`tests/fixtures/gen_at_rest_vectors.py`.

## What it is, and what it is not

A way to store a value so that reading the storage alone does not reveal it. A value is
encrypted under a key derived from a per-project data key (DEK); the DEK is kept only
wrapped by a key-encryption key (KEK) that the customer controls. **The running service
decrypts in memory to do its work.** This is service-decryptable protection of data at
rest. It is not end-to-end encryption, gives no protection against the service or anyone
with access to the running process, and must not be described as end-to-end.

Wording (claims policy, END-TO-END-ENCRYPTION.md section 3): "customer-managed encryption at
rest" is permitted only at level 1, which needs Phase 0 shipped and a passed restore drill.
Neither is true yet, so no public page may claim at-rest encryption on the strength of this
module. When it is claimed, it must say that the running service can still read content and
that it is not end-to-end. The magic `DYNOENC` differs from `DYNOSEAL` on purpose, so an
at-rest envelope is never mistaken for an end-to-end sealed object.

## Key hierarchy

```text
KEK  (a key provider: here a local file; later Vault Transit or a cloud KMS)
 |   wrap / unwrap, bound to installation + project + purpose + KEK version
 v
DEK  32 random bytes, one per project (one for non-project data), stored only wrapped
 |   HKDF-SHA-256, salt = the value's 32-byte salt,
 |   info = LP(label) || LP(table) || LP(column) || LP(row_key)      -> 44 bytes
 v
key (32) + nonce (12)   single-use, AES-256-GCM, one per stored value
```

A provider call happens once per DEK per cache lifetime, not once per row. Because every
value has its own key and nonce, the message-count limit of one AES-GCM key never applies,
and no nonce counter has to be coordinated between writers.

## Stored form

```text
offset  size  field
     0     8  magic        44 59 4e 4f 45 4e 43 00   ("DYNOENC" and a zero byte)
     8     1  version      0x01
     9     4  dek_id       u32 big-endian, the data_key row holding the wrapped DEK
    13     4  kek_version  u32 big-endian, the KEK version that wrapped that DEK when written
    17    32  salt
    49     n  ciphertext   same length as the plaintext
  49+n    16  tag          AES-256-GCM
```

Overhead is 65 bytes. An empty plaintext is a 65-byte envelope. `MAX_PLAINTEXT_BYTES` is
256 MiB; the parser refuses larger values before reading them.

**Associated data** (length-framed, `LP(x)` = `u64` big-endian length then bytes, the framing
`log.rs` uses):

```text
LP("dynodoc.at-rest.v1") || LP(installation uuid, 16 bytes) || LP(table) || LP(column)
  || LP(row_key) || LP(version) || LP(dek_id u32 BE) || LP(kek_version u32 BE)
```

`table` and `column` are 1 to 128 bytes, `row_key` 1 to 1024 bytes (the row's stable
identity; the same bytes on write and read). The table, column and row key are bound twice,
through the key derivation and through the AAD; the installation through the AAD only.

**Deterministic salt for content-addressed tables.** `PgStore::put_batch` compares stored
bytes with incoming bytes for an existing address, so a random salt would break the
comparison. `Salt::content_addressed(dek, document, address)` is
`HMAC-SHA-256(DEK, LP("dynodoc.at-rest.v1/salt") || LP(document) || LP(address))`;
`seal_content_addressed` uses it. The same key, document and address always give the same
envelope. It is safe only where the address determines the plaintext (the address is a hash
of it); the salt does not include the plaintext, so a caller that seals two different
plaintexts under one (key, document, address) repeats the AES key and nonce and leaks their
XOR. The caller must have verified the address first. Everything else uses `seal`, which
draws a random salt.

### Refinements of the design (7.4), and why

| Design says | Built | Why |
|---|---|---|
| `AAD = label \|\| installation \|\| table \|\| column \|\| row_key` | Every field length-framed, and `version`, `dek_id`, `kek_version` appended | Plain concatenation is ambiguous (table `ab` + column `c` equals table `a` + column `bc`). The header fields are not otherwise authenticated, so binding them makes a flipped bit in any header byte fail (`dek_id` is also checked against the key handle) |
| salt `= HMAC(DEK, "salt" \|\| document \|\| address)` | Length-framed fields and the label `dynodoc.at-rest.v1/salt` | Same ambiguity; a versioned label separates the use of the DEK as an HMAC key |
| `info = table, column, row identity` | `LP(label) \|\| LP(table) \|\| LP(column) \|\| LP(row_key)` | Same framing; the label ties the key derivation to this format version |
| Strict parsing of "trailing bytes" | Not separable; see below | The layout has no length field, as designed |

**No length field.** The ciphertext runs to the end of the stored value. A value cut or
extended after the header still parses and then fails authentication
(`Error::Authentication`); only a value too short to hold a header and a tag is a parse
error. Adding a length field would make cut and extended values separate parse errors but
changes the proposed layout; it is left for the owner (see Open questions).

## Parsing and errors

`Header::parse` (also the first step of `open`) refuses, each with its own error:

| Input | Error |
|---|---|
| fewer bytes than the magic, or than header plus tag (65) | `Truncated { len, min }` |
| other magic, including `DYNOSEAL` | `BadMagic` |
| other version | `UnsupportedVersion(v)` |
| more than header + 256 MiB + tag | `TooLarge { len, max }` |

`open` adds `DekMismatch` (the header names a different data key than the handle) and
`Authentication`, the single answer for a wrong key, a wrong context and any change to the
value, so the error is no oracle. Context and key construction have `EmptyField`,
`FieldTooLong`, `BadSaltLength`, `BadKeyLength`; sealing has `PlaintextTooLarge`. No message
contains key bytes or plaintext.

## Key providers

`KeyProvider` (synchronous; a network provider will need an asynchronous form or a blocking
adapter, not built):

| Method | Meaning |
|---|---|
| `wrap(&Dek, &WrapContext) -> Wrapped` | under the newest KEK version |
| `unwrap(&Wrapped, &WrapContext) -> Dek` | under the version recorded in `Wrapped`; old versions keep working |
| `rewrap(&Wrapped, &WrapContext) -> Wrapped` | default: unwrap then wrap; the KEK-rotation step, touches no row data |
| `describe() -> ProviderInfo` | provider, `key_ref`, newest KEK version, for audit |

`WrapContext` is the installation, the project (none for installation scope) and the
purpose (`data` or `backup`, as in `kms_key.purpose`). `WrapContext::to_bytes()` is its
canonical length-framed form, for providers that bind a context some other way (a KMS
`EncryptionContext`, a Vault `context`). `Wrapped` is the opaque provider bytes (16 to 4096,
the bound of the proposed `data_key.wrapped`) plus the KEK version (the proposed
`data_key.kek_version` is text; here it is a `u32`, decimal for the file provider).

`ProviderError` separates `Unavailable` (retry), `Denied` (permission or policy),
`NotFound` (no such KEK version or file) and `Corrupt` (malformed material, or a wrapped
key that fails authentication: damaged, or presented under another context).

**Wrapped form of the file and test providers**, 61 bytes:
`0x01 | nonce[12] | AES-256-GCM(KEK, nonce, aad, DEK)[32] | tag[16]` with
`aad = WrapContext::to_bytes() || LP(0x01) || LP(kek_version u32 BE)`. A wrapped DEK
therefore does not unwrap under another installation, project, purpose or KEK version, and
every byte is authenticated. The nonce is random (96 bits); a KEK version wraps a handful of
DEKs plus rewraps, far below the limit for random GCM nonces.

**`FileKeyProvider`** (the weakest provider, for evaluation and small sites: the KEK sits on
the same host as the service). One file per KEK version, `kek-<version>.key` in a directory
(`from_dir`; version a canonical decimal `u32`) or an explicit list (`from_files`). A file
holds exactly 64 lowercase hex digits and at most one line ending, at most 128 bytes; a
directory, a larger file or other text is refused unread or unparsed. On Unix a file readable
by its group or others is refused (`Denied`); on other systems the access list is not
checked (the same limit as `docs/KEY-FILE-FORMAT.md` states). Files are read once, when the
provider is built, and the bytes are wiped on drop; rotate by adding `kek-<n+1>.key` and
building the provider again. New wraps use the highest version. Key bytes are never in an
error, `Debug` or log text.

**`TestKeyProvider`** holds KEKs in memory and takes nonces from a counter, so the same call
sequence gives the same bytes. Never use it outside tests: the counter restarts with the
process.

**`DekCache`**: unwrapped keys as `Arc<DataKey>`, lifetime measured from insertion (default
15 minutes, `DEFAULT_TTL`) and not extended by use, at most `max_entries` (the oldest is
dropped), every call first removes every expired entry, so no call returns or counts a key
past its lifetime; a service must still call `purge_expired` from a timer so an idle cache
does not keep an expired key in memory. The clock is injected (`Clock`, `ManualClock` in
tests). `get_or_unwrap` is the read path of a store: cache, else provider, never cached on
failure. Nothing is written to disk.

`DataKey`/`Dek`/`Kek` overwrite their bytes when dropped (volatile writes and a compiler
fence, the pattern of `engine-cli`'s `passphrase::Secret`, repeated because `engine-cli`
depends on `engine-core`), print `<redacted>` in `Debug`, and are not `Clone`.

## Test vectors

`tests/fixtures/at-rest-v1.json` (13 envelopes including empty, block-boundary, 64 KiB and
1 MiB plaintexts, binary row keys, maximum field lengths and extreme ids; 4 deterministic-salt
envelopes; 4 wrapped DEKs; 11 rejected inputs; 3 KEK files) is written by
`gen_at_rest_vectors.py`, which uses the PyPI `cryptography` package (OpenSSL AES-GCM and
HKDF) and the standard library `hmac`/`hashlib` and no dynodoc code:

```text
python -m pip install --target DEPS cryptography
python -I gen_at_rest_vectors.py DEPS > at-rest-v1.json
```

The output is deterministic (running it twice gives the same bytes). It aborts unless HKDF
reproduces RFC 5869 test case 1, AES-256-GCM reproduces the McGrew-Viega test case 14, a
second HKDF written from RFC 5869 agrees for every derived key, and every ciphertext
decrypts. Both sides were written from the same design text, so the vectors catch
implementation mistakes, not a mistake in the design itself. The Rust tests reproduce every
ciphertext, tag, derived key and nonce, associated data, HKDF info, deterministic salt and
wrapped DEK byte for byte and decrypt the generator's own bytes. Never regenerate the
fixture to make a Rust check pass.

## What it protects and does not

From END-TO-END-ENCRYPTION.md 7.9, restated for what the library can support once a store
uses it:

| Adversary | Outcome |
|---|---|
| Thief of storage, snapshots or backups without the KEK | Cannot read covered values |
| DBA with SQL access to covered columns, without the KEK | Cannot read them |
| Operator with a shell on the application host, or a compromised application | **Not protected**: the DEK is in process memory and the provider answers to the service |
| Anyone who can call the provider as the service | **Not protected** |
| Customer revokes the KEK | After the cache lifetime the service cannot unwrap; covered data is unreadable (restore by re-enabling) |
| Compelled operator | Content producible with the provider's cooperation |

Limits of the library itself: `ring` keeps its own expanded copies of keys and cannot wipe
them, and swap files or core dumps can capture memory; the metadata of a value (which table,
the length, the envelope header, the data key id and KEK version) is not hidden; a deleted
KEK or lost backup key makes data unrecoverable; no FIPS-validated module is used; no
external cryptographic review has taken place.

## How a future `EnvelopeStore` will use it (not built)

```text
write(table, column, row_key, plaintext):
    key  = cache.get_or_unwrap(active dek_id, data_key.wrapped, wrap_ctx, provider)
    bytes = seal(key, Context::new(installation, table, column, row_key), plaintext)
            (seal_content_addressed for checkpoint_object, after checking address = hash)
    store bytes
read(table, column, row_key, bytes):
    if !has_magic(bytes): legacy plaintext row
    header = Header::parse(bytes);  key = cache.get_or_unwrap(header.dek_id, ...)
    plaintext = open(key, ctx, bytes);  then validate address and codec as today
```

The store, not this module, must supply: the `data_key` and `kms_key` tables (END-TO-END-ENCRYPTION.md
7.7), the choice of the active DEK, the migration of existing rows (7.8), the rewrap and
re-encryption jobs, the kill switch and crypto-erase flows, and the audit records.

## Not built

`EnvelopeStore` or any use by `PgStore`, snapshots, uploads, bundles or drafts; migrations
and `data_key`/`kms_key` tables; backfill and `engine-cli kms` commands; Vault Transit, AWS,
Azure, Google and PKCS#11 providers; an asynchronous provider interface; observability and
failure injection; encrypted backups (tier 0a); the restore drill; tier 0c (JSON columns);
public wording or configuration. The proposed configuration in 7.7 is not parsed anywhere.

## Open questions for the owner

1. **Trailing bytes.** Add a ciphertext length field to the header (a format change before
   anything is stored) so cut and extended values are distinct parse errors, or keep the
   proposed layout and treat them as authentication failures (built)?
2. **Misuse resistance of the deterministic salt.** Built as designed (no plaintext in the
   salt). Hashing the plaintext into the salt input too would make a caller that wrongly
   reuses an address harmless (different plaintext, different salt) at the cost of one
   SHA-256 over the plaintext; equal plaintexts would still give equal bytes.
3. **Header `kek_version` is informational and authenticated.** After a KEK rewrap the
   `data_key` row is newer than the envelopes' field, and nothing uses the field to select a
   key. Keep it as designed, or drop it from the header?
4. **Maximum plaintext of 256 MiB** and name limits (128 and 1024 bytes) are choices of this
   module, not of the design.
