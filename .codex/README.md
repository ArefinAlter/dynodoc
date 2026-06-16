# Dynodoc Codex Assets

This directory mirrors the repo's existing `.cursor/` workflow support so Codex sessions can follow the same project-specific guidance.

## Included assets

- `agents/` - focused prompts for build stages, engine review, schema review, and PoC scope guarding
- `skills/` - reusable stage execution, verification, and architecture reference workflows
- `hooks/` - safety reminders for destructive commands and migration edits
- `hooks.json` - hook wiring metadata matching the Cursor setup

## Usage

- Start a stage by reading `AGENTS.md`, `.cursor/stage-map.md`, and the relevant `docs/*-build-*.md` file.
- Use the build-stage and verify-stage skills to keep implementation aligned with PoC scope.
- Treat `.codex/` as repo-local guidance; the canonical product and architecture rules still live in `docs/` and `AGENTS.md`.
