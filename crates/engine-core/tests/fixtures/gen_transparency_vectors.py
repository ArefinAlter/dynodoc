#!/usr/bin/env python3
"""Independent generator for transparency-v1.json (Python standard library only).

Run:  python -I gen_transparency_vectors.py      (rewrites transparency-v1.json
next to this file; the output is deterministic, so a clean `git diff` afterwards
shows the committed file is reproduced.)

Nothing here shares code with the Rust module under test
(crates/engine-core/src/transparency). It is written from the RFC 6962 recursive
definitions, RFC 9162 verification, RFC 8032 Ed25519, and the C2SP signed-note,
tlog-checkpoint, tlog-cosignature and tlog-tiles specifications as the author
remembered them; no published text was available when this was written. The
points that could not be checked against the published text are marked
"unverified" below and in the Rust rustdoc. Because both sides were written from
the same recollection, the vectors catch implementation bugs, not a shared
misreading of a standard.

Cross-checks run on every execution (the script aborts if any fails):
  * Ed25519 against the RFC 8032 section 7.1 test vectors;
  * the Merkle code against the eight published RFC 6962 / Certificate
    Transparency test roots (leaves "", 00, 10, 2021, 3031, 40..43, 50..57,
    60..6f), recalled from memory;
  * every proof this script emits against the RFC 9162 verification algorithms;
  * the signed-note key id and signature line against the well-known Go
    golang.org/x/mod/sumdb/note example (key PeterNeumann, id c74f20a3), recalled
    from memory; the example's vkey string was not recalled exactly, so only the
    key id and the signature line are asserted (both reproduce).
"""
import base64
import hashlib
import hmac
import json
import os
import struct
import sys
import uuid

# ---- Ed25519 (RFC 8032 section 5.1, pure Python) ---------------------------
P = 2**255 - 19
Q = 2**252 + 27742317777372353535851937790883648493
D = -121665 * pow(121666, P - 2, P) % P
SQRT_M1 = pow(2, (P - 1) // 4, P)


def inv(x):
    return pow(x, P - 2, P)


def recover_x(y, sign):
    if y >= P:
        return None
    x2 = (y * y - 1) * inv(D * y * y + 1) % P
    if x2 == 0:
        return None if sign else 0
    x = pow(x2, (P + 3) // 8, P)
    if (x * x - x2) % P != 0:
        x = x * SQRT_M1 % P
    if (x * x - x2) % P != 0:
        return None
    if (x & 1) != sign:
        x = P - x
    return x


G_Y = 4 * inv(5) % P
G_X = recover_x(G_Y, 0)
G = (G_X, G_Y, 1, G_X * G_Y % P)


def point_add(a, b):
    A = (a[1] - a[0]) * (b[1] - b[0]) % P
    B = (a[1] + a[0]) * (b[1] + b[0]) % P
    C = 2 * a[3] * b[3] * D % P
    DD = 2 * a[2] * b[2] % P
    E, F, GG, H = B - A, DD - C, DD + C, B + A
    return (E * F % P, GG * H % P, F * GG % P, E * H % P)


def point_mul(s, point):
    q = (0, 1, 1, 0)
    while s > 0:
        if s & 1:
            q = point_add(q, point)
        point = point_add(point, point)
        s >>= 1
    return q


def point_equal(a, b):
    if (a[0] * b[2] - b[0] * a[2]) % P != 0:
        return False
    return (a[1] * b[2] - b[1] * a[2]) % P == 0


def point_compress(point):
    zinv = inv(point[2])
    x, y = point[0] * zinv % P, point[1] * zinv % P
    return int.to_bytes(y | ((x & 1) << 255), 32, "little")


def point_decompress(data):
    if len(data) != 32:
        return None
    y = int.from_bytes(data, "little")
    sign = y >> 255
    y &= (1 << 255) - 1
    x = recover_x(y, sign)
    if x is None:
        return None
    return (x, y, 1, x * y % P)


def sha512(data):
    return hashlib.sha512(data).digest()


def sha512_modq(data):
    return int.from_bytes(sha512(data), "little") % Q


def secret_expand(secret):
    assert len(secret) == 32
    h = sha512(secret)
    a = int.from_bytes(h[:32], "little")
    a &= (1 << 254) - 8
    a |= 1 << 254
    return a, h[32:]


def ed_public(secret):
    a, _ = secret_expand(secret)
    return point_compress(point_mul(a, G))


def ed_sign(secret, msg):
    a, prefix = secret_expand(secret)
    public = point_compress(point_mul(a, G))
    r = sha512_modq(prefix + msg)
    big_r = point_compress(point_mul(r, G))
    h = sha512_modq(big_r + public + msg)
    s = (r + h * a) % Q
    return big_r + int.to_bytes(s, 32, "little")


def ed_verify(public, msg, signature):
    if len(public) != 32 or len(signature) != 64:
        return False
    a = point_decompress(public)
    r = point_decompress(signature[:32])
    if a is None or r is None:
        return False
    s = int.from_bytes(signature[32:], "little")
    if s >= Q:
        return False
    h = sha512_modq(signature[:32] + public + msg)
    return point_equal(point_mul(s, G), point_add(r, point_mul(h, a)))


RFC8032_VECTORS = [
    # (secret, public, message, signature) from RFC 8032 section 7.1, recalled from memory.
    (
        "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
        "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
        "",
        "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155"
        "5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
    ),
    (
        "4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb",
        "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c",
        "72",
        "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da"
        "085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00",
    ),
    (
        "c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7",
        "fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025",
        "af82",
        "6291d657deec24024827e69c3abe01a30ce548a284743a445e3680d7db5ac3ac"
        "18ff9b538d16f290ae67f760984dc6594a7c15e9716ed28dc027beceea1ec40a",
    ),
]


def check_ed25519():
    checked = 0
    for secret, public, message, signature in RFC8032_VECTORS:
        secret, message, signature = map(bytes.fromhex, (secret, message, signature))
        got_public = ed_public(secret)
        got_signature = ed_sign(secret, message)
        if got_public.hex() != public:
            raise SystemExit("RFC 8032 public key mismatch for " + secret.hex()[:8])
        if got_signature != signature:
            raise SystemExit("RFC 8032 signature mismatch for " + secret.hex()[:8])
        if not ed_verify(got_public, message, signature):
            raise SystemExit("RFC 8032 verification failed for " + secret.hex()[:8])
        if ed_verify(got_public, message + b"x", signature):
            raise SystemExit("verification accepted a different message")
        checked += 1
    return checked


# ---- Merkle tree (RFC 6962 section 2.1, recursive definitions) -------------
def h256(data):
    return hashlib.sha256(data).digest()


def leaf_hash(entry):
    return h256(b"\x00" + entry)


def node_hash(left, right):
    return h256(b"\x01" + left + right)


def split(n):
    """Largest power of two strictly below n (n >= 2)."""
    k = 1
    while k * 2 < n:
        k *= 2
    return k


def mth(hs):
    """Merkle tree hash of a list of leaf hashes."""
    n = len(hs)
    if n == 0:
        return h256(b"")
    if n == 1:
        return hs[0]
    k = split(n)
    return node_hash(mth(hs[:k]), mth(hs[k:]))


def audit_path(m, hs):
    """RFC 6962 PATH(m, D[n])."""
    n = len(hs)
    if n == 1:
        return []
    k = split(n)
    if m < k:
        return audit_path(m, hs[:k]) + [mth(hs[k:])]
    return audit_path(m - k, hs[k:]) + [mth(hs[:k])]


def subproof(m, hs, complete):
    """RFC 6962 SUBPROOF(m, D[n], b)."""
    n = len(hs)
    if m == n:
        return [] if complete else [mth(hs)]
    k = split(n)
    if m <= k:
        return subproof(m, hs[:k], complete) + [mth(hs[k:])]
    return subproof(m - k, hs[k:], False) + [mth(hs[:k])]


def consistency_proof(m, hs):
    """RFC 6962 PROOF(m, D[n]); empty for m == 0 (an empty tree is a prefix of all)."""
    return [] if m == 0 else subproof(m, hs, True)


def verify_inclusion(leaf, index, size, path, root):
    """RFC 9162 section 2.1.3.2."""
    if index >= size:
        return False
    fn, sn, r = index, size - 1, leaf
    for p in path:
        if sn == 0:
            return False
        if fn & 1 or fn == sn:
            r = node_hash(p, r)
            if not fn & 1:
                while not fn & 1 and fn != 0:
                    fn >>= 1
                    sn >>= 1
        else:
            r = node_hash(r, p)
        fn >>= 1
        sn >>= 1
    return sn == 0 and r == root


def verify_consistency(first, second, root1, root2, proof):
    """RFC 9162 section 2.1.4.2."""
    if first > second:
        return False
    if first == second:
        return not proof and root1 == root2
    if first == 0:
        return not proof and root1 == h256(b"")
    path = ([root1] if first & (first - 1) == 0 else []) + list(proof)
    if not path:
        return False
    fn, sn = first - 1, second - 1
    while fn & 1:
        fn >>= 1
        sn >>= 1
    fr = sr = path[0]
    for c in path[1:]:
        if sn == 0:
            return False
        if fn & 1 or fn == sn:
            fr = node_hash(c, fr)
            sr = node_hash(c, sr)
            if not fn & 1:
                while not fn & 1 and fn != 0:
                    fn >>= 1
                    sn >>= 1
        else:
            sr = node_hash(sr, c)
        fn >>= 1
        sn >>= 1
    return sn == 0 and fr == root1 and sr == root2


def check_rfc6962_roots():
    leaves = [
        b"", b"\x00", b"\x10", b"\x20\x21", b"\x30\x31", b"\x40\x41\x42\x43",
        bytes(range(0x50, 0x58)), bytes(range(0x60, 0x70)),
    ]
    roots = [
        "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d",
        "fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125",
        "aeb6bcfe274b70a14fb067a5e5578264db0fa9b51af5e0ba159158f329e06e77",
        "d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
        "4e3bbb1f7b478dcfe71fb631631519a3bca12c9aefca1612bfce4c13a86264d4",
        "76e67dadbcdf1e10e1b74ddc608abd2f98dfb16fbce75277b5232a127f2087ef",
        "ddb89be403809e325750d3d263cd78929c2942b7942a34b77e122c9594a74c8c",
        "5dc9da79a70659a9ad559cb701ded9a2ab9d823aad2f4960cfe370eff4604328",
    ]
    hs = [leaf_hash(x) for x in leaves]
    for n, expected in enumerate(roots, start=1):
        if mth(hs[:n]).hex() != expected:
            raise SystemExit("RFC 6962 published root mismatch at size %d" % n)
    if mth([]).hex() != "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855":
        raise SystemExit("empty root mismatch")
    return len(roots)


# ---- Encodings --------------------------------------------------------------
def b64(data):
    return base64.b64encode(data).decode("ascii")


def canon(obj):
    """Canonical JSON: sorted keys, no whitespace, UTF-8 (not ASCII-escaped)."""
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


# ---- signed note, checkpoint, cosignature (C2SP; unverified, see header) -----
DASH = "— "
TYPE_LOG = 0x01
TYPE_COSIGNATURE = 0x04  # unverified: check at build time


def key_id(name, typ, public):
    return h256(name.encode() + b"\n" + bytes([typ]) + public)[:4]


def vkey(name, typ, public):
    return "%s+%s+%s" % (name, key_id(name, typ, public).hex(), b64(bytes([typ]) + public))


def sig_line(name, kid, body):
    return "%s%s %s\n" % (DASH, name, b64(kid + body))


def log_line(name, seed, text):
    public = ed_public(seed)
    return sig_line(name, key_id(name, TYPE_LOG, public), ed_sign(seed, text.encode()))


def cosign_body(seed, text, time):
    message = ("cosignature/v1\ntime %d\n" % time + text).encode()  # unverified layout
    return struct.pack(">Q", time) + ed_sign(seed, message)


def cosign_line(name, seed, text, time):
    public = ed_public(seed)
    return sig_line(name, key_id(name, TYPE_COSIGNATURE, public), cosign_body(seed, text, time))


def checkpoint_text(origin, size, root):
    return "%s\n%d\n%s\n" % (origin, size, b64(root))


def make_note(text, lines):
    return text + "\n" + "".join(lines)


def check_signed_note_example():
    """The Go golang.org/x/mod/sumdb/note example, recalled from memory: the key id
    c74f20a3 and the signature line must both reproduce."""
    skey = "PRIVATE+KEY+PeterNeumann+c74f20a3+AYEKFALVFGyNhPJEMzD1QIDr+Y7hfZx09iUvxdXHKDFz"
    raw = base64.b64decode(skey.split("+", 4)[4])
    assert raw[0] == TYPE_LOG and len(raw) == 33
    seed = raw[1:]
    kid = key_id("PeterNeumann", TYPE_LOG, ed_public(seed))
    if kid.hex() != "c74f20a3":
        raise SystemExit("signed-note key id example mismatch")
    text = (
        "If you think cryptography is the answer to your problem,\n"
        "then you don't know what your problem is.\n"
    )
    expected = (
        "— PeterNeumann x08go/ZJkuBS9UG/SffcvIAQxVBtiFupLLr8pAcElZInNIuGUgYN1FFYC2pZSN"
        "XgKvqfqdngotpRZb6KE6RyyBwJnAM=\n"
    )
    if log_line("PeterNeumann", seed, text) != expected:
        raise SystemExit("signed-note signature line example mismatch")
    return True


# ---- tlog-tiles (height 8; unverified, see header) ---------------------------
def tile_index_path(n):
    digits = str(n)
    digits = "0" * ((3 - len(digits) % 3) % 3) + digits
    groups = [digits[i:i + 3] for i in range(0, len(digits), 3)]
    return "/".join(["x" + g for g in groups[:-1]] + [groups[-1]])


def tile_suffix(count):
    return "" if count == 256 else ".p/%d" % count


def tile_files(entries, hs):
    size = len(hs)
    files = {}
    level = 0
    while size >> (8 * level) > 0:
        count = size >> (8 * level)
        span = 1 << (8 * level)
        nodes = [mth(hs[j * span:(j + 1) * span]) for j in range(count)]
        for tile in range((count + 255) // 256):
            chunk = nodes[tile * 256:(tile + 1) * 256]
            path = "tile/%d/%s%s" % (level, tile_index_path(tile), tile_suffix(len(chunk)))
            files[path] = b"".join(chunk)
        level += 1
    for bundle in range((size + 255) // 256):
        chunk = entries[bundle * 256:(bundle + 1) * 256]
        path = "tile/entries/%s%s" % (tile_index_path(bundle), tile_suffix(len(chunk)))
        files[path] = b"".join(struct.pack(">H", len(e)) + e for e in chunk)
    return files


# ---- Leaves and tags (design 4.1) --------------------------------------------
LEAF_DOMAIN = b"dynodoc.transparency-leaf\x00v1\x00"
LOG_ORIGIN = "log.dynodoc.example/dynodoc/v1/transparency"
OLD_ORIGIN = "old.dynodoc.example/dynodoc/v1/transparency"
OTHER_ORIGIN = "other.dynodoc.example/dynodoc/v1/transparency"
K_PROJECT = bytes.fromhex("31" * 32)
K_INSTALLATION = bytes.fromhex("32" * 32)
DOC_UUID = uuid.UUID("00000000-0000-0000-0000-00000000d0c1")


def tag(key, message):
    return hmac.new(key, message, hashlib.sha256).digest()


def ref_tag(branch, generation):
    return tag(K_PROJECT, b"ref\x00" + branch.encode() + b"\x00" + struct.pack(">Q", generation))


def test_digest(label, i=None):
    return h256(b"dynodoc.test-" + label.encode() + (b"" if i is None else struct.pack(">Q", i)))


def leaf_record(inputs):
    """inputs -> (record for the fixture, leaf entry bytes)."""
    kind = inputs["type"]
    if kind == "ref":
        t = ref_tag(inputs["branch"], inputs["generation"])
        data = {"t": "ref", "tag": t.hex(), "receipt": inputs["receipt"]}
    elif kind == "rot":
        t = tag(K_INSTALLATION, b"rot")
        data = {"t": "rot", "tag": t.hex(), "rotation": inputs["rotation"]}
    elif kind == "hzn":
        t = tag(K_PROJECT, b"hzn\x00" + struct.pack(">Q", inputs["seq"]))
        data = {"t": "hzn", "tag": t.hex(), "horizon": inputs["horizon"], "seq": inputs["seq"]}
    elif kind == "evh":
        t = None
        data = {"t": "evh", "root": inputs["root"], "count": inputs["count"],
                "interval": inputs["interval"]}
    elif kind == "ver":
        t = tag(K_PROJECT, b"ver\x00" + uuid.UUID(inputs["document"]).bytes)
        data = {"t": "ver", "tag": t.hex(), "version": inputs["version"]}
    elif kind == "suc":
        t = None
        data = {"t": "suc", "prev_origin": inputs["prev_origin"], "size": inputs["size"],
                "root": inputs["root"]}
    else:
        raise SystemExit("unknown leaf type")
    text = canon(data)
    assert 1 <= len(text.encode()) <= 256
    entry = LEAF_DOMAIN + text.encode()
    record = dict(inputs)
    record.update({"tag": None if t is None else t.hex(), "data": text,
                   "leaf_hash": leaf_hash(entry).hex()})
    return record, entry


def ref_inputs(i):
    return {"type": "ref", "branch": "main", "generation": i + 1,
            "receipt": test_digest("receipt", i).hex()}


def tree_leaf_inputs():
    """The 64 leaves of the tree vectors: the six leaf types, then ref leaves."""
    inputs = [
        ref_inputs(0),
        {"type": "rot", "rotation": test_digest("rotation").hex()},
        {"type": "hzn", "seq": 1, "horizon": test_digest("horizon").hex()},
        {"type": "evh", "root": test_digest("event-heads").hex(), "count": 42, "interval": 3},
        {"type": "ver", "document": str(DOC_UUID), "version": test_digest("version").hex()},
        {"type": "suc", "prev_origin": OLD_ORIGIN, "size": 1000,
         "root": test_digest("successor").hex()},
    ]
    inputs += [ref_inputs(i) for i in range(6, 64)]
    return inputs


# ---- Keys (public synthetic seeds, hard-coded) -------------------------------
LOG_SEED = bytes.fromhex("21" * 32)
ROGUE_SEED = bytes.fromhex("25" * 32)
WITNESSES = [
    ("w1.verne.example/w", "verne", bytes.fromhex("22" * 32)),
    ("w2.uni-a.example/w", "uni-a", bytes.fromhex("23" * 32)),
    ("w3.customer.example/w", "customer", bytes.fromhex("24" * 32)),
]
T0 = 1760000000
FRESHNESS = 3600
NOW = T0 + 150


def policy_object(operators):
    return {
        "version": 1,
        "log": {"origin": LOG_ORIGIN, "vkey": vkey(LOG_ORIGIN, TYPE_LOG, ed_public(LOG_SEED))},
        "witnesses": [
            {"name": name, "vkey": vkey(name, TYPE_COSIGNATURE, ed_public(seed)), "operator": op}
            for (name, _, seed), op in zip(WITNESSES, operators)
        ],
        "quorum": {"threshold": 2, "distinct_operators": True},
        "freshness": {"max_age_seconds": FRESHNESS},
        "on_failure": "warn",
    }


def cosigned(text, which, times):
    lines = [log_line(LOG_ORIGIN, LOG_SEED, text)]
    for i in which:
        name, _, seed = WITNESSES[i]
        lines.append(cosign_line(name, seed, text, times[i]))
    return make_note(text, lines)


def bundle_object(leaf_record_, index, size, path, note):
    return {
        "version": 1,
        "leaf": json.loads(leaf_record_["data"]),
        "index": index,
        "size": size,
        "path": [p.hex() for p in path],
        "checkpoint": note,
    }


# ---- Output ------------------------------------------------------------------
def emit(obj, depth=0):
    """Readable but diff-friendly JSON: dicts and lists of dicts expand to one
    entry per line down to depth 2, everything else is compact."""
    compact = lambda v: json.dumps(v, ensure_ascii=False, separators=(",", ":"))
    pad = "  " * (depth + 1)
    end = "  " * depth
    if isinstance(obj, dict) and depth < 2:
        body = ",\n".join("%s%s: %s" % (pad, json.dumps(k), emit(v, depth + 1)) for k, v in obj.items())
        return "{\n%s\n%s}" % (body, end)
    if isinstance(obj, list) and depth < 2 and obj and all(isinstance(v, dict) for v in obj):
        body = ",\n".join(pad + compact(v) for v in obj)
        return "[\n%s\n%s]" % (body, end)
    return compact(obj)


def main():
    rfc8032 = check_ed25519()
    rfc6962 = check_rfc6962_roots()
    note_example = check_signed_note_example()

    log_public = ed_public(LOG_SEED)
    keys = {
        "log": {"name": LOG_ORIGIN, "seed": LOG_SEED.hex(), "public": log_public.hex(),
                "key_id": key_id(LOG_ORIGIN, TYPE_LOG, log_public).hex(),
                "vkey": vkey(LOG_ORIGIN, TYPE_LOG, log_public)},
        "rogue_log": {"name": LOG_ORIGIN, "seed": ROGUE_SEED.hex(),
                      "public": ed_public(ROGUE_SEED).hex(),
                      "key_id": key_id(LOG_ORIGIN, TYPE_LOG, ed_public(ROGUE_SEED)).hex()},
        "witnesses": [
            {"name": name, "operator": op, "seed": seed.hex(), "public": ed_public(seed).hex(),
             "key_id": key_id(name, TYPE_COSIGNATURE, ed_public(seed)).hex(),
             "vkey": vkey(name, TYPE_COSIGNATURE, ed_public(seed))}
            for name, op, seed in WITNESSES
        ],
        "k_project": K_PROJECT.hex(),
        "k_installation": K_INSTALLATION.hex(),
    }

    # Tags.
    tags = [
        {"kind": "ref", "key": "k_project", "branch": "main", "generation": 7,
         "tag": ref_tag("main", 7).hex()},
        {"kind": "ref", "key": "k_project", "branch": "feature/x", "generation": 1,
         "tag": ref_tag("feature/x", 1).hex()},
        {"kind": "rot", "key": "k_installation", "tag": tag(K_INSTALLATION, b"rot").hex()},
        {"kind": "hzn", "key": "k_project", "seq": 9,
         "tag": tag(K_PROJECT, b"hzn\x00" + struct.pack(">Q", 9)).hex()},
        {"kind": "ver", "key": "k_project", "document": str(DOC_UUID),
         "tag": tag(K_PROJECT, b"ver\x00" + DOC_UUID.bytes).hex()},
    ]

    # The 64-leaf tree.
    records, entries = [], []
    for inputs in tree_leaf_inputs():
        record, entry = leaf_record(inputs)
        records.append(record)
        entries.append(entry)
    hs = [leaf_hash(e) for e in entries]
    roots = [mth(hs[:n]).hex() for n in range(1, 65)]

    # Every index of the small sizes, sampled indices (edges and powers of two) above.
    wanted = [(size, index) for size in (1, 2, 3, 4, 5, 6, 7, 8, 9, 13, 16, 17)
              for index in range(size)]
    wanted += [(33, i) for i in (0, 15, 16, 31, 32)]
    wanted += [(63, i) for i in (0, 31, 32, 62)]
    wanted += [(64, i) for i in (0, 1, 17, 31, 32, 40, 63)]
    inclusion = []
    for size, index in wanted:
        path = audit_path(index, hs[:size])
        assert verify_inclusion(hs[index], index, size, path, mth(hs[:size]))
        inclusion.append({"size": size, "index": index, "path": [p.hex() for p in path]})

    # Every pair up to 12 leaves, sampled first sizes above.
    pairs = [(m, n) for n in range(1, 13) for m in range(1, n + 1)]
    firsts = (1, 2, 7, 8, 9, 15, 16, 17, 31, 32, 33, 40)
    pairs += [(m, n) for n in (16, 17, 31, 32, 33, 63, 64) for m in firsts if m <= n]
    consistency = []
    for m, n in pairs:
        proof = consistency_proof(m, hs[:n])
        assert verify_consistency(m, n, mth(hs[:m]), mth(hs[:n]), proof)
        consistency.append({"first": m, "second": n, "proof": [p.hex() for p in proof]})

    # The 600-leaf tile vectors.
    tile_entries = []
    for i in range(600):
        _, entry = leaf_record(ref_inputs(i))
        tile_entries.append(entry)
    tile_hs = [leaf_hash(e) for e in tile_entries]
    files = tile_files(tile_entries, tile_hs)
    tiles = {
        "leaf_count": 600,
        "leaves": "ref leaves i = 0..599: branch main, generation i + 1, receipt = "
                  "SHA-256('dynodoc.test-receipt' || u64be(i)); batch-appended in one call",
        "root": mth(tile_hs).hex(),
        "index_paths": {str(n): tile_index_path(n)
                        for n in (0, 5, 255, 256, 999, 1000, 1234067, 18446744073709551615)},
        "files": {path: {"sha256": h256(data).hex(), "bytes": len(data)}
                  for path, data in sorted(files.items())},
        "small_files": {path: data.hex() for path, data in sorted(files.items()) if len(data) <= 64},
        "inclusion": [],
        "consistency": [],
    }
    for size, index in ((600, 0), (600, 255), (600, 256), (600, 511), (600, 512), (600, 599),
                        (300, 299), (257, 256), (1, 0)):
        path = audit_path(index, tile_hs[:size])
        assert verify_inclusion(tile_hs[index], index, size, path, mth(tile_hs[:size]))
        tiles["inclusion"].append({"size": size, "index": index, "path": [p.hex() for p in path]})
    for m, n in ((255, 600), (256, 600), (257, 600), (300, 600), (512, 600), (1, 257)):
        proof = consistency_proof(m, tile_hs[:n])
        assert verify_consistency(m, n, mth(tile_hs[:m]), mth(tile_hs[:n]), proof)
        tiles["consistency"].append({"first": m, "second": n, "proof": [p.hex() for p in proof]})

    # Checkpoints and notes over the 64-leaf tree.
    times = [T0, T0 + 42, T0 + 100]
    old_times = [T0 - 1000, T0 - 958, T0 - 900]
    text64 = checkpoint_text(LOG_ORIGIN, 64, mth(hs))
    text40 = checkpoint_text(LOG_ORIGIN, 40, mth(hs[:40]))
    notes = {
        "size64_log_only": cosigned(text64, [], times),
        "size64_one": cosigned(text64, [0], times),
        "size64_quorum": cosigned(text64, [0, 1], times),
        "size64_full": cosigned(text64, [0, 1, 2], times),
        "size40_quorum": cosigned(text40, [0, 1], old_times),
    }
    checkpoint = {
        "size64": {"origin": LOG_ORIGIN, "size": 64, "root": mth(hs).hex(), "text": text64},
        "size40": {"origin": LOG_ORIGIN, "size": 40, "root": mth(hs[:40]).hex(), "text": text40},
        "witness_times": times,
        "old_witness_times": old_times,
        "log_signature_line_size64": log_line(LOG_ORIGIN, LOG_SEED, text64),
        "cosignature_line_w1_size64": cosign_line(WITNESSES[0][0], WITNESSES[0][2], text64, times[0]),
    }

    # Negative notes: each must be refused by Policy::verify_checkpoint with this reason.
    quorum = notes["size64_quorum"]
    w_lines = [cosign_line(WITNESSES[i][0], WITNESSES[i][2], text64, times[i]) for i in (0, 1)]
    log_sig = log_line(LOG_ORIGIN, LOG_SEED, text64)
    flipped = bytearray(base64.b64decode(log_sig.rstrip("\n").split(" ", 2)[2]))
    flipped[10] ^= 1
    flipped_line = sig_line(LOG_ORIGIN, bytes(flipped[:4]), bytes(flipped[4:]))
    w1_kid = key_id(WITNESSES[0][0], TYPE_COSIGNATURE, ed_public(WITNESSES[0][2]))
    w1_body = cosign_body(WITNESSES[0][2], text64, times[0])
    retimed = struct.pack(">Q", times[0] + 1) + w1_body[8:]
    other_text = checkpoint_text(OTHER_ORIGIN, 64, mth(hs))
    ext_text = text64 + "extension line\n"
    zero_text = checkpoint_text(LOG_ORIGIN, 64, mth(hs)).replace("\n64\n", "\n064\n")
    rejected_notes = [
        ("no_blank_line", quorum.replace("\n\n", "\n", 1), "malformed_note"),
        ("em_dash_replaced", quorum.replace("—", "-"), "malformed_note"),
        ("control_character", quorum.replace(LOG_ORIGIN, LOG_ORIGIN + "\x07", 1), "malformed_note"),
        ("no_final_newline", quorum.rstrip("\n"), "malformed_note"),
        ("duplicate_log_signature", quorum + log_sig, "duplicate_signature"),
        ("duplicate_cosignature", quorum + w_lines[0], "duplicate_signature"),
        ("log_name_with_another_key",
         make_note(text64, [log_line(LOG_ORIGIN, ROGUE_SEED, text64)] + w_lines),
         "missing_log_signature"),
        ("no_log_signature", make_note(text64, w_lines), "missing_log_signature"),
        ("log_signature_bit_flipped", make_note(text64, [flipped_line] + w_lines), "bad_signature"),
        ("cosignature_time_changed",
         make_note(text64, [log_sig, sig_line(WITNESSES[0][0], w1_kid, retimed), w_lines[1]]),
         "bad_signature"),
        ("other_log_origin",
         make_note(other_text, [log_line(OTHER_ORIGIN, ROGUE_SEED, other_text)]), "wrong_origin"),
        ("origin_of_this_log_in_other_text",
         make_note(other_text, [log_line(LOG_ORIGIN, LOG_SEED, other_text)]), "wrong_origin"),
        ("extension_line", make_note(ext_text, [log_line(LOG_ORIGIN, LOG_SEED, ext_text)]),
         "checkpoint_extension"),
        ("size_with_leading_zero",
         make_note(zero_text, [log_line(LOG_ORIGIN, LOG_SEED, zero_text)]), "malformed_checkpoint"),
    ]

    # Bundles and assessments.
    index = 17
    path17 = audit_path(index, hs)
    assert verify_inclusion(hs[index], index, 64, path17, mth(hs))
    bundles = {name: bundle_object(records[index], index, 64, path17, note)
               for name, note in notes.items() if name.startswith("size64")}
    quorum_bundle = bundles["size64_quorum"]
    flipped_path = list(quorum_bundle["path"])
    flipped_path[0] = ("%02x" % (int(flipped_path[0][:2], 16) ^ 1)) + flipped_path[0][2:]
    other_leaf = dict(quorum_bundle, leaf=json.loads(records[18]["data"]))
    rejected_bundles = [
        ("leaf_of_another_index", other_leaf, "bad_inclusion_proof"),
        ("path_byte_flipped", dict(quorum_bundle, path=flipped_path), "bad_inclusion_proof"),
        ("path_truncated", dict(quorum_bundle, path=quorum_bundle["path"][:-1]), "bad_inclusion_proof"),
        ("wrong_index", dict(quorum_bundle, index=18), "bad_inclusion_proof"),
        ("index_beyond_size", dict(quorum_bundle, index=64), "index_out_of_range"),
        ("size_differs_from_checkpoint", dict(quorum_bundle, size=63), "bundle_size_mismatch"),
    ]

    proof_40_64 = consistency_proof(40, hs)
    assert verify_consistency(40, 64, mth(hs[:40]), mth(hs), proof_40_64)
    operators = [op for _, op, _ in WITNESSES]
    shared_operators = ["verne", "verne", "customer"]
    assessments = [
        {"name": "quorum_first_contact", "policy": "main", "bundle": "size64_quorum",
         "known": None, "now": NOW, "level": "L2", "limits": ["no_prior_checkpoint"]},
        {"name": "consistent_with_known", "policy": "main", "bundle": "size64_quorum",
         "known": "size40_quorum", "proofs": True, "now": NOW, "level": "L3", "limits": []},
        {"name": "known_but_no_proof", "policy": "main", "bundle": "size64_quorum",
         "known": "size40_quorum", "proofs": False, "now": NOW, "level": "L2",
         "limits": ["consistency_unavailable"]},
        {"name": "log_signature_only", "policy": "main", "bundle": "size64_log_only",
         "known": None, "now": NOW, "level": "L1",
         "limits": ["quorum_not_met", "no_prior_checkpoint"]},
        {"name": "one_cosignature_of_two", "policy": "main", "bundle": "size64_one",
         "known": None, "now": NOW, "level": "L1",
         "limits": ["quorum_not_met", "no_prior_checkpoint"]},
        {"name": "stale_cosignatures", "policy": "main", "bundle": "size64_quorum",
         "known": None, "now": T0 + 100000, "level": "L1",
         "limits": ["quorum_not_met", "no_prior_checkpoint"]},
        {"name": "three_cosignatures", "policy": "main", "bundle": "size64_full",
         "known": None, "now": NOW, "level": "L2", "limits": ["no_prior_checkpoint"]},
        {"name": "two_keys_one_operator", "policy": "shared_operator", "bundle": "size64_quorum",
         "known": None, "now": NOW, "level": "L1",
         "limits": ["quorum_not_met", "no_prior_checkpoint"]},
        {"name": "two_operators_of_three_keys", "policy": "shared_operator",
         "bundle": "size64_full", "known": None, "now": NOW, "level": "L2",
         "limits": ["no_prior_checkpoint"]},
    ]

    out = {
        "description": "Transparency log vectors v1 (design W0). Produced by "
                       "gen_transparency_vectors.py, an independent pure-Python implementation; "
                       "never regenerate to make a failing Rust check pass.",
        "generator": "gen_transparency_vectors.py (Python stdlib only; pure-Python RFC 8032)",
        "cross_checks": {
            "rfc8032_vectors": rfc8032,
            "rfc6962_published_roots": rfc6962,
            "signed_note_example_reproduced": note_example,
        },
        "unverified": [
            "signature type byte 0x04 and key-id input for witness (cosignature) keys",
            "cosignature/v1 message layout: 'cosignature/v1\\ntime <decimal>\\n' + checkpoint text",
            "cosignature signature-line body: 8-byte big-endian time then the 64-byte signature",
            "tlog-tiles path spelling, partial-tile suffix and entry-bundle framing",
            "forbidden control characters in notes",
        ],
        "leaf_domain": LEAF_DOMAIN.decode("latin-1").replace("\x00", "\\0"),
        "keys": keys,
        "tags": tags,
        "leaves": records,
        "roots": roots,
        "inclusion": inclusion,
        "consistency": consistency,
        "tiles": tiles,
        "checkpoint": checkpoint,
        "notes": notes,
        "policy": policy_object(operators),
        "policy_shared_operator": policy_object(shared_operators),
        "now": NOW,
        "bundles": bundles,
        "bundle_index": index,
        "bundle_canonical_json": canon(quorum_bundle),
        "proof_40_to_64": [p.hex() for p in proof_40_64],
        "assessments": assessments,
        "rejected_notes": [{"name": n, "note": v, "reason": r} for n, v, r in rejected_notes],
        "rejected_bundles": [{"name": n, "bundle": v, "reason": r} for n, v, r in rejected_bundles],
    }
    target = os.path.join(os.path.dirname(os.path.abspath(__file__)), "transparency-v1.json")
    with open(target, "w", encoding="utf-8", newline="\n") as f:
        f.write(emit(out) + "\n")
    print("wrote %s (%d bytes)" % (target, os.path.getsize(target)))


if __name__ == "__main__":
    sys.exit(main())
