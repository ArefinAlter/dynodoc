# Encrypted signing key files: `dynodoc-key-v1`

14 October 2026. Source only: no release records this format yet. It changes the
local file `engine-cli` uses for a device signing key. It adds no PostgreSQL
migration, no server route and no enrollment change: an encrypted key has the same
Ed25519 public key as the unencrypted key it came from, so an enrolled key stays
enrolled and the server never sees the file.

Unencrypted DER PKCS#8 files remain valid everywhere. Every command that reads a
key file (`device public-key`, `device prove`, `sync --device-key`,
`repository commit`, `merge`, `recover`, `receipt sign`, `receipt sign-rotation`)
detects the format by content and decrypts transparently.

## File format

A UTF-8 JSON object, at most 16 KiB. Unknown fields, a wrong `format`, an unknown
algorithm name and any field outside the bounds below are rejected before any key
derivation runs.

```json
{
  "format": "dynodoc-key-v1",
  "public_key": "<64 lowercase hex characters: the Ed25519 public key>",
  "kdf": { "name": "pbkdf2-hmac-sha256", "iterations": 600000, "salt": "<base64url>" },
  "aead": { "name": "chacha20-poly1305", "nonce": "<base64url, 12 bytes>" },
  "keystore": { "provider": "windows-dpapi", "blob": "<base64url>" },
  "ciphertext": "<base64url: ChaCha20-Poly1305 ciphertext followed by the 16-byte tag>"
}
```

`keystore` is absent for a passphrase file. Base64url is RFC 4648 section 5 without
padding; a spelling that does not re-encode to itself is rejected, so one file has
one meaning.

| Field | Accepted |
|---|---|
| file size | at most 16,384 bytes |
| `iterations` | 600,000 to 5,000,000 inclusive |
| `salt` | 16 to 64 bytes (written files use 16) |
| `nonce` | exactly 12 bytes (random per file) |
| `ciphertext` | 17 to 4,112 bytes (the PKCS#8 plaintext is at most 4,096) |
| `keystore.provider` | `windows-dpapi`, `macos-keychain`, `linux-secret-service` |
| `keystore.blob` | required and at most 2,048 bytes for `windows-dpapi`; absent otherwise |

The iteration floor is 600,000 so a file cannot quietly weaken the key
derivation; the ceiling keeps a hostile file from holding the process. On the
development host (release build, one core) 600,000 iterations take about 0.22
seconds and the 5,000,000 ceiling about 1.8 seconds.

## Key, nonce and associated data

1. The wrapping secret is the passphrase as raw bytes (UTF-8 as typed or as read
   from the file; no Unicode normalization is applied, so use the same bytes every
   time), or for a keystore-bound file the 64 lowercase hex characters of a random
   256-bit secret held by the operating system.
2. `key = PBKDF2-HMAC-SHA256(secret, salt, iterations, 32 bytes)`.
3. `ciphertext = ChaCha20-Poly1305(key, nonce, aad, plaintext)` where `plaintext` is
   the unencrypted DER PKCS#8 Ed25519 private key and `aad` is the following
   ASCII text, exactly, joined by single `\n` characters with no trailing newline:

```text
dynodoc-key-v1
public_key=<the public_key field>
kdf=pbkdf2-hmac-sha256
iterations=<decimal iterations>
salt=<the salt field, as written>
aead=chacha20-poly1305
keystore=none | <provider> | <provider>:<blob field, as written>
```

The format identifier, the KDF parameters, the public-key fingerprint and the
keystore binding are therefore authenticated: editing any of them, or moving a
ciphertext under another public key or another binding, makes decryption fail.
The nonce is authenticated by the cipher itself.

After decryption the reader parses the plaintext as an Ed25519 PKCS#8 key and
requires its public half to equal `public_key`. There is no separate password
verifier: the AEAD tag check inside `ring` (constant time) is the only test, so a
wrong passphrase, a damaged file and a swapped field all give the same message,
`cannot decrypt the key file: wrong passphrase, or the file was changed`. Structural
problems (not JSON, wrong format, a bound exceeded) say
`not a valid dynodoc-key-v1 encrypted key file`. No message contains the
passphrase, the secret or any key byte.

The `public_key` field is readable without a passphrase but is **not**
authenticated until decryption succeeds. Treat it as a label, as `device
public-key` does by decrypting first.

## Keystore binding (optional)

`--keystore` replaces the typed passphrase by a random wrapping secret that the
operating system holds. The file records which provider, never the secret.

| Provider | Where the secret lives | Notes |
|---|---|---|
| `windows-dpapi` | Inside the file as `keystore.blob`: `CryptProtectData`, current user, UI forbidden, optional entropy `dynodoc-key-v1 NUL <public key hex>` | Decrypts only for the Windows account that protected it (and with the same machine/user DPAPI master key). A blob copied to another key file does not decrypt because of the entropy. Exercised for real on a Windows 11 host. |
| `macos-keychain` | Login keychain generic password, service `dynodoc-key-v1`, account the public key hex, through `/usr/bin/security -i` (commands on standard input, so the secret is not in the process arguments) | Implemented and unit-tested through command construction; **not run on a Mac**. |
| `linux-secret-service` | Secret Service through `secret-tool` (attributes `service` and `account`; the secret on standard input) | Implemented and unit-tested through command construction; **not run on Linux**. If `secret-tool` is missing the command fails and points to the passphrase mode. |

This binds the key to an operating-system account. It is not an additional factor
on top of a passphrase, and it is not a hardware token (see Residual risks).
`device unprotect` removes the macOS or Linux entry it created.

## Passphrase sources

Never an argument. In order: `--passphrase-file FILE`; the environment variable
`DYNODOC_KEY_PASSPHRASE_FILE` (a path, not the passphrase); a prompt with echo
off when standard input is a terminal. With none of these a command fails with a
message naming them (a CI or background process therefore never hangs on a
prompt). A passphrase file is read as bytes, one trailing line ending is removed,
nothing else is trimmed, it must be 1 to 1,024 bytes, and on Unix a file readable
by group or others is refused unless `--allow-insecure-key-file` is passed. A new
passphrase (`keygen --encrypt`, `protect`) must be at least 12 bytes; a prompt asks
twice. A named file that cannot be read is an error, never a silent fall back to a
prompt.

A decrypted key is kept in process memory for the life of one `engine-cli`
invocation (so a command that loads the same file twice asks once) and is not
written anywhere.

## Test vector

Generated with Python 3 `hashlib.pbkdf2_hmac` and the `cryptography` package's
`ChaCha20Poly1305` (OpenSSL), with no dynodoc code. The key is the public
synthetic seed `07` repeated 32 times, the same seed as the receipt signature
fixture; never enroll or trust it. The fixture file is
`crates/engine-core/tests/fixtures/dynodoc-key-v1.json`; the CLI test decrypts it,
and re-encrypts with its salt and nonce to the identical file.

```json
{
  "passphrase": "dynodoc-key-v1 public test vector passphrase",
  "pkcs8_hex": "302e020100300506032b6570042204200707070707070707070707070707070707070707070707070707070707070707",
  "public_key": "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c",
  "derived_key_hex": "fed7e9056f49dfd55abbf10eb2183376c22ed0ad4545fa71abff68c13e8e0d09",
  "file": {
    "format": "dynodoc-key-v1",
    "public_key": "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c",
    "kdf": { "name": "pbkdf2-hmac-sha256", "iterations": 600000, "salt": "oKGio6SlpqeoqaqrrK2urw" },
    "aead": { "name": "chacha20-poly1305", "nonce": "wMHCw8TFxsfIycrL" },
    "ciphertext": "KoOoXpSRGObuWGs3jfjJ81Gz6XvaTlnGdJig-n4NkIQJQbT9kUYV1nXQ59rLPYb_QoGPh3fmS74HrlKRNvuGXg"
  }
}
```

The independent generator, in full:

```python
import base64, hashlib
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305
from cryptography.hazmat.primitives import serialization

b64 = lambda b: base64.urlsafe_b64encode(b).rstrip(b"=").decode()
passphrase = "dynodoc-key-v1 public test vector passphrase"
seed = bytes([7]) * 32
pkcs8 = bytes.fromhex("302e020100300506032b657004220420") + seed
public = Ed25519PrivateKey.from_private_bytes(seed).public_key().public_bytes(
    serialization.Encoding.Raw, serialization.PublicFormat.Raw).hex()
iterations, salt, nonce = 600_000, bytes(range(0xA0, 0xB0)), bytes(range(0xC0, 0xCC))
key = hashlib.pbkdf2_hmac("sha256", passphrase.encode(), salt, iterations, 32)
aad = "\n".join(["dynodoc-key-v1", f"public_key={public}", "kdf=pbkdf2-hmac-sha256",
                 f"iterations={iterations}", f"salt={b64(salt)}",
                 "aead=chacha20-poly1305", "keystore=none"])
ciphertext = ChaCha20Poly1305(key).encrypt(nonce, pkcs8, aad.encode())
print(b64(ciphertext))
```

## Commands

```text
engine-cli device keygen --out KEY --encrypt [--keystore] [--passphrase-file FILE]
engine-cli device protect KEY [--out NEW] [--keystore] [--passphrase-file FILE]
engine-cli device unprotect KEY --i-understand [--out NEW] [--passphrase-file FILE]
engine-cli device public-key KEY [--passphrase-file FILE]
```

`keygen --encrypt` creates the file exclusively with the same owner-only protection
as an unencrypted key (mode 0600; on Windows a one-entry access list, verified).
`protect` encrypts an existing plain key: with `--out` it writes a new file and
never overwrites one; without it, it prepares a same-folder temporary file with
that protection first, then replaces the original atomically, so the file is
always either the whole old content or the whole new content. A narrower Unix mode
of the original (for example 0400) is kept and a wider one never is. The public key
printed is unchanged. `unprotect` is the reverse and refuses without
`--i-understand`. All three check, before changing anything, that what they write
decrypts to the same key.

## Residual risks, precisely

- **The strength is the passphrase.** A stolen encrypted file can be attacked
  offline. PBKDF2-HMAC-SHA256 at 600,000 iterations is deliberately slow but is
  not memory-hard, so GPUs and ASICs gain more against it than against Argon2id.
  Argon2id would need a new dependency; none was added because `ring` is already
  the only crypto dependency of the workspace. The `kdf.name` field makes a later
  `dynodoc-key-v2` or an added name possible without breaking v1 files. Use a long
  random passphrase (several random words); a short passphrase is weak whatever the
  iteration count.
- **Unlocked means exposed.** While a command runs, the decrypted key is in this
  process. Zeroisation is best effort: passphrase and PKCS#8 buffers are
  overwritten on drop, but `ring`'s key object, the terminal, the kernel and a
  swap file or core dump may hold copies. Malware running as the same user, or a
  keylogger, defeats a passphrase and also DPAPI/keychain/Secret Service, which
  answer to the user's own processes.
- **`protect` cannot erase history.** The old plaintext blocks of an in-place
  conversion may remain on the disk, a snapshot or a backup. If a key might have
  been copied, generate a new one and enroll it instead; a revoked key is
  permanent.
- **A passphrase file is a secret in a file.** `--passphrase-file` and
  `DYNODOC_KEY_PASSPHRASE_FILE` trade the prompt for a file the same account can
  read; keep it away from the key file and out of backups. On Windows its access
  list is not checked when read (only files this program creates are restricted).
- **No hardware or per-use confirmation.** The keystore binding is not a TPM,
  smart card or Secure Enclave key and does not ask the user before each use.
- **Prompt echo and interruption.** The hidden prompt turns terminal echo off and
  restores it; a process killed mid-prompt on Unix can leave echo off (`stty sane`).
- **Not done:** key rotation inside a file, multiple recipients, a key-agent
  process, an encrypted token file for the interim account sync mode, native host
  adoption, and running the macOS and Linux keystore paths on those systems.
