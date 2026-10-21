# Platform and processor support

Status: written 21 October 2026 from the CI runs linked below. It states what a run
proved and nothing else. A row is true for the commit its run names; re-read the run
before relying on it.

The policy behind this page is decision 016 of the Dynodoc product repository
(20 October 2026): the engine, command line and servers run with full feature parity
on the processors people use (Intel/AMD and Arm, including Apple silicon).

## Support matrix

| Platform | Tier | Runner label | Built and tested on every change | Prebuilt `engine-cli` and `engine-api` |
| --- | --- | --- | --- | --- |
| Linux x86-64 | 1 | `ubuntu-latest` | [`ci.yml`](../.github/workflows/ci.yml) | `x86_64-unknown-linux-gnu` |
| Linux aarch64 | 1 | `ubuntu-24.04-arm` | [`platforms.yml`](../.github/workflows/platforms.yml) | `aarch64-unknown-linux-gnu` |
| macOS aarch64 (Apple silicon) | 1 | `macos-latest` | `platforms.yml` | `aarch64-apple-darwin` |
| macOS x86-64 | 1 | `macos-15-intel` | `platforms.yml` | `x86_64-apple-darwin` |
| Windows x86-64 | 1 | `windows-latest` | `platforms.yml` | `x86_64-pc-windows-msvc` |
| Windows aarch64 | 2 | `windows-11-arm` | `platforms.yml`, non-blocking (`continue-on-error`) | `aarch64-pc-windows-msvc` |

Tier 1: a failing build or test on that platform fails the workflow. Tier 2: built
and tested, a failure is reported but does not block, nothing stronger is promised.
Not supported: 32-bit targets, RISC-V, big-endian, musl (the Linux binaries link
glibc), and Linux distributions older than the glibc floor listed below.

## What CI verifies

Evidence run: [engine-platforms 39493540615](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/39493540615)
(commit `7346a61`; later commits change only a temporary trigger, comments and this
page). Every job builds with `cargo build --workspace --locked`, runs
`cargo test --workspace --locked --no-fail-fast` against PostgreSQL, regenerates
`openapi.yaml` and compares it, and exercises the real operating-system key store.
Tests run one at a time (`RUST_TEST_THREADS=1`, as in `ci.yml`) with
`CARGO_PROFILE_DEV_DEBUG=0` and `CARGO_INCREMENTAL=0`.

| Platform | Image | PostgreSQL | Build | Clippy | Tests passed / failed / ignored | SSE timing test | Real key store | Job time |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Linux aarch64 | `ubuntu-24.04-arm`, Ubuntu 24.04 (20261018.154.1) | 16.15, service container | pass | not run (see below) | 1285 / 0 / 4 | passed, blocking | Secret Service (gnome-keyring): pass | 13 min |
| macOS aarch64 | macOS 26.6.2, Apple silicon (`macos-latest`) | 16.15, Homebrew, `en_US.UTF-8` | pass | pass | 1285 / 0 / 4 | passed, blocking | Keychain (temporary keychain): pass; login keychain left clean | 16 min |
| macOS x86-64 | macOS 15.7.9, Intel (`macos-15-intel`) | 16.15, Homebrew, `en_US.UTF-8` | pass | not run | 1284 / 0 / 4 | excluded; alone: passed | Keychain (temporary keychain): pass; login keychain left clean | 23 min |
| Windows x86-64 | Windows Server 2025 Datacenter 10.0.26100 (`windows-latest`) | 17.11, from the runner image, UTF-8, `en-US` | pass | pass | 1285 / 0 / 4 | excluded; alone: **failed** | DPAPI: pass | 22 min |
| Windows aarch64 (tier 2) | Windows 11 Enterprise 10.0.26200, ARM64 (`windows-11-arm`) | 16.15, x64 build under emulation, UTF-8, `en-US` | pass | not run | 1285 / 0 / 4 | excluded; alone: **failed** | DPAPI: pass | 40 min |
| Linux x86-64 | `ci.yml` | `ci.yml` | `ci.yml` | `ci.yml` | `ci.yml` | `ci.yml` | Secret Service (gnome-keyring), job `linux-x64-key-store`: pass | 2 min |

Counts differ by platform only through `cfg(unix)` and `cfg(windows)` tests and through
the excluded timing test. "Ignored" is the same four tests everywhere (see below). All
six jobs concluded `success`. The Windows jobs check out with CRLF line endings
(`core.autocrlf=true`).

Earlier runs of the same workflow that informed the fixes below:
[39470062867](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/39470062867),
[39472623633](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/39472623633),
[39477250840](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/39477250840),
[39482886733](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/39482886733),
[39488859505](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/39488859505).

## Excluded or different in CI, and why

- **`sse_delivers_a_new_event_within_100ms`** (`crates/engine-api/tests/api_test.rs`) asserts
  that a server-sent event arrives within 100 ms. It is a latency check, not a
  behaviour check, and it is not weakened. It is excluded from the blocking run with
  `--skip` on Windows x86-64, Windows aarch64 and macOS x86-64, and run again alone in a
  non-blocking step whose PASSED or FAILED is written to the job summary. Measured so
  far: `windows-latest` failed in every recorded run (3 of 3 runs), `windows-11-arm` failed in
  every recorded run (2 of 2 runs), `macos-15-intel` failed once in four runs, while
  `ubuntu-24.04-arm` and `macos-latest` passed it in every run (6 and 5 blocking
  runs), so it still runs and blocks there. If it fails on one of those two, add
  `skip_sse` for that matrix entry rather than editing the test.
- **Four `#[ignore]`d tests** are not run on any platform, `ci.yml` included: the key
  store test (run separately, see below), a compression measurement, planning a hundred
  thousand commits, and a million-leaf transparency property. They are listed as
  "ignored" in the table.
- **Clippy** runs on macOS aarch64 and Windows x86-64 (the `cfg(unix)` and `cfg(windows)`
  code) and, via `ci.yml`, Linux x86-64. The workspace has no `target_arch` code, so
  the other jobs would lint the same sources. `cargo fmt --check` and `cargo doc` stay
  in `ci.yml` only.
- **PostgreSQL** is 16 everywhere except Windows x86-64 (17.11); see the limits.

## Platform differences and how each is tested

| Behaviour | Linux | macOS | Windows | Test |
| --- | --- | --- | --- | --- |
| Device key held by the OS (`device keygen --keystore`) | Secret Service through `secret-tool` (needs `libsecret-tools` and a running Secret Service such as gnome-keyring; otherwise refused with a pointer to the passphrase mode) | login keychain through `/usr/bin/security`, secret sent on stdin | DPAPI (`CryptProtectData`, current user, no UI); the protected blob lives in the key file | `keystore.rs` unit tests (no secret in command arguments, absent tool is a clear error, DPAPI binds account and entropy); `tests/native_keystore.rs` drives the real binary against the real store |
| Owner-only key and token files | mode `0600` set at creation; loaders refuse group or other access | same | a one-entry access list set with `icacls` and verified, creation fails closed | `private_file.rs` unit tests (including the two Windows cases below), `tests/key_files.rs`, `device.rs` and `sync` tests |
| Secret files for the API (`NAME_FILE`) and at-rest KEK files | group or other access refused unless `DYNODOC_ALLOW_OPEN_SECRET_FILES=true` | same | **not checked** (no mode bits); protect the folder with an access list | `secrets.rs` and `at_rest/provider.rs` tests (Unix only) |
| Paths | case sensitive; any byte except `/` | case insensitive on APFS by default; names are normalisation-insensitive | case insensitive; drive letters; either slash accepted; child tools print in the console code page | `tests/platform_behaviour.rs`: spaces, Bengali letters, `..` segments, forward slashes, relative paths; an existing key is never overwritten, even through another letter case |
| Line endings | LF | LF | `core.autocrlf=true` checks files out as CRLF; `migrations/*.sql` and `integrations/**` are pinned to LF by `.gitattributes` (SQLx hashes the migration bytes; a test requires LF plugin files) | the Windows jobs check out with CRLF on purpose; the generated `openapi.yaml` must contain no CR and match the checked-out one ignoring CR; `platform_behaviour.rs` opens a key with a CRLF passphrase file (one line ending is removed, a second is part of the passphrase) and checks that key files are written byte for byte |
| Hidden passphrase prompt | `stty -echo` on the terminal | same | console mode API | **not exercised in CI** (needs a terminal); passphrase files are the tested path |
| PostgreSQL used by the tests | `postgres:16` service container | Homebrew `postgresql@16`, `en_US.UTF-8` | see the results table | every job logs `select version()`, encoding and collation |

### The real key store test

`crates/engine-cli/tests/native_keystore.rs` is marked `#[ignore]` because it writes to
the signed-in user's key store; the platform workflow prepares a throwaway store and
runs it with `--ignored`. It creates a key with `device keygen --encrypt --keystore`,
opens it again through the store (`device public-key`), then unprotects it and proves
the stored secret is gone: a copy of the encrypted file must no longer open. On
Windows the DPAPI blob lives in the file, so the copy keeps opening for the same
account. Each child process is killed after 60 seconds so a surprise prompt fails the
test instead of hanging the job.

- macOS: a temporary keychain is created and unlocked, becomes the only entry of the
  search list and the default keychain, and is deleted afterwards. A following step
  checks that nothing was written to the login keychain. It worked headless on both
  macOS images without any prompt.
- Linux: `dbus-run-session` with a fresh, unlocked gnome-keyring; nothing outside the
  job is touched. Run on aarch64 and on x86-64.
- Windows: DPAPI needs no preparation.

To run it against your own key store (it creates and deletes one `dynodoc-key-v1`
entry): `cargo test -p engine-cli --test native_keystore -- --ignored`.

### Bugs this work found and fixed

All three were found by running tests: the first on a Windows PC while writing the
new path test, the other two on the CI runners.

1. **Windows, non-ASCII key path.** `icacls` prints in the console code page, so a
   Bengali or accented folder name came back as `?` and `device keygen`, `protect`
   and the sync token files failed closed with "unrecognised icacls output" for any
   such path or user name. The verification now compares the user name by its ASCII
   letters when the path or name is not ASCII (`private_file.rs`). Test:
   `platform_behaviour.rs::key_paths_with_spaces_bengali_letters_...` runs the real
   binary on a Bengali path; unit test with synthetic `icacls` output.
2. **Windows, elevated token.** A process running as Administrator creates files that
   already carry explicit SYSTEM and Administrators entries; dropping inheritance left
   them and the one-entry check refused every key and token file (12 tests failed on
   the runner). `icacls` now also removes those two by well-known SID in the same call
   (`private_file.rs`). Test: a Windows-only test reproduces the state.
3. **Windows checkout, plugin files.** A default Windows clone turned the Claude Code
   plugin files (`integrations/`) into CRLF and `mcp_plugin.rs` rejected them.
   `.gitattributes` now pins `integrations/**` to LF.

## Verify a downloaded release

Each `cli-v*` release has one archive per target and a `SHA256SUMS` file. An archive
holds `engine-cli`, `engine-api`, `LICENSE` and a short `README.md`.

```sh
sha256sum --check --ignore-missing SHA256SUMS      # Linux
shasum -a 256 -c SHA256SUMS --ignore-missing        # macOS
```

```powershell
Get-FileHash .\dynodoc-engine-cli-<version>-x86_64-pc-windows-msvc.zip -Algorithm SHA256
```

Then `engine-cli --version`. `engine-api` has no `--version`; started with no
configuration it exits with the list of missing settings, which is the smoke test the
release workflow uses.

**The binaries are not signed or notarised.** The project has no code-signing
certificate yet. A checksum shows that a file is the one the workflow built; it does
not show who wrote the source.

- macOS Gatekeeper blocks a downloaded unsigned binary. After the checksum matches, run
  `xattr -d com.apple.quarantine engine-cli engine-api`, or open it once with a right
  click and Open. A file fetched with `curl` carries no quarantine flag.
- Windows SmartScreen shows an unknown-publisher warning. After the checksum matches,
  choose More info, then Run anyway.
- Linux shows no warning; `chmod +x` the files.

What a release binary needs from the system, read from the binaries themselves in the
release runs (sizes are the archives):

| Target | Archive | Needs |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | about 15 MB | glibc 2.34 or newer (highest symbol version `GLIBC_2.34`; built on the Ubuntu 22.04 image), `libgcc_s`, `libm` |
| `aarch64-unknown-linux-gnu` | about 14.5 MB | glibc 2.34 or newer (`GLIBC_2.34`) |
| `x86_64-apple-darwin` | about 15 MB | macOS 11 or newer (`LC_BUILD_VERSION` minos 11.0), system libraries only |
| `aarch64-apple-darwin` | about 14 MB | macOS 11 or newer (minos 11.0) |
| `x86_64-pc-windows-msvc` | about 14.5 MB | the C runtime is linked statically: no `VCRUNTIME140` import, so no Visual C++ redistributable; Windows 10 or newer is Rust's own minimum for the target and was not tested on a Windows 10 host |
| `aarch64-pc-windows-msvc` | about 13.5 MB | same, on Windows on Arm |

glibc 2.34 covers Ubuntu 22.04 (2.35), Debian 12 (2.36) and RHEL 9 (2.34). The floor is
set by the runner image the Linux archives are built on; when GitHub retires the
Ubuntu 22.04 image the floor rises to that of the next image unless a container is used.

## Releases

[`release-cli.yml`](../.github/workflows/release-cli.yml) runs on a pushed tag `cli-v*`
and on manual dispatch. It builds natively on a runner of each target, runs every
binary it built, unpacks the archive into a clean directory and runs that too, and
writes `SHA256SUMS`. A manual run stops at workflow artifacts (kept 30 days). Only a
tag push creates a GitHub Release; that last step has not been exercised, see below.

Verification (each of these runs built all six targets natively; none created a release):

- Every build job ran its own binaries on a runner of the target processor:
  `engine-cli --version`, `engine-cli --help`, and `engine-api` with no configuration
  (it must exit non-zero and name `DATABASE_URL`). It then packed the archive,
  unpacked it into a clean directory and ran `engine-cli --version` from there.
  Runs: [39472623743](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/39472623743),
  [39482886720](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/39482886720),
  [39488859525](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/39488859525),
  [39493540654](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/39493540654).
- Archives were also downloaded from the last run to a Windows PC and checked against
  `SHA256SUMS`. The Windows x86-64 one ran there (`engine-cli --version` printed
  `engine-cli 0.1.0`; `engine-api` refused to start without configuration; an earlier
  archive's `device keygen` produced a key file whose access list names only the
  current user). The Linux x86-64 one ran in a clean `ubuntu:22.04` container (glibc
  2.35). The Linux aarch64 one ran in an `ubuntu:22.04` `linux/arm64` container under
  QEMU emulation. The macOS and Windows aarch64 archives were run only by their own
  jobs on native runners, not downloaded and run elsewhere.
- **Not exercised: the tag path.** The `publish` job (`gh release create` on a
  `cli-v*` tag) has not run, because a test would publish a release in the public
  repository. It is guarded by `github.event_name == 'push'` and a `cli-v` tag ref,
  and was skipped in every run above. Before the first real release, push a throwaway
  tag such as `cli-v0.0.0-test`, check the release, then delete the release and the tag.

## Container image

[`containers.yml`](../.github/workflows/containers.yml) builds the repository's
`Dockerfile` (engine-api and engine-cli on `debian:bookworm-slim`, non-root user 10001)
natively for `linux/amd64` (`ubuntu-latest`) and `linux/arm64` (`ubuntu-24.04-arm`). Each
job checks the image's architecture, runs `engine-cli --version`, runs `engine-api` with
no configuration (it must exit non-zero and name `DATABASE_URL`) and checks the user id.
Run [39531119708](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/39531119708)
passed on both. The images are not signed and carry no SBOM. **Not exercised: the
publish path.** A pushed `image-v*` tag pushes `<tag>-amd64` and `<tag>-arm64` to GHCR
and combines them into one multi-architecture `<tag>`; it needs `packages: write`, never
runs for a branch, pull request or manual run, and has not been run. Before the first
real tag, push a throwaway one and delete the package version afterwards.

## Known limits

- Tier 2 means what it says: Windows aarch64 has no native PostgreSQL. Its job
  downloads the EDB x64 build of PostgreSQL 16.15 (zip, pinned by a SHA-256 taken from
  a download on 20 October 2026, so trust on first use) and runs it under the
  operating system's x64 emulation; the engine under test is native aarch64. That
  job takes about 40 minutes, mostly slow emulated database tests.
- Windows x86-64 uses the PostgreSQL 17.11 that the runner image ships, not 16 (the
  policy version); its tests pass on 17. Both Windows clusters are created fresh with
  UTF-8 and the `en-US` collation. macOS uses `en_US.UTF-8`; Linux uses the
  `postgres:16` image default. The suite passing on all of them says the engine does
  not depend on collation order, not that every collation is equivalent.
- Hosted macOS Arm runners have limited capacity: jobs queued for 10 to 20 minutes and
  one release job was cancelled with "the job was not acquired by Runner of type
  hosted"; a re-run fixes it.
- Linux Secret Service needs a running Secret Service. Headless servers usually have
  none, in which case `--keystore` is refused with a pointer to passphrase mode. On
  macOS, a locked login keychain in a headless session can make `security` ask for
  a password; the tests used an unlocked temporary keychain, so what an ordinary user's
  keychain does is not tested.
- Secret files read by the API (`NAME_FILE`) and at-rest KEK files are not permission
  checked on Windows (see the table); the hidden passphrase prompt is not tested in CI.
- Tests run on the debug profile. The release binaries are only smoke tested
  (`--version`, `--help`, the configuration refusal), not run through the suite.
- Not covered anywhere: real hardware and devices, 32-bit, big-endian, RISC-V, musl,
  Linux distributions other than Ubuntu 24.04 (tests) and 22.04 (release builds), macOS
  before 15 and Windows 10 hosts (the macOS 11 and Windows 10 floors are build
  settings, not test results), and the Android and iOS applications.
- The binaries are unsigned and not notarised.

## Running the checks yourself

```sh
cargo build --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked          # needs DATABASE_URL for PostgreSQL 16 (README)
cargo test -p engine-cli --test platform_behaviour
cargo test -p engine-cli --test native_keystore -- --ignored   # writes to your key store
```
