---
name: dynodoc-verify-stage
description: Runs the verification checklist for a Dynodoc build stage from docs/. Use when the user asks to verify, validate, or sign off a stage, or check if a stage is complete.
---

# Verify build stage

## Steps

1. Look up the stage in `.cursor/stage-map.md` and read the **Verification checklist** section in that doc.
2. Run each checklist item and record pass/fail with command output.
3. Cross-check `SETUP.md` for any already-documented status.
4. Report gaps as a short table: item | status | notes.

## Standard commands

```sh
task up
task migrate-reset    # schema stages only
task test
task lint
task fmt-check
curl -s http://127.0.0.1:8080/healthz
curl -s http://127.0.0.1:5173/healthz
```

## Schema stage extras (13)

```sh
cargo test -p engine-core --test schema_test
# psql: \dt, \di; UPDATE event should raise immutability error
```

## Event log stage extras (14)

```sh
cargo test -p engine-core
# verify_chain tests; proptest if present
```

If any item fails, fix before marking the stage complete.
