#!/usr/bin/env python3
"""Independent generator for horizon-v1.json (Python standard library only).

Run:  python -I gen_horizon_vectors.py      (rewrites horizon-v1.json next to this
file; the output is deterministic, so a clean `git diff` afterwards shows the
committed file is reproduced.)

Nothing here shares code with the Rust modules under test
(crates/engine-core/src/repository/horizon and the planner in
repository/horizon/plan.rs). It implements, from docs/HORIZON-RECORD-V1.md:

  * the canonical JSON form, record address and signature input of a horizon record;
  * the RFC 6962 Merkle roots over skeleton entries, frontier edges, released states
    and (plan only) the retained commit set, and the heads digest;
  * the normalized form and hash of a retention policy;
  * the read-only planner: retained set, skeleton, frontier, released states, object
    accounting, reasons, plan JSON and plan hash, for synthetic histories.

The Ed25519 block below is copied unchanged from gen_transparency_vectors.py (pure
Python RFC 8032) and is checked against the RFC 8032 section 7.1 vectors on every
run. Because both implementations were written from the same specification, the
vectors catch implementation bugs and nondeterminism, not a shared misreading.
"""
import hashlib
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



# ---- hashing, canonical JSON, RFC 6962 Merkle roots --------------------------
def h256(data):
    return hashlib.sha256(data).digest()


def canon(obj):
    """Canonical JSON: sorted keys, no whitespace, UTF-8 (not ASCII-escaped)."""
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def u64(n):
    assert 0 <= n < 2**53
    return struct.pack(">Q", n)


def split(n):
    """Largest power of two strictly below n (n >= 2)."""
    k = 1
    while k * 2 < n:
        k *= 2
    return k


def mth(hs):
    n = len(hs)
    if n == 0:
        return h256(b"")
    if n == 1:
        return hs[0]
    k = split(n)
    return h256(b"\x01" + mth(hs[:k]) + mth(hs[k:]))


def merkle_root(leaf_data):
    return mth([h256(b"\x00" + d) for d in leaf_data])


# ---- domains (the trailing NUL after each version is part of the domain) -------
ADDRESS_DOMAIN = b"dynodoc.history-horizon\x00v1\x00"
SIGNATURE_DOMAIN = b"dynodoc.history-horizon-signature\x00v1\x00"
HEADS_DOMAIN = b"dynodoc.horizon-heads\x00v1\x00"
SKELETON_LEAF = b"dynodoc.horizon-skeleton-leaf\x00v1\x00"
FRONTIER_LEAF = b"dynodoc.horizon-frontier-leaf\x00v1\x00"
STATE_LEAF = b"dynodoc.horizon-state-leaf\x00v1\x00"
RETAINED_LEAF = b"dynodoc.horizon-retained-leaf\x00v1\x00"
POLICY_DOMAIN = b"dynodoc.retention-policy\x00v1\x00"
PLAN_DOMAIN = b"dynodoc.horizon-plan-hash\x00v1\x00"
COMMIT_SIGNATURE_DOMAIN = b"dynodoc.project-commit-signature\x00v1\x00"
DOMAINS = {
    "address": ADDRESS_DOMAIN,
    "signature": SIGNATURE_DOMAIN,
    "heads": HEADS_DOMAIN,
    "skeleton_leaf": SKELETON_LEAF,
    "frontier_leaf": FRONTIER_LEAF,
    "state_leaf": STATE_LEAF,
    "retained_leaf": RETAINED_LEAF,
    "policy": POLICY_DOMAIN,
    "plan": PLAN_DOMAIN,
}
DAY_MS = 86_400_000
EMPTY_ROOT = h256(b"").hex()


def skeleton_leaf(e):
    parents = b"".join(bytes.fromhex(p) for p in e["parents"])
    assert len(e["parents"]) <= 2
    return (
        SKELETON_LEAF
        + bytes.fromhex(e["address"])
        + bytes([len(e["parents"])])
        + parents
        + u64(e["height"])
    )


def frontier_leaf(e):
    return (
        FRONTIER_LEAF
        + bytes.fromhex(e["child"])
        + bytes.fromhex(e["parent"])
        + u64(e["parent_height"])
    )


def state_leaf(s):
    return STATE_LEAF + uuid.UUID(s["document"]).bytes + bytes.fromhex(s["state_root"])


def retained_leaf(address):
    return RETAINED_LEAF + bytes.fromhex(address)


def skeleton_root(entries):
    ordered = sorted(entries, key=lambda e: e["address"])
    assert len({e["address"] for e in ordered}) == len(ordered)
    return merkle_root([skeleton_leaf(e) for e in ordered]).hex()


def frontier_root(edges):
    ordered = sorted(edges, key=lambda e: (e["child"], e["parent"]))
    return merkle_root([frontier_leaf(e) for e in ordered]).hex()


def state_sort_key(s):
    return (uuid.UUID(s["document"]).bytes, s["state_root"])


def states_root(states):
    ordered = sorted(states, key=state_sort_key)
    return merkle_root([state_leaf(s) for s in ordered]).hex()


def retained_root(addresses):
    return merkle_root([retained_leaf(a) for a in sorted(addresses)]).hex()


def heads_digest(heads):
    ordered = sorted(heads, key=lambda h: h["branch"].encode())
    lines = b"".join(
        h["branch"].encode() + b"\x00" + h["commit"].encode() + b"\x00"
        + str(h["generation"]).encode() + b"\n"
        for h in ordered
    )
    return h256(HEADS_DOMAIN + lines).hex()


def record_address(record):
    return h256(ADDRESS_DOMAIN + canon(record).encode("utf-8")).hex()


def signing_input(address):
    return SIGNATURE_DOMAIN + bytes.fromhex(address)


def label(text):
    """A synthetic 64-hex address (not a real commit hash) for a vector label."""
    return h256(("dynodoc-horizon-vector:" + text).encode()).hex()


def uid(text):
    return str(uuid.UUID(bytes=h256(("dynodoc-horizon-vector:" + text).encode())[:16]))


def seed(text):
    return h256(("dynodoc-horizon-vector-seed:" + text).encode())


# ---- retention policy (normalized form and hash) -------------------------------
def normalize_policy(p):
    window = {k: p["window"][k] for k in ("keep_within_days", "keep_last_commits", "min_commits_kept") if k in p["window"]}
    protect = p.get("protect", {})
    shield = {
        "branch_heads": protect.get("branch_heads", True),
        "named_versions": protect.get("named_versions", True),
        "open_review_candidates": protect.get("open_review_candidates", True),
        "reviewed_candidates": protect.get("reviewed_candidates", "body"),
        "recovery_sources": protect.get("recovery_sources", True),
        "merge_inputs": protect.get("merge_inputs", "states"),
        "holds": protect.get("holds", True),
        "commits": sorted(set(protect.get("commits", []))),
    }
    if "merge_bases" in protect:
        mb = protect["merge_bases"]
        shield["merge_bases"] = {"against": mb["against"], "branches": sorted(set(mb.get("branches", ["*"])))}
    archive = {"required": p.get("archive", {}).get("required", False)}
    if "destination" in p.get("archive", {}):
        archive["destination"] = p["archive"]["destination"]
    return {
        "version": 1,
        "id": p["id"],
        "description": p.get("description", ""),
        "tier": p["tier"],
        "window": window,
        "protect": shield,
        "topology": p.get("topology", "keep"),
        "acknowledge_unmergeable_history": p.get("acknowledge_unmergeable_history", False),
        "archive": archive,
        "signatures": {"required": p.get("signatures", {}).get("required", 1)},
        "quarantine_days": p.get("quarantine_days", 0),
    }


def policy_hash(normalized):
    return h256(POLICY_DOMAIN + canon(normalized).encode("utf-8")).hex()


# The design's section 4.1 example, converted to JSON (YAML is not accepted).
EXAMPLE_POLICY = {
    "version": 1,
    "id": "team-default",
    "description": "Keep a year of full history. Keep named versions, merged-review evidence and\nrecovery provenance forever. Keep topology for everything else.\n",
    "tier": "history",
    "window": {"keep_within_days": 365, "keep_last_commits": 500, "min_commits_kept": 50},
    "protect": {
        "branch_heads": True,
        "named_versions": True,
        "merge_bases": {"against": "main", "branches": ["*"]},
        "open_review_candidates": True,
        "reviewed_candidates": "body",
        "recovery_sources": True,
        "merge_inputs": "states",
        "holds": True,
        "commits": [],
    },
    "topology": "keep",
    "archive": {"required": True, "destination": "file:///archives/{project}/{sequence}.dynarc"},
    "signatures": {"required": 2},
    "quarantine_days": 14,
}

SCAFFOLD = '"version":1,"id":"t","tier":"history","window":{"min_commits_kept":5},"protect":{"branch_heads":true}'

# Hand-written expectations from docs/HORIZON-RECORD-V1.md, not computed: the loader
# must report exactly these (path, code) pairs, and nothing else.
INVALID_POLICIES = [
    ("not_json", "{", 1, [("", "policy.malformed_json")]),
    ("not_an_object", "[1]", 1, [("", "policy.invalid_value")]),
    ("duplicate_key", '{"version":1,"version":1}', 1, [("", "policy.duplicate_field")]),
    ("unknown_top_level_field", "{" + SCAFFOLD + ',"bogus":1}', 1, [("/bogus", "policy.unknown_field")]),
    ("unknown_nested_field", '{"version":1,"id":"t","tier":"content","window":{"min_commits_kept":5,"extra":1}}', 1, [("/window/extra", "policy.unknown_field")]),
    ("unsupported_version", '{"version":2,"id":"t","tier":"content","window":{"min_commits_kept":5}}', 1, [("/version", "policy.unsupported_version")]),
    ("missing_version", '{"id":"t","tier":"content","window":{"min_commits_kept":5}}', 1, [("/version", "policy.missing_field")]),
    ("missing_id_and_tier", '{"version":1,"window":{"min_commits_kept":5}}', 1, [("/id", "policy.missing_field"), ("/tier", "policy.missing_field")]),
    ("heads_false", '{"version":1,"id":"t","tier":"history","window":{"min_commits_kept":5},"protect":{"branch_heads":false}}', 1, [("/protect/branch_heads", "policy.heads_unprotected")]),
    ("heads_absent_in_history", '{"version":1,"id":"t","tier":"history","window":{"min_commits_kept":5},"protect":{}}', 1, [("/protect/branch_heads", "policy.heads_unprotected")]),
    ("protect_absent_in_history", '{"version":1,"id":"t","tier":"history","window":{"min_commits_kept":5}}', 1, [("/protect/branch_heads", "policy.heads_unprotected")]),
    ("window_empty", '{"version":1,"id":"t","tier":"content","window":{}}', 1, [("/window", "policy.window_empty")]),
    ("window_absent", '{"version":1,"id":"t","tier":"content"}', 1, [("/window", "policy.window_empty")]),
    ("window_zero_days", '{"version":1,"id":"t","tier":"content","window":{"keep_within_days":0}}', 1, [("/window/keep_within_days", "policy.invalid_value")]),
    ("signatures_below_service_floor", "{" + SCAFFOLD + ',"signatures":{"required":1}}', 2, [("/signatures/required", "policy.signatures_below_floor")]),
    ("signatures_zero", "{" + SCAFFOLD + ',"signatures":{"required":0}}', 1, [("/signatures/required", "policy.signatures_below_floor")]),
    ("signatures_absent_below_service_floor", "{" + SCAFFOLD + "}", 2, [("/signatures/required", "policy.signatures_below_floor")]),
    ("drop_unacknowledged", "{" + SCAFFOLD + ',"topology":"drop"}', 1, [("/topology", "policy.topology_drop_unacknowledged")]),
    ("drop_in_content_tier", '{"version":1,"id":"t","tier":"content","window":{"min_commits_kept":5},"topology":"drop","acknowledge_unmergeable_history":true}', 1, [("/topology", "policy.invalid_value")]),
    ("holds_switched_off", '{"version":1,"id":"t","tier":"history","window":{"min_commits_kept":5},"protect":{"branch_heads":true,"holds":false}}', 1, [("/protect/holds", "policy.protection_not_optional")]),
    ("recovery_sources_switched_off", '{"version":1,"id":"t","tier":"history","window":{"min_commits_kept":5},"protect":{"branch_heads":true,"recovery_sources":false}}', 1, [("/protect/recovery_sources", "policy.protection_not_optional")]),
    ("archive_without_destination", "{" + SCAFFOLD + ',"archive":{"required":true}}', 1, [("/archive/destination", "policy.archive_destination_missing")]),
    ("archive_destination_not_a_file_url", "{" + SCAFFOLD + ',"archive":{"destination":"https://example.org/a"}}', 1, [("/archive/destination", "policy.invalid_value")]),
    ("archive_unknown_placeholder", "{" + SCAFFOLD + ',"archive":{"destination":"file:///a/{user}"}}', 1, [("/archive/destination", "policy.invalid_value")]),
    ("tier_not_in_vocabulary", '{"version":1,"id":"t","tier":"weekly","window":{"min_commits_kept":5}}', 1, [("/tier", "policy.invalid_value")]),
    ("id_not_an_identifier", '{"version":1,"id":"Bad Id","tier":"content","window":{"min_commits_kept":5}}', 1, [("/id", "policy.invalid_value")]),
    ("merge_bases_without_against", '{"version":1,"id":"t","tier":"history","window":{"min_commits_kept":5},"protect":{"branch_heads":true,"merge_bases":{}}}', 1, [("/protect/merge_bases/against", "policy.missing_field")]),
    ("merge_bases_pattern", '{"version":1,"id":"t","tier":"history","window":{"min_commits_kept":5},"protect":{"branch_heads":true,"merge_bases":{"against":"main","branches":["a*b"]}}}', 1, [("/protect/merge_bases/branches/0", "policy.invalid_value")]),
    ("pinned_commit_not_an_address", '{"version":1,"id":"t","tier":"history","window":{"min_commits_kept":5},"protect":{"branch_heads":true,"commits":["ABC"]}}', 1, [("/protect/commits/0", "policy.invalid_value")]),
    ("two_problems_are_both_reported", '{"version":1,"id":"t","tier":"history","window":{},"protect":{"branch_heads":false}}', 1, [("/protect/branch_heads", "policy.heads_unprotected"), ("/window", "policy.window_empty")]),
]

VALID_POLICIES = [
    ("design_example", json.dumps(EXAMPLE_POLICY, ensure_ascii=False), 2),
    ("minimal_content", '{"version":1,"id":"keep.last_5","tier":"content","window":{"min_commits_kept":5}}', 1),
    (
        "unsorted_lists_and_defaults",
        '{"window":{"keep_last_commits":3},"tier":"history","id":"x","version":1,"protect":{"commits":["%s","%s","%s"],"branch_heads":true,"merge_bases":{"against":"main","branches":["review/*","*","review/*"]}}}'
        % (label("pin-b"), label("pin-a"), label("pin-b")),
        1,
    ),
    (
        "drop_acknowledged",
        '{"version":1,"id":"drop","tier":"history","window":{"keep_within_days":30},"protect":{"branch_heads":true},"topology":"drop","acknowledge_unmergeable_history":true}',
        1,
    ),
]


# ---- signatures and records ------------------------------------------------------
def make_signature(role, seed_bytes, address):
    return {
        "role": role,
        "key": ed_public(seed_bytes).hex(),
        "signature": ed_sign(seed_bytes, signing_input(address)).hex(),
    }


def make_envelope(record, signers):
    sigs = [make_signature(role, sd, record_address(record)) for role, sd in signers]
    return {"horizon": record, "signatures": sorted(sigs, key=lambda s: s["key"])}


# ---- the fixed graph used by the roots and the invariant checks ------------------
GRAPH = [
    ("c00", []), ("c01", ["c00"]), ("c02", ["c01"]), ("c03", ["c01"]), ("c04", ["c02", "c03"]),
    ("c05", ["c04"]), ("c06", ["c05"]), ("c07", ["c06"]), ("c08", ["c03"]),
    ("c09", ["c07", "c08"]), ("c10", ["c09"]), ("c11", ["c10"]),
]
GRAPH_RELEASED = ["c00", "c01", "c02", "c03", "c04", "c08"]
GRAPH_HEADS = [
    {"branch": "main", "commit": label("c11"), "generation": 5},
    {"branch": "review/round-2", "commit": label("c07"), "generation": 2},
]


def graph_vector():
    heights = {}
    for name, parents in GRAPH:
        heights[name] = 1 + max([heights[p] for p in parents]) if parents else 0
    docs = [uid("graph-doc-1"), uid("graph-doc-2")]
    states = {}
    for index, (name, _) in enumerate(GRAPH):
        states[name] = [
            {"document": docs[0], "state_root": label("graph-state-1-%d" % index)},
            {"document": docs[1], "state_root": label("graph-state-2-%d" % (index // 4))},
        ]
    retained = [n for n, _ in GRAPH if n not in GRAPH_RELEASED]
    released_set = set(GRAPH_RELEASED)
    skeleton = [
        {"address": label(n), "parents": [label(p) for p in dict(GRAPH)[n]], "height": heights[n]}
        for n in GRAPH_RELEASED
    ]
    frontier = [
        {"child": label(n), "parent": label(p), "parent_height": heights[p]}
        for n in retained for p in dict(GRAPH)[n] if p in released_set
    ]
    kept_states = {(s["document"], s["state_root"]) for n in retained for s in states[n]}
    released_states = []
    for n in GRAPH_RELEASED:
        for s in states[n]:
            if (s["document"], s["state_root"]) not in kept_states and s not in released_states:
                released_states.append(s)
    return {
        "commits": [
            {"name": n, "address": label(n), "parents": [label(p) for p in ps], "height": heights[n], "states": states[n]}
            for n, ps in GRAPH
        ],
        "released": [label(n) for n in GRAPH_RELEASED],
        "retained": [label(n) for n in retained],
        "heads": GRAPH_HEADS,
        "heads_digest": heads_digest(GRAPH_HEADS),
        "skeleton": skeleton,
        "frontier": frontier,
        "released_states": released_states,
        "roots": {
            "skeleton": skeleton_root(skeleton),
            "frontier": frontier_root(frontier),
            "released_states": states_root(released_states),
            "retained": retained_root([label(n) for n in retained]),
        },
    }


def merkle_vectors():
    """Roots over growing prefixes of one sorted-by-hash population, fed unsorted."""
    cases = []
    for kind in ("skeleton", "frontier", "states", "retained"):
        for size in (0, 1, 2, 3, 4, 5, 7, 8, 9):
            if kind == "skeleton":
                items = [
                    {"address": label("m-s-%d" % i),
                     "parents": [label("m-p-%d-%d" % (i, j)) for j in range(i % 3)],
                     "height": i * 7}
                    for i in range(size)
                ]
                leaves = [skeleton_leaf(e) for e in sorted(items, key=lambda e: e["address"])]
                root = skeleton_root(items)
            elif kind == "frontier":
                items = [
                    {"child": label("m-c-%d" % i), "parent": label("m-q-%d" % (i % 3)), "parent_height": i}
                    for i in range(size)
                ]
                leaves = [frontier_leaf(e) for e in sorted(items, key=lambda e: (e["child"], e["parent"]))]
                root = frontier_root(items)
            elif kind == "states":
                items = [
                    {"document": uid("m-doc-%d" % (i % 2)), "state_root": label("m-state-%d" % i)}
                    for i in range(size)
                ]
                leaves = [state_leaf(e) for e in sorted(items, key=state_sort_key)]
                root = states_root(items)
            else:
                items = [label("m-r-%d" % i) for i in range(size)]
                leaves = [retained_leaf(a) for a in sorted(items)]
                root = retained_root(items)
            cases.append({
                "kind": kind,
                "items": items,
                "sorted_leaf_data": [leaf.hex() for leaf in leaves],
                "root": root,
            })
    return cases


def heads_vectors():
    sets = [
        [],
        [{"branch": "main", "commit": label("h0"), "generation": 1}],
        [
            {"branch": "review/round-2", "commit": label("h2"), "generation": 12},
            {"branch": "main", "commit": label("h1"), "generation": 9007199254740991},
            {"branch": "Main", "commit": label("h3"), "generation": 3},
        ],
    ]
    return [{"heads": s, "digest": heads_digest(s)} for s in sets]


PROJECT = "5b0f5b77-5a35-47a6-9d61-2ad0a2c9b8d0"


def record_vectors(graph):
    example_norm = normalize_policy(EXAMPLE_POLICY)
    policy_ref = {"id": "team-default", "hash": policy_hash(example_norm)}
    counts = {
        "commits_released": len(GRAPH_RELEASED),
        "commit_bytes_released": 1893,
        "states_released": len(graph["released_states"]),
        "objects_released": 11,
        "object_bytes_released": 20481,
    }
    rec1 = {
        "version": 1, "kind": "dynodoc.history-horizon", "project": PROJECT, "sequence": 1,
        "previous": None, "created_at_ms": 1791000000000, "tier": "history", "policy": policy_ref,
        "heads_digest": graph["heads_digest"], "counts": counts,
        "roots": {"skeleton": graph["roots"]["skeleton"], "frontier": graph["roots"]["frontier"],
                  "released_states": graph["roots"]["released_states"]},
        "holds": [], "evidence": ["merge_inputs:states", "recovery_sources"],
    }
    addr1 = record_address(rec1)
    counts2 = dict(counts, commits_released=3, commit_bytes_released=700, states_released=2,
                   objects_released=4, object_bytes_released=4096)
    rec2 = {
        "version": 1, "kind": "dynodoc.history-horizon", "project": PROJECT, "sequence": 2,
        "previous": addr1, "created_at_ms": 1791086400000, "tier": "history", "policy": policy_ref,
        "heads_digest": heads_digest(GRAPH_HEADS[:1]), "counts": counts2,
        "roots": {"skeleton": label("r2-skeleton"), "frontier": label("r2-frontier"),
                  "released_states": label("r2-states")},
        "archive": {"sha256": label("r2-archive"), "bytes": 3200145002, "commits": 3, "objects": 4},
        "holds": sorted([
            {"commit": label("c04"), "reason": "legal-hold:LH-17"},
            {"commit": label("c04"), "reason": "আইনি সুরক্ষা 🔒 \"quoted\" \\ end é"},
        ], key=lambda h: (h["commit"], h["reason"])),
        "evidence": ["holds", "merge_bases", "merge_inputs:states", "recovery_sources", "reviewed_candidates:body"],
    }
    rec3 = {
        "version": 1, "kind": "dynodoc.history-horizon", "project": PROJECT, "sequence": 1,
        "previous": None, "created_at_ms": 1791000000000, "tier": "content",
        "policy": {"id": "keep.last_5", "hash": policy_hash(normalize_policy(json.loads(VALID_POLICIES[1][1])))},
        "heads_digest": graph["heads_digest"],
        "counts": {"commits_released": 0, "commit_bytes_released": 0, "states_released": 3,
                   "objects_released": 7, "object_bytes_released": 900},
        "roots": {"skeleton": EMPTY_ROOT, "frontier": EMPTY_ROOT, "released_states": label("r3-states")},
        "holds": [], "evidence": ["merge_inputs:states", "recovery_sources"],
    }
    seeds = {"owner": seed("owner"), "authority": seed("authority"), "device": seed("device")}
    keys = {name: {"seed": sd.hex(), "public": ed_public(sd).hex()} for name, sd in seeds.items()}
    recs = [rec1, rec2, rec3]
    out = []
    for rec in recs:
        out.append({
            "canonical": canon(rec),
            "address": record_address(rec),
            "signing_input": signing_input(record_address(rec)).hex(),
        })
    env1 = make_envelope(rec1, [("owner", seeds["owner"]), ("authority", seeds["authority"])])
    env2 = make_envelope(rec2, [("owner", seeds["owner"]), ("authority", seeds["authority"])])
    env3 = make_envelope(rec3, [("device", seeds["device"])])
    # Signature-domain confusion and the other envelope rejections (all hand-built).
    a2 = record_address(rec2)
    commit_domain = {
        "role": "owner", "key": keys["owner"]["public"],
        "signature": ed_sign(seeds["owner"], COMMIT_SIGNATURE_DOMAIN + bytes.fromhex(a2)).hex(),
    }
    no_domain = {
        "role": "owner", "key": keys["owner"]["public"],
        "signature": ed_sign(seeds["owner"], bytes.fromhex(a2)).hex(),
    }
    other_address = make_signature("owner", seeds["owner"], addr1)
    good = {s["role"]: s for s in env2["signatures"]}
    authority_ok = good["authority"]
    envelopes = [
        {"name": "shared_two_signatures", "envelope": env2, "scope": "shared", "required": 2, "expect": "ok"},
        {"name": "shared_first_in_chain", "envelope": env1, "scope": "shared", "required": 2, "expect": "ok"},
        {"name": "local_device", "envelope": env3, "scope": "local", "required": 1, "expect": "ok"},
        {"name": "one_signature_where_two_required", "envelope": {"horizon": rec2, "signatures": [authority_ok]}, "scope": "shared", "required": 2, "expect": "H2"},
        {"name": "signature_over_commit_domain", "envelope": {"horizon": rec2, "signatures": [commit_domain]}, "scope": "shared", "required": 1, "expect": "H2"},
        {"name": "signature_without_domain", "envelope": {"horizon": rec2, "signatures": [no_domain]}, "scope": "shared", "required": 1, "expect": "H2"},
        {"name": "signature_over_another_record", "envelope": {"horizon": rec2, "signatures": [other_address]}, "scope": "shared", "required": 1, "expect": "H2"},
        {"name": "authority_is_not_a_local_role", "envelope": {"horizon": rec2, "signatures": [authority_ok]}, "scope": "local", "required": 1, "expect": "H2"},
        {"name": "device_is_not_a_shared_role", "envelope": env3, "scope": "shared", "required": 1, "expect": "H2"},
        {"name": "signatures_out_of_order", "envelope": {"horizon": rec2, "signatures": list(reversed(env2["signatures"]))}, "scope": "shared", "required": 2, "expect": "H2"},
        {"name": "same_key_twice", "envelope": {"horizon": rec2, "signatures": [env2["signatures"][0], env2["signatures"][0]]}, "scope": "shared", "required": 2, "expect": "H2"},
    ]
    for e in envelopes:
        e["canonical"] = canon(e["envelope"])
        del e["envelope"]
    return {
        "keys": keys,
        "records": [dict(r, record=rec) for r, rec in zip(out, recs)],
        "envelopes": envelopes,
        "chain": [canon(env1), canon(env2)],
        "policy_example_hash": policy_ref["hash"],
    }


# ---- the planner, written from the specification --------------------------------
REASON_ORDER = [
    "branch-head", "hold", "policy-commit", "merge-base",
    "recent-count", "recent-time", "evidence-recovery", "evidence-merge",
]
PRIMARY = set(REASON_ORDER[:6])
PINNED = {"hold", "policy-commit", "merge-base", "evidence-recovery", "evidence-merge"}


def plan(view, policy_raw, as_of_ms, list_n):
    norm = normalize_policy(policy_raw)
    commits = {c["address"]: c for c in view["commits"]}
    skeleton = {e["address"]: e for e in view["skeleton"]}
    assert not (set(commits) & set(skeleton))
    bodies = sorted(commits)
    parents = {a: commits[a]["parents"] for a in bodies}
    parents.update({a: skeleton[a]["parents"] for a in skeleton})
    for a, ps in parents.items():
        for p in ps:
            assert p in parents, "missing parent"
    height = {}

    def h(a):
        if a not in height:
            if a in skeleton:
                height[a] = skeleton[a]["height"]
            else:
                height[a] = max([h(p) + 1 for p in parents[a]], default=0)
        return height[a]

    for a in parents:
        h(a)
    warnings = set()
    heads = sorted(view["branches"], key=lambda b: b["branch"])
    head_node = {b["branch"]: b["commit"] for b in heads}
    for commit in head_node.values():
        assert commit in commits, "head must be a stored body"

    keep = {}

    def mark(a, reason):
        keep.setdefault(a, reason)

    def ancestors(start):
        seen, stack = {start}, [start]
        while stack:
            for p in parents[stack.pop()]:
                if p not in seen:
                    seen.add(p)
                    stack.append(p)
        return seen

    def merge_bases(a, b):
        common = ancestors(a) & ancestors(b)
        dominated = {p for c in common for p in parents[c]}
        return sorted(common - dominated)

    for name in sorted(head_node):
        mark(head_node[name], "branch-head")
    holds = sorted({(x["commit"], x["reason"]) for x in view["holds"]})
    for commit, _ in holds:
        if commit in commits:
            mark(commit, "hold")
        else:
            warnings.add("hold on %s is not a stored commit" % commit)
    for commit in norm["protect"]["commits"]:
        if commit in commits:
            mark(commit, "policy-commit")
        else:
            warnings.add("protected commit %s is not a stored commit" % commit)
    mb = norm["protect"].get("merge_bases")
    if mb:
        if mb["against"] not in head_node:
            warnings.add("merge_bases.against branch %s does not exist; no fork point was pinned" % mb["against"])
        else:
            def matches(branch):
                return any(branch.startswith(p[:-1]) if p.endswith("*") else p == branch for p in mb["branches"])
            for name in sorted(head_node):
                if name != mb["against"] and matches(name):
                    for base in merge_bases(head_node[mb["against"]], head_node[name]):
                        if base in commits:
                            mark(base, "merge-base")
    window = norm["window"]
    count = max([window[k] for k in ("keep_last_commits", "min_commits_kept") if k in window], default=None)
    if count is not None:
        for name in sorted(head_node):
            current = head_node[name]
            for _ in range(count):
                if current is None:
                    break
                mark(current, "recent-count")
                first = parents[current][0] if parents[current] else None
                current = first if first in commits else None
    cutoff = None
    if "keep_within_days" in window:
        assert as_of_ms is not None
        cutoff = as_of_ms - window["keep_within_days"] * DAY_MS
        for a in bodies:
            if commits[a]["authored_at_ms"] >= cutoff:
                mark(a, "recent-time")

    primary = [a for a in bodies if a in keep]
    evidence = []
    for a in primary:
        c = commits[a]
        if c["version"] == 2:
            for source in c["recovery"]:
                evidence.append((source, "evidence-recovery"))
            if parents[a]:
                evidence.append((parents[a][0], "evidence-recovery"))
        elif c["version"] == 3:
            for p in parents[a]:
                evidence.append((p, "evidence-merge"))
            if len(parents[a]) == 2:
                bases = merge_bases(parents[a][0], parents[a][1])
                assert len(bases) == 1
                evidence.append((bases[0], "evidence-merge"))
    for a, reason in evidence:
        if a in commits:
            mark(a, reason)
        else:
            warnings.add("evidence commit %s was released by an earlier horizon" % a)

    history = norm["tier"] == "history"
    retained = [a for a in bodies if (not history) or a in keep]
    released = [a for a in bodies if history and a not in keep]
    released_set = set(released)
    dropped = norm["topology"] == "drop"
    skeleton_rows = [] if dropped else [
        {"address": a, "parents": parents[a], "height": height[a]} for a in released
    ]
    frontier = sorted(
        [{"child": a, "parent": p, "parent_height": height[p]}
         for a in retained for p in parents[a] if p in released_set],
        key=lambda e: (e["child"], e["parent"]),
    )

    def state_set(a):
        return {(s[0], s[1]) for s in commits[a]["files"]}

    if history:
        state_nodes = retained
    else:
        state_nodes = [a for a in bodies if a in keep and (keep[a] in PRIMARY or keep[a] == "evidence-recovery")]
    retained_states = set()
    for a in state_nodes:
        retained_states |= state_set(a)
    if not history and norm["protect"]["merge_inputs"] == "states":
        for a in primary:
            for record in commits[a]["inputs"]:
                for key in ("base", "left", "right"):
                    retained_states.add((record["document"], record[key]))
    all_states = set()
    for a in bodies:
        all_states |= state_set(a)
    released_state_keys = sorted(all_states - retained_states, key=lambda s: (uuid.UUID(s[0]).bytes, s[1]))
    released_states = [{"document": d, "state_root": r} for d, r in released_state_keys]

    objects = view["objects"]
    root_of = {(s["document"], s["state_root"]): s["object"] for s in view["state_objects"]}
    reached = set()

    def walk(roots):
        found, stack = set(), list(roots)
        while stack:
            o = stack.pop()
            if o in reached or o in found:
                continue
            found.add(o)
            stack.extend(objects[o]["children"])
        return found

    kept = walk([root_of[s] for s in retained_states if s in root_of])
    reached |= kept
    freed = walk([root_of[s] for s in all_states - retained_states if s in root_of])
    objects_released = len(freed)
    object_bytes_released = sum(objects[o]["size"] for o in freed)
    measured = all(s in root_of for s in all_states - retained_states) and all(
        s in root_of for s in retained_states if s in all_states
    )

    counts = {
        "commits_released": len(released),
        "commit_bytes_released": sum(commits[a]["bytes"] for a in released),
        "states_released": len(released_states),
        "objects_released": objects_released,
        "object_bytes_released": object_bytes_released,
    }
    by_reason = {}
    for a in bodies:
        if a in keep:
            by_reason[keep[a]] = by_reason.get(keep[a], 0) + 1
    words = {"recovery_sources", "merge_inputs:" + norm["protect"]["merge_inputs"]}
    if mb:
        words.add("merge_bases")
    if holds:
        words.add("holds")
    if dropped:
        words.add("topology:drop")
    result = {
        "format": "dynodoc.horizon-plan.v1",
        "project": view["project"],
        "read_only": True,
        "tier": norm["tier"],
        "topology": norm["topology"],
        "policy": {"id": norm["id"], "hash": policy_hash(norm)},
        "heads_digest": heads_digest(heads),
        "heads": heads,
        "commits": {"total": len(bodies), "retained": len(retained), "released": len(released)},
        "counts": counts,
        "objects_measured": measured,
        "roots": {
            "retained": retained_root(retained),
            "skeleton": skeleton_root(skeleton_rows),
            "frontier": frontier_root(frontier),
            "released_states": states_root(released_states),
        },
        "reasons": [{"reason": r, "commits": by_reason[r]} for r in REASON_ORDER if r in by_reason],
        "evidence": sorted(words),
        "holds": [{"commit": c, "reason": r} for c, r in holds],
        "warnings": sorted(warnings),
        "examples": {
            "released": [{"commit": a, "bytes": commits[a]["bytes"], "height": height[a]} for a in released[:list_n]],
            "pinned": [{"commit": a, "reason": keep[a]} for a in retained if a in keep and keep[a] in PINNED][:list_n],
        },
    }
    if cutoff is not None:
        result["as_of_ms"] = as_of_ms
    body = dict(result)
    del body["examples"]
    result["plan_hash"] = h256(PLAN_DOMAIN + canon(body).encode("utf-8")).hex()
    result["_sets"] = {"retained": retained, "released": released, "skeleton": skeleton_rows}
    return result


# ---- synthetic histories for the planner -----------------------------------------
def build_view():
    """p0..p2 -> a -> b -> {c, d} -> e (v3 merge) -> f -> g -> r (v2 recovery of c on
    mainline b) -> h -> i; a legacy line a -> x -> y. Authored one day apart."""
    docs = [uid("plan-doc-1"), uid("plan-doc-2")]
    spec = [
        # name, parents, version, authored day, d1 state, d2 state, bytes
        ("p0", [], 1, 1, "s0", None, 310),
        ("p1", ["p0"], 1, 2, "s1", None, 320),
        ("p2", ["p1"], 1, 3, "s2", None, 330),
        ("a", ["p2"], 1, 4, "s3", None, 340),
        ("b", ["a"], 1, 5, "s4", "u0", 350),
        ("c", ["b"], 1, 6, "s5", "u0", 360),
        ("d", ["b"], 1, 6, "s6", "u0", 370),
        ("e", ["c", "d"], 3, 7, "s7", "u0", 800),
        ("f", ["e"], 1, 8, "s8", "u1", 380),
        ("g", ["f"], 1, 9, "s9", "u1", 390),
        ("r", ["g"], 2, 10, "s10", "u1", 700),
        ("h", ["r"], 1, 11, "s11", "u1", 400),
        ("i", ["h"], 1, 12, "s12", "u1", 410),
        ("x", ["a"], 1, 4, "s13", None, 420),
        ("y", ["x"], 1, 5, "s14", None, 430),
    ]
    commits = []
    for name, ps, version, day, s1, s2, size in spec:
        files = [[docs[0], label("state:" + s1)]]
        if s2:
            files.append([docs[1], label("state:" + s2)])
        entry = {
            "name": name, "address": label("commit:" + name), "version": version,
            "parents": [label("commit:" + p) for p in ps], "recovery": [],
            "authored_at_ms": day * DAY_MS, "bytes": size, "files": files, "inputs": [],
        }
        if name == "e":
            entry["inputs"] = [{"document": docs[0], "base": label("state:s4"), "left": label("state:s5"), "right": label("state:s6")}]
        if name == "r":
            entry["recovery"] = [label("commit:c"), label("commit:b")]
        commits.append(entry)
    # Object graphs: every state root has one own object and one chunk shared by a
    # group of neighbouring states; the d2 states share one chunk each.
    objects, state_objects = [], []

    def add_object(size, children):
        objects.append({"size": size, "children": children})
        return len(objects) - 1

    chunks = {}

    def chunk(key, size):
        if key not in chunks:
            chunks[key] = add_object(size, [])
        return chunks[key]

    seen = set()
    for c in commits:
        for document, root in c["files"]:
            if (document, root) in seen:
                continue
            seen.add((document, root))
            name = next(n for n in ["s%d" % i for i in range(15)] + ["u0", "u1"] if label("state:" + n) == root)
            group = "k0" if name in ("s0", "s1", "s2") else "k1" if name in ("s3", "s4", "s5", "s6", "s7") else "k2"
            if name.startswith("u"):
                group = "ku" + name[1:]
            shared = chunk(group, 1000 + 100 * len(group))
            own = add_object(50 + len(name), [shared])
            state_objects.append({"document": document, "state_root": root, "object": own})
    return {
        "project": uid("plan-project"),
        "branches": [
            {"branch": "main", "commit": label("commit:i"), "generation": 7},
            {"branch": "legacy", "commit": label("commit:y"), "generation": 2},
        ],
        "commits": commits,
        "skeleton": [],
        "holds": [{"commit": label("commit:e"), "reason": "audit"}],
        "objects": objects,
        "state_objects": state_objects,
    }


def plan_scenarios():
    view = build_view()
    base = {
        "version": 1, "id": "history-3", "tier": "history",
        "window": {"keep_last_commits": 3},
        "protect": {"branch_heads": True, "merge_bases": {"against": "main", "branches": ["*"]}},
    }
    scenarios = []
    views = {}

    def add(name, view_, policy, as_of=None, list_n=3):
        text = json.dumps(policy, ensure_ascii=False)
        result = plan(view_, policy, as_of, list_n)
        sets = result.pop("_sets")
        view_name = next((k for k, v in views.items() if v is view_), None)
        if view_name is None:
            view_name = "view-%d" % len(views)
            views[view_name] = view_
        scenarios.append({
            "name": name, "view": view_name, "policy": text, "as_of_ms": as_of, "list": list_n,
            "plan": canon(result), "plan_hash": result["plan_hash"],
            "retained": sets["retained"], "released": sets["released"], "skeleton": sets["skeleton"],
        })
        return result, sets

    first, sets1 = add("history_basic", view, base)
    add("content_tier", view, dict(base, id="content-3", tier="content"))
    add("topology_drop", view, dict(base, id="drop-3", topology="drop", acknowledge_unmergeable_history=True))
    add("time_window", view, {
        "version": 1, "id": "days-3", "tier": "history", "window": {"keep_within_days": 3},
        "protect": {"branch_heads": True}}, as_of=12 * DAY_MS + DAY_MS // 2)
    add("merge_inputs_none_content", view, dict(base, id="content-none", tier="content", protect=dict(base["protect"], merge_inputs="none")))
    add("no_window_only_heads_and_floor", view, {
        "version": 1, "id": "floor-1", "tier": "history", "window": {"min_commits_kept": 1},
        "protect": {"branch_heads": True}})
    ghost_view = dict(view, holds=view["holds"] + [{"commit": label("commit:ghost"), "reason": "no such commit"}])
    add("warnings", ghost_view, dict(base, id="warn", protect={
        "branch_heads": True, "merge_bases": {"against": "nope", "branches": ["*"]},
        "commits": [label("commit:ghost2"), label("commit:b")]}))
    add("explicit_pins", view, dict(base, id="pins", window={"min_commits_kept": 1}, protect={
        "branch_heads": True, "commits": [label("commit:p1"), label("commit:g")]}))
    # A second horizon: the retained commits of the first plan plus its skeleton.
    keep = set(sets1["retained"])
    kept_commits = [c for c in view["commits"] if c["address"] in keep]
    live_states = {(d, r) for c in kept_commits for d, r in c["files"]}
    second = dict(
        view,
        commits=kept_commits,
        skeleton=sets1["skeleton"],
        state_objects=[s for s in view["state_objects"] if (s["document"], s["state_root"]) in live_states],
    )
    add("second_horizon_is_idempotent", second, base)
    return {"views": views, "scenarios": scenarios}


# ---- output --------------------------------------------------------------------------
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
    graph = graph_vector()
    records = record_vectors(graph)
    for entry in records["envelopes"]:
        envelope = json.loads(entry["canonical"])
        address = record_address(envelope["horizon"])
        results = [
            ed_verify(bytes.fromhex(s["key"]), signing_input(address), bytes.fromhex(s["signature"]))
            for s in envelope["signatures"]
        ]
        # Self-check: the good envelopes verify, and each rejected one has a bad signature
        # or a structural problem the Rust side must name.
        if entry["expect"] == "ok" and not all(results):
            raise SystemExit("envelope %s should verify" % entry["name"])
    policies = {
        "valid": [
            {"name": name, "input": text, "min_signatures": floor,
             "normalized": canon(normalize_policy(json.loads(text))),
             "hash": policy_hash(normalize_policy(json.loads(text)))}
            for name, text, floor in VALID_POLICIES
        ],
        "invalid": [
            {"name": name, "input": text, "min_signatures": floor,
             "issues": [{"path": p, "code": c} for p, c in issues]}
            for name, text, floor, issues in INVALID_POLICIES
        ],
    }
    out = {
        "description": "Horizon record v1 vectors (docs/HORIZON-RECORD-V1.md). Written by gen_horizon_vectors.py; never edit by hand and never regenerate to make a test pass.",
        "domains": {k: v.decode("latin-1").replace("\x00", "\\0") for k, v in DOMAINS.items()},
        "commit_signature_domain": COMMIT_SIGNATURE_DOMAIN.decode("latin-1").replace("\x00", "\\0"),
        "rfc8032_vectors_checked": rfc8032,
        "empty_root": EMPTY_ROOT,
        "project": PROJECT,
        "policies": policies,
        "merkle": merkle_vectors(),
        "heads": heads_vectors(),
        "graph": graph,
        "records": records,
        "plans": plan_scenarios(),
    }
    target = os.path.join(os.path.dirname(os.path.abspath(__file__)), "horizon-v1.json")
    with open(target, "w", encoding="utf-8", newline="\n") as f:
        f.write(emit(out) + "\n")
    print("wrote %s (%d bytes)" % (target, os.path.getsize(target)))


if __name__ == "__main__":
    sys.exit(main())
