#!/usr/bin/env python3
"""Independent generator for merge-policy-v1.json (Python standard library only).

Run:  python -I gen_merge_policy_vectors.py      (rewrites merge-policy-v1.json next
to this file; the output is deterministic, so a clean `git diff` afterwards shows the
committed file is reproduced, and a second run changes nothing.)

Nothing here shares code with the Rust module under test
(crates/engine-core/src/merge_policy). It implements the rules of the policy
schema `dynodoc.merge-policy.v1` that decide the canonical form and the hash:

  * the input is JSON text; unknown or duplicate keys and `null` never occur in
    the inputs below (the Rust loader rejects them; that is tested there);
  * the canonical form is the policy with every schema-level default written
    out (description omitted when absent, `params` always present, `floors` always
    present with a `gate_at_least` object), each rule's `match.type` list sorted
    (it is a set) and every object's keys sorted by code point, written compactly
    (separators `,` and `:`), strings with only the escapes JSON requires and
    non-ASCII characters left as UTF-8;
  * the hash is SHA-256 over the bytes b"dynodoc.merge-policy\\0v1\\0" followed
    by the canonical bytes.

Both this script and the Rust code were written by one author from the same
specification text. The vectors catch implementation bugs and a drift between the
two; they cannot catch a misreading of the specification both share. Per-strategy
parameters are not expanded to defaults (a strategy's defaults are frozen with its
versioned id), so a policy that spells a default out hashes differently from one
that omits it; `same_as` pairs below differ only in spelling that the schema
normalizes.

`test.params.v1` is a strategy the Rust test declares for these vectors only; it
takes the parameters `flag` (bool), `count` (integer 1..=200) and `mode`
(`dedupe` or `keep`).
"""
import hashlib
import json
import os

DOMAIN = b"dynodoc.merge-policy\x00v1\x00"
ROLES = ["owner", "manager", "editor", "contributor", "reviewer"]
DEFAULTS = {
    "unknown": "manual",
    "loser": "suggestion",
    "roles_rank": ROLES,
    "invariants": "flag",
}

# The text of crates/engine-core/src/merge_policy/default.rs `POLICY_JSON`; the Rust
# test requires the two to be byte-identical.
DEFAULT_POLICY = r"""{
  "version": 1,
  "id": "dynodoc.default.v1",
  "description": "Reproduces workspace_merge::merge: rich text and page setup merge by their own rules; every other overlap is a conflict.",
  "defaults": {
    "unknown": "manual",
    "loser": "none",
    "roles_rank": ["owner", "manager", "editor", "contributor", "reviewer"],
    "invariants": "flag"
  },
  "rules": [
    {
      "id": "legacy-content",
      "match": {"field": "content"},
      "strategy": "dynodoc.default.richtext.v1",
      "gate": "auto"
    },
    {
      "id": "legacy-page-setup",
      "match": {"field": "page_setup"},
      "strategy": "dynodoc.default.property.v1",
      "gate": "auto"
    },
    {
      "id": "legacy-whole-value",
      "match": {},
      "strategy": "dynodoc.default.whole-value.v1",
      "gate": "auto"
    }
  ],
  "floors": {"gate_at_least": {}}
}"""

FULL = (
    '{"version":1,"id":"research-default","description":"Text auto with review; sheets manual.",'
    '"defaults":{"unknown":"blocked","loser":"comment","roles_rank":["reviewer","owner"],'
    '"invariants":"conflict"},'
    '"rules":['
    '{"id":"text","match":{"family":"document","type":["paragraph","heading","codeBlock"],"field":"content"},'
    '"strategy":"dynodoc.default.richtext.v1","gate":"auto-with-review","description":"Text paragraphs."},'
    '{"id":"cells","match":{"family":"spreadsheet","field":"cell_*"},"strategy":"test.params.v1",'
    '"params":{"mode":"dedupe","count":200,"flag":true},"gate":"manual"},'
    '{"id":"comments","match":{"aspect":"comments"},"strategy":"dynodoc.collection.element-wise.v1","gate":"auto"},'
    '{"id":"suggestions","match":{"aspect":"suggestions"},"strategy":"dynodoc.collection.element-wise.v1","gate":"blocked"},'
    '{"id":"rest","match":{},"strategy":"dynodoc.default.whole-value.v1","gate":"auto"}'
    '],'
    '"floors":{"gate_at_least":{"test.params.v1":"manual","dynodoc.collection.element-wise.v1":"auto"}}}'
)

# The same policy as FULL: other key order, schema defaults spelled out, type sets
# listed in another order, insignificant whitespace.
FULL_RESPELLED = """
{
  "floors": {"gate_at_least": {"dynodoc.collection.element-wise.v1": "auto", "test.params.v1": "manual"}},
  "rules": [
    {"gate": "auto-with-review", "strategy": "dynodoc.default.richtext.v1", "description": "Text paragraphs.",
     "match": {"field": "content", "type": ["heading", "codeBlock", "paragraph"], "family": "document"}, "id": "text", "params": {}},
    {"id": "cells", "gate": "manual", "strategy": "test.params.v1",
     "match": {"field": "cell_*", "family": "spreadsheet"}, "params": {"flag": true, "count": 200, "mode": "dedupe"}},
    {"id": "comments", "match": {"aspect": "comments"}, "strategy": "dynodoc.collection.element-wise.v1", "gate": "auto"},
    {"id": "suggestions", "match": {"aspect": "suggestions"}, "strategy": "dynodoc.collection.element-wise.v1", "gate": "blocked"},
    {"id": "rest", "match": {}, "strategy": "dynodoc.default.whole-value.v1", "gate": "auto"}
  ],
  "description": "Text auto with review; sheets manual.",
  "defaults": {"invariants": "conflict", "roles_rank": ["reviewer", "owner"], "loser": "comment", "unknown": "blocked"},
  "id": "research-default",
  "version": 1
}
"""

# Characters outside ASCII are written as they are; quote and backslash are escaped.
UNICODE = (
    '{"version":1,"id":"unicode","description":"ক্ষ — \\"quoted\\" back\\\\slash \U0001f600",'
    '"rules":[]}'
)

# Parameters of every type, boundary values, a rule per gate.
PARAMS = (
    '{"version":1,"id":"params-edges","rules":['
    '{"id":"a","match":{"field":"a"},"strategy":"test.params.v1","params":{"count":1,"flag":false,"mode":"keep"},"gate":"auto"},'
    '{"id":"b","match":{"field":"b"},"strategy":"test.params.v1","params":{"count":200},"gate":"auto-with-review"},'
    '{"id":"c","match":{"field":"c"},"strategy":"test.params.v1","params":{"mode":"dedupe"},"gate":"manual"},'
    '{"id":"d","match":{"field":"d"},"strategy":"test.params.v1","gate":"blocked"}'
    ']}'
)

VECTORS = [
    ("minimal", '{"version":1,"id":"minimal","rules":[]}', None),
    ("default", DEFAULT_POLICY, None),
    ("full", FULL, None),
    ("full-respelled", FULL_RESPELLED, "full"),
    ("unicode-and-escapes", UNICODE, None),
    ("params-edges", PARAMS, None),
]


def reject_duplicates(pairs):
    out = {}
    for key, value in pairs:
        assert key not in out, "duplicate key " + key
        out[key] = value
    return out


def normalise(doc):
    out = {"version": doc["version"], "id": doc["id"]}
    if "description" in doc:
        out["description"] = doc["description"]
    defaults = dict(DEFAULTS)
    defaults.update(doc.get("defaults", {}))
    out["defaults"] = defaults
    rules = []
    for rule in doc["rules"]:
        match = dict(rule["match"])
        if "type" in match:
            match["type"] = sorted(match["type"])
        item = {
            "id": rule["id"],
            "match": match,
            "strategy": rule["strategy"],
            "params": dict(rule.get("params", {})),
            "gate": rule["gate"],
        }
        if "description" in rule:
            item["description"] = rule["description"]
        for value in item["params"].values():
            assert isinstance(value, (bool, int, str)), "parameters are bool, int or text"
            assert not isinstance(value, float)
        rules.append(item)
    out["rules"] = rules
    out["floors"] = {"gate_at_least": dict(doc.get("floors", {}).get("gate_at_least", {}))}
    return out


def encode_string(text):
    out = ['"']
    for ch in text:
        code = ord(ch)
        if ch == '"':
            out.append('\\"')
        elif ch == "\\":
            out.append("\\\\")
        elif code == 0x08:
            out.append("\\b")
        elif code == 0x0C:
            out.append("\\f")
        elif code == 0x0A:
            out.append("\\n")
        elif code == 0x0D:
            out.append("\\r")
        elif code == 0x09:
            out.append("\\t")
        elif code < 0x20:
            out.append("\\u%04x" % code)
        else:
            out.append(ch)
    out.append('"')
    return "".join(out)


def canonical(value):
    if isinstance(value, dict):
        items = ",".join(
            encode_string(key) + ":" + canonical(value[key]) for key in sorted(value)
        )
        return "{" + items + "}"
    if isinstance(value, list):
        return "[" + ",".join(canonical(item) for item in value) + "]"
    if value is True:
        return "true"
    if value is False:
        return "false"
    if value is None:
        return "null"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, str):
        return encode_string(value)
    raise TypeError(type(value))


def main():
    # SHA-256 sanity (FIPS 180-4 example) before anything depends on it.
    assert (
        hashlib.sha256(b"abc").hexdigest()
        == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    )
    vectors = []
    by_name = {}
    for name, text, same_as in VECTORS:
        doc = json.loads(text, object_pairs_hook=reject_duplicates)
        assert doc["version"] == 1
        form = canonical(normalise(doc))
        digest = hashlib.sha256(DOMAIN + form.encode("utf-8")).hexdigest()
        entry = {"name": name, "input": text, "canonical": form, "sha256": digest}
        if same_as is not None:
            assert by_name[same_as]["canonical"] == form, name
            entry["same_as"] = same_as
        by_name[name] = entry
        vectors.append(entry)
        assert len(form.encode("utf-8")) <= 16384
    # Two inputs that differ in content must not share a hash.
    assert len({v["sha256"] for v in vectors if "same_as" not in v}) == len(
        [v for v in vectors if "same_as" not in v]
    )
    document = {
        "schema": "dynodoc.merge-policy.vectors.v1",
        "hash": "sha256 over domain || canonical bytes",
        "domain_hex": DOMAIN.hex(),
        "vectors": vectors,
    }
    here = os.path.dirname(os.path.abspath(__file__))
    path = os.path.join(here, "merge-policy-v1.json")
    with open(path, "w", encoding="utf-8", newline="\n") as handle:
        handle.write(json.dumps(document, indent=2, ensure_ascii=False) + "\n")


main()
