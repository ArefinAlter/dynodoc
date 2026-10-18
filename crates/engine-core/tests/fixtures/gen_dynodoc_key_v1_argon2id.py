"""Independent generator for dynodoc-key-v1-argon2id.json (no dynodoc code).

Needs the PyPI packages `argon2-cffi` (the reference C implementation of Argon2) and
`cryptography` (OpenSSL ChaCha20-Poly1305). Run from an empty scratch directory:

    python -m pip install --target DEPS argon2-cffi cryptography
    python -I gen_dynodoc_key_v1_argon2id.py DEPS > dynodoc-key-v1-argon2id.json

Every value is synthetic and public; the key must never be enrolled or trusted.
"""
import base64
import json
import sys

sys.path.insert(0, sys.argv[1])

from argon2.low_level import Type, hash_secret_raw
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305


def b64(b):
    return base64.urlsafe_b64encode(b).rstrip(b"=").decode()


passphrase = "dynodoc-key-v1 public test vector passphrase"
seed = bytes([7]) * 32
pkcs8 = bytes.fromhex("302e020100300506032b657004220420") + seed
public = (
    Ed25519PrivateKey.from_private_bytes(seed)
    .public_key()
    .public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw)
    .hex()
)
time_cost, memory_kib, lanes = 3, 65536, 4
salt, nonce = bytes(range(0xA0, 0xB0)), bytes(range(0xC0, 0xCC))
key = hash_secret_raw(
    passphrase.encode(),
    salt,
    time_cost=time_cost,
    memory_cost=memory_kib,
    parallelism=lanes,
    hash_len=32,
    type=Type.ID,
    version=19,
)
aad = "\n".join(
    [
        "dynodoc-key-v1",
        f"public_key={public}",
        "kdf=argon2id",
        f"iterations={time_cost}",
        f"memory_kib={memory_kib}",
        f"parallelism={lanes}",
        f"salt={b64(salt)}",
        "aead=chacha20-poly1305",
        "keystore=none",
    ]
)
ciphertext = ChaCha20Poly1305(key).encrypt(nonce, pkcs8, aad.encode())
print(
    json.dumps(
        {
            "description": "dynodoc-key-v1 Argon2id test vector. Every value is synthetic and public; the key must never be enrolled or trusted.",
            "generator": "Python 3 argon2-cffi (reference Argon2 C code, version 0x13) and cryptography ChaCha20Poly1305 (OpenSSL), no dynodoc code",
            "passphrase": passphrase,
            "pkcs8_hex": pkcs8.hex(),
            "public_key": public,
            "derived_key_hex": key.hex(),
            "aad": aad,
            "file": {
                "format": "dynodoc-key-v1",
                "public_key": public,
                "kdf": {
                    "name": "argon2id",
                    "iterations": time_cost,
                    "memory_kib": memory_kib,
                    "parallelism": lanes,
                    "salt": b64(salt),
                },
                "aead": {"name": "chacha20-poly1305", "nonce": b64(nonce)},
                "ciphertext": b64(ciphertext),
            },
        },
        indent=2,
    )
)
