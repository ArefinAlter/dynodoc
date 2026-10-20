#!/usr/bin/env python3
"""Independent generator for at-rest-v1.json (no dynodoc code).

Needs the PyPI package `cryptography` (OpenSSL AES-256-GCM and HKDF); everything else is
the Python standard library (`hmac`, `hashlib`, `struct`). Run from an empty scratch
directory:

    python -m pip install --target DEPS cryptography
    python -I gen_at_rest_vectors.py DEPS > at-rest-v1.json

The output is deterministic (no randomness, no clock, no environment in it), so running
it twice, or on another machine, gives the identical file. Every key, salt and nonce is
synthetic and public; none may ever protect real data.

Written from docs/END-TO-END-ENCRYPTION.md section 7.4 and docs/AT-REST-ENVELOPE.md, not
from the Rust module. The Rust tests (`tests/at_rest_vectors_test.rs` and the unit test in
`src/at_rest/envelope.rs`) reproduce every value below byte for byte. Never regenerate the
fixture to make a Rust check pass.

Cross-checks run on every execution (the script aborts if any fails):
  * HKDF-SHA-256 against RFC 5869 appendix A.1 test case 1;
  * AES-256-GCM against the McGrew-Viega GCM test case 14;
  * the `cryptography` HKDF against a second implementation written here from RFC 5869
    with `hmac`, for every derived key;
  * every ciphertext decrypts back to its plaintext.
"""
import hashlib
import hmac
import json
import struct
import sys

sys.path.insert(0, sys.argv[1])

from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
from cryptography.hazmat.primitives.kdf.hkdf import HKDF

MAGIC = b"DYNOENC\x00"
VERSION = 1
AAD_LABEL = b"dynodoc.at-rest.v1"
KEY_INFO_LABEL = b"dynodoc.at-rest.v1/aead+nonce"
SALT_LABEL = b"dynodoc.at-rest.v1/content-salt"
WRAP_LABEL = b"dynodoc.at-rest.v1/dek-wrap"
WRAP_VERSION = 1


def lp(data):
    """Length-framed field: u64 big-endian length, then the bytes."""
    return struct.pack(">Q", len(data)) + data


def hkdf_sha256(ikm, salt, info, length):
    return HKDF(algorithm=hashes.SHA256(), length=length, salt=salt, info=info).derive(ikm)


def hkdf_rfc5869(ikm, salt, info, length):
    """Second implementation straight from RFC 5869 section 2."""
    prk = hmac.new(salt, ikm, hashlib.sha256).digest()
    okm, block, counter = b"", b"", 1
    while len(okm) < length:
        block = hmac.new(prk, block + info + bytes([counter]), hashlib.sha256).digest()
        okm += block
        counter += 1
    return okm[:length]


# ---- known-answer checks of the primitives --------------------------------
rfc5869_1 = hkdf_sha256(
    bytes([0x0B]) * 22, bytes(range(0x00, 0x0D)), bytes(range(0xF0, 0xFA)), 42
)
assert rfc5869_1.hex() == (
    "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf"
    "34007208d5b887185865"
), "HKDF known answer"
kat = AESGCM(bytes(32)).encrypt(bytes(12), bytes(16), b"")
assert kat[:16].hex() == "cea7403d4d606b6e074ec5d3baf39d18", "GCM known answer"
assert kat[16:].hex() == "d0d1c8a799996bf0265b98b5d48ab919", "GCM known answer"


def pattern(length):
    return bytes((i * 7 + 3) & 0xFF for i in range(length))


INSTALLATION_A = bytes.fromhex("0123456789abcdef0123456789abcdef")
INSTALLATION_B = bytes.fromhex("fedcba9876543210fedcba9876543210")
PROJECT = bytes.fromhex("aaaabbbbccccddddeeeeffff00001111")
DEK = bytes(range(0x40, 0x60))
OTHER_DEK = bytes(range(0x60, 0x80))
KEKS = {1: bytes(range(0x80, 0xA0)), 2: bytes(range(0xA0, 0xC0))}

PLAINTEXT_HEX_LIMIT = 1024


def encode_envelope(case):
    dek = case.get("dek", DEK)
    salt = case["salt"]
    header = (
        MAGIC
        + bytes([VERSION])
        + struct.pack(">I", case["dek_id"])
        + struct.pack(">I", case["kek_version"])
        + salt
    )
    aad = (
        lp(AAD_LABEL)
        + lp(case["installation"])
        + lp(case["table"].encode())
        + lp(case["column"].encode())
        + lp(case["row_key"])
        + lp(bytes([VERSION]))
        + lp(struct.pack(">I", case["dek_id"]))
        + lp(struct.pack(">I", case["kek_version"]))
    )
    info = (
        lp(KEY_INFO_LABEL)
        + lp(case["table"].encode())
        + lp(case["column"].encode())
        + lp(case["row_key"])
    )
    okm = hkdf_sha256(dek, salt, info, 44)
    assert okm == hkdf_rfc5869(dek, salt, info, 44), "HKDF implementations disagree"
    key, nonce = okm[:32], okm[32:]
    sealed = AESGCM(key).encrypt(nonce, case["plaintext"], aad)
    ciphertext, tag = sealed[:-16], sealed[-16:]
    assert AESGCM(key).decrypt(nonce, sealed, aad) == case["plaintext"]
    envelope = header + ciphertext + tag
    out = {
        "name": case["name"],
        "installation_hex": case["installation"].hex(),
        "table": case["table"],
        "column": case["column"],
        "row_key_hex": case["row_key"].hex(),
        "dek_hex": dek.hex(),
        "dek_id": case["dek_id"],
        "kek_version": case["kek_version"],
        "salt_hex": salt.hex(),
    }
    plaintext = case["plaintext"]
    if len(plaintext) <= PLAINTEXT_HEX_LIMIT and not case.get("pattern"):
        out["plaintext_hex"] = plaintext.hex()
    else:
        out["plaintext_pattern"] = {"len": len(plaintext), "mul": 7, "add": 3}
        assert plaintext == pattern(len(plaintext))
    out["aad_hex"] = aad.hex()
    out["key_info_hex"] = info.hex()
    out["derived_key_hex"] = key.hex()
    out["derived_nonce_hex"] = nonce.hex()
    out["tag_hex"] = tag.hex()
    out["ciphertext_sha256"] = hashlib.sha256(ciphertext).hexdigest()
    if len(plaintext) <= PLAINTEXT_HEX_LIMIT:
        out["ciphertext_hex"] = ciphertext.hex()
        out["envelope_hex"] = envelope.hex()
    out["envelope_len"] = len(envelope)
    out["envelope_sha256"] = hashlib.sha256(envelope).hexdigest()
    return out, envelope


def salt_for(n):
    return bytes((i * 3 + n) & 0xFF for i in range(32))


ROW = b"doc-0001/addr-0001"
envelope_cases = [
    dict(name="empty", installation=INSTALLATION_A, table="checkpoint_object", column="bytes",
         row_key=ROW, dek_id=1, kek_version=1, salt=salt_for(0), plaintext=b""),
    dict(name="one_byte", installation=INSTALLATION_A, table="checkpoint_object", column="bytes",
         row_key=ROW, dek_id=1, kek_version=1, salt=salt_for(1), plaintext=b"\x00"),
    dict(name="block_15", installation=INSTALLATION_A, table="checkpoint_object", column="bytes",
         row_key=ROW, dek_id=1, kek_version=1, salt=salt_for(2), plaintext=pattern(15)),
    dict(name="block_16", installation=INSTALLATION_A, table="checkpoint_object", column="bytes",
         row_key=ROW, dek_id=1, kek_version=1, salt=salt_for(3), plaintext=pattern(16)),
    dict(name="block_17", installation=INSTALLATION_A, table="checkpoint_object", column="bytes",
         row_key=ROW, dek_id=1, kek_version=1, salt=salt_for(4), plaintext=pattern(17)),
    dict(name="text_utf8", installation=INSTALLATION_A, table="snapshot", column="state",
         row_key=PROJECT + b"/42", dek_id=2, kek_version=1, salt=salt_for(5),
         plaintext="Dynodoc – résumé ✓".encode("utf-8")),
    dict(name="binary_row_key_and_extreme_ids", installation=INSTALLATION_A,
         table="document_upload", column="content", row_key=bytes(range(256)),
         dek_id=0xFFFFFFFF, kek_version=0xFFFFFFFE, salt=salt_for(6), plaintext=pattern(40)),
    dict(name="zero_ids_other_dek", installation=INSTALLATION_A, table="provenance_bundle",
         column="body", row_key=b"bundle-1", dek_id=0, kek_version=0, salt=salt_for(7),
         dek=OTHER_DEK, plaintext=pattern(33)),
    dict(name="other_installation", installation=INSTALLATION_B, table="checkpoint_object",
         column="bytes", row_key=ROW, dek_id=1, kek_version=1, salt=salt_for(0), plaintext=b""),
    dict(name="maximum_names", installation=INSTALLATION_A, table="t" * 128, column="c" * 128,
         row_key=bytes([0xAB]) * 1024, dek_id=3, kek_version=2, salt=salt_for(8),
         plaintext=pattern(100)),
    dict(name="kilobyte", installation=INSTALLATION_A, table="workspace_draft",
         column="base_state", row_key=b"draft-9", dek_id=1, kek_version=1, salt=salt_for(9),
         plaintext=pattern(1024), pattern=True),
    dict(name="over_one_block_boundary", installation=INSTALLATION_A, table="checkpoint_object",
         column="bytes", row_key=ROW, dek_id=1, kek_version=1, salt=salt_for(10),
         plaintext=pattern(65537), pattern=True),
    dict(name="one_mebibyte", installation=INSTALLATION_A, table="document_upload",
         column="content", row_key=b"upload-1", dek_id=1, kek_version=1, salt=salt_for(11),
         plaintext=pattern(1 << 20), pattern=True),
]
envelopes, encoded = [], {}
for case in envelope_cases:
    entry, raw = encode_envelope(case)
    envelopes.append(entry)
    encoded[case["name"]] = raw

# ---- deterministic salts for content-addressed rows ------------------------
DOCUMENT_1 = bytes.fromhex("11111111222233334444555566667777")
DOCUMENT_2 = bytes.fromhex("88889999aaaabbbbccccddddeeeeffff")


def content_addressed(name, document, plaintext, address=None):
    if address is None:
        address = hashlib.sha256(plaintext).digest()
    salt = hmac.new(
        DEK,
        lp(SALT_LABEL) + lp(document) + lp(address) + lp(hashlib.sha256(plaintext).digest()),
        hashlib.sha256,
    ).digest()
    case = dict(
        name=name, installation=INSTALLATION_A, table="checkpoint_object", column="bytes",
        row_key=document + address, dek_id=1, kek_version=1, salt=salt, plaintext=plaintext,
    )
    entry, _ = encode_envelope(case)
    entry["document_hex"] = document.hex()
    entry["address_hex"] = address.hex()
    return entry


payload = b"checkpoint object payload"
content_addressed_vectors = [
    content_addressed("document_1", DOCUMENT_1, payload),
    content_addressed("same_address_other_document", DOCUMENT_2, payload),
    content_addressed("other_address", DOCUMENT_1, b"another payload"),
    content_addressed(
        "empty_document_text_address", b"", b"",
        address=hashlib.sha256(b"").hexdigest().encode("ascii"),
    ),
]

# ---- wrapped data keys ------------------------------------------------------
NIL = bytes(16)


def wrap(name, kek_version, project, purpose, nonce):
    context = (
        lp(WRAP_LABEL)
        + lp(INSTALLATION_A)
        + lp(project if project is not None else b"")
        + lp(purpose.encode())
    )
    aad = context + lp(bytes([WRAP_VERSION])) + lp(struct.pack(">I", kek_version))
    sealed = AESGCM(KEKS[kek_version]).encrypt(nonce, DEK, aad)
    assert AESGCM(KEKS[kek_version]).decrypt(nonce, sealed, aad) == DEK
    wrapped = bytes([WRAP_VERSION]) + nonce + sealed
    assert len(wrapped) == 61
    return {
        "name": name,
        "installation_hex": INSTALLATION_A.hex(),
        "project_hex": project.hex() if project is not None else None,
        "purpose": purpose,
        "kek_version": kek_version,
        "kek_hex": KEKS[kek_version].hex(),
        "dek_hex": DEK.hex(),
        "nonce_hex": nonce.hex(),
        "context_hex": context.hex(),
        "aad_hex": aad.hex(),
        "wrapped_hex": wrapped.hex(),
    }


wrapped_vectors = [
    wrap("project_data_kek1", 1, PROJECT, "data", bytes(range(0x10, 0x1C))),
    wrap("installation_scope_kek2", 2, None, "data", bytes(range(0x20, 0x2C))),
    wrap("project_backup_kek1", 1, PROJECT, "backup", bytes(range(0x30, 0x3C))),
    wrap("nil_project_kek2", 2, NIL, "data", bytes(range(0x40, 0x4C))),
]

# ---- strict-parsing rejects -------------------------------------------------
base = encoded["block_16"]
rejects = [
    {"name": "empty_input", "input_hex": "", "error": "truncated"},
    {"name": "seven_bytes", "input_hex": MAGIC[:7].hex(), "error": "truncated"},
    {"name": "magic_only", "input_hex": MAGIC.hex(), "error": "truncated"},
    {"name": "cut_inside_header", "input_hex": base[:30].hex(), "error": "truncated"},
    {"name": "one_byte_short_of_minimum", "input_hex": base[:64].hex(), "error": "truncated"},
    {"name": "sealed_object_magic", "input_hex": (b"DYNOSEAL" + base[8:]).hex(), "error": "bad_magic"},
    {"name": "wrong_last_magic_byte", "input_hex": (base[:7] + b"\x01" + base[8:]).hex(), "error": "bad_magic"},
    {"name": "json", "input_hex": (b'{"a":1}' + bytes(70)).hex(), "error": "bad_magic"},
    {"name": "version_0", "input_hex": (base[:8] + b"\x00" + base[9:]).hex(), "error": "unsupported_version", "version": 0},
    {"name": "version_2", "input_hex": (base[:8] + b"\x02" + base[9:]).hex(), "error": "unsupported_version", "version": 2},
    {"name": "version_255", "input_hex": (base[:8] + b"\xff" + base[9:]).hex(), "error": "unsupported_version", "version": 255},
]

# ---- KEK files --------------------------------------------------------------
kek_files = [
    {"name": "lf", "version": 1, "text": KEKS[1].hex() + "\n", "kek_hex": KEKS[1].hex()},
    {"name": "crlf", "version": 2, "text": KEKS[2].hex() + "\r\n", "kek_hex": KEKS[2].hex()},
    {"name": "no_line_ending", "version": 1, "text": KEKS[1].hex(), "kek_hex": KEKS[1].hex()},
]

fixture = {
    "format": "dynodoc at-rest envelope test vectors, v1",
    "note": "Synthetic public keys. Produced by gen_at_rest_vectors.py with no dynodoc code.",
    "constants": {
        "magic_hex": MAGIC.hex(),
        "version": VERSION,
        "header_len": 49,
        "tag_len": 16,
        "aad_label": AAD_LABEL.decode(),
        "key_info_label": KEY_INFO_LABEL.decode(),
        "salt_label": SALT_LABEL.decode(),
        "wrap_label": WRAP_LABEL.decode(),
        "wrapped_len": 61,
    },
    "primitives": {
        "hkdf_rfc5869_a1_okm_hex": rfc5869_1.hex(),
        "gcm_case14_ciphertext_hex": kat[:16].hex(),
        "gcm_case14_tag_hex": kat[16:].hex(),
    },
    "envelopes": envelopes,
    "content_addressed": content_addressed_vectors,
    "wrapped": wrapped_vectors,
    "rejects": rejects,
    "kek_files": kek_files,
}
sys.stdout.buffer.write((json.dumps(fixture, indent=2) + "\n").encode("ascii"))
