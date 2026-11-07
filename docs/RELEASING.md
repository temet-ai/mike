---
title: Preparing mike + mike-mcp release candidates
status: active
created: 2026-07-23
updated: 2026-11-07
summary: Where the 0.1.0-alpha.N release stands (0.1.0-alpha.4 is published), the release route (tag-triggered public repo workflow, Homebrew tap and mise, glibc 2.35 floor), archive preparation and verification; qualification and same-byte publication remain separate gates.
---

# Preparing mike + mike-mcp release candidates

[`RELEASE_ACCEPTANCE_MATRIX.md`](RELEASE_ACCEPTANCE_MATRIX.md) lists
the cases a candidate is judged on.
**A prepared candidate is not a qualified or published release.**

## Where things stand (2026-11-07)

- **Repositories.** `temet-ai/mike` is public. It was created on 2026-11-06 from a
  collapsed export of the private development repository (the public-export step,
  which lives in the private repository), so it carries none of the development history. The tap `temet-ai/homebrew-mike`
  is public and holds `Formula/` and a README.
- **Current release.** `0.1.0-alpha.4` is published as a pre-release, with archives for
  Linux x86_64 and Apple Silicon, from the `Release alpha` run on its tag.
- **The tap.** `HOMEBREW_TAP_TOKEN` is set but cannot write to the tap: on alpha.4 the
  push failed with 403, so `publish` shows as failed although the release itself was
  published. The formulas of alpha.3 and alpha.4 were written by hand with
  `scripts/release/homebrew_alpha.py`. A token with write access to `temet-ai/homebrew-mike`
  ends this. A real install and `brew test` of both alpha.4 formulas pass on Apple Silicon.
- **mise needs two tools.** The `github:` backend takes one archive per tool, so `mike`
  comes from `github:temet-ai/mike` and `mike-mcp` from
  `ubi:temet-ai/mike[matching=mike-mcp,exe=mike-mcp]`. `install.md` has the one line; it
  passes on Linux.
- **alpha.1 did not build.** The export dropped files the `mike` crate embeds, so the
  public tree did not compile. The `Release alpha` run passed `validate`, failed the
  Linux build, had its macOS build cancelled and skipped `publish`.
- **alpha.2 has no Apple Silicon archive.** Its macOS job (`macos-14`) did not run:
  GitHub retired that image on 2 November, and the job shows as cancelled. `publish`
  needs the archives of both targets, so it was skipped. alpha.3 builds on `macos-15`.
- **Scope of the alpha.** The bar is install and run, plus every case that can lose
  data (AC01, AC02, AC03, AC07, AC08, AC22; see
  [`RELEASE_ACCEPTANCE.md`](RELEASE_ACCEPTANCE.md)). There is no pilot. AC23 and AC24
  and the upgrade, removal and reinstall journeys are not part of the alpha.
- **Evidence.** No acceptance run is committed to this repository. A case counts only
  from a ledger written by `scripts/release-acceptance` on a candidate.

`scripts/release.sh` prepares local archives. It never changes workspace versions,
tracked Homebrew formulae, tags, branches, releases, or tap repositories.
`--publish` fails immediately, before tool checks or builds. The separate
[publication tools](RELEASE_PUBLICATION.md) require qualification evidence,
permission preflight, exact-byte verification and explicit operator approval.
They never rebuild a candidate and do not make an unqualified artifact releasable.

## The route to a release

mike ships by the route radar (gg) ships by:

- **Version and tag.** `0.1.0-alpha.N`, tagged `v0.1.0-alpha.N`. Every release is a
  public alpha.
- **Source and builds.** The public repository `temet-ai/mike` holds the source and
  builds the release archives on its free GitHub-hosted runners: Linux x86_64 on
  `ubuntu-22.04`, Linux ARM64 on `ubuntu-22.04-arm` and Apple Silicon macOS on `macos-15` (GitHub retired `macos-14` on
  2 November). The private development repository runs no release workflow: its
  Actions are disabled.
- **Channels.** Homebrew through the tap `temet-ai/homebrew-mike`
  (`brew install temet-ai/mike/mike temet-ai/mike/mike-mcp`; the full names matter,
  because current Homebrew refuses a short name from an untrusted tap) and mise
  (`github:temet-ai/mike`). There is no curl installer.
- **Linux floor.** Linux archives run on glibc 2.35 (Ubuntu 22.04) and newer.
  A binary built on this development host needs glibc 2.39 and is never a
  Linux candidate. The floor is enforced by where the archive is built (the
  `ubuntu-22.04` runner) and by `python3 scripts/release/glibc_floor.py
  <candidate>/manifest.json`, which reads the `readelf --version-info` report
  in the manifest and exits 1 when either binary references a `GLIBC_` version
  above 2.35 or the report shows no versions. Run it on every Linux candidate
  before acceptance.
- **Not signed.** macOS archives are unsigned; Homebrew and mise are the
  supported routes.

The repository, the tap and the two tags already exist. Pushing another tag, or
publishing a release by any route, needs the operator's explicit yes; nothing in
this repository does it on its own.

## The tag-triggered alpha workflow

`.github/workflows/release-alpha.yml` is radar's `release-alpha.yml` adapted to
mike. Pushing a tag `v0.1.0-alpha.N` to the public repository runs it; a manual
dispatch builds and packages without publishing.

1. **validate** (`ubuntu-22.04`). The workspace version must be an alpha SemVer and,
   on a tag, equal the tag without its `v`. It also runs
   `scripts/release/test_homebrew*.py`. Bump `[workspace.package].version` and
   `Cargo.lock` to `0.1.0-alpha.N` in a reviewed commit before tagging; the
   workflow never edits a version.
2. **build** (one job per target, `fail-fast: false`): `x86_64-unknown-linux-gnu` on
   `ubuntu-22.04`, `aarch64-unknown-linux-gnu` on `ubuntu-22.04-arm` and
   `aarch64-apple-darwin` on `macos-15`. Each job installs the
   toolchain `scripts/release.sh` requires (the Rust version in
   `rust-toolchain.toml`, Node from `.nvmrc`, the pnpm in
   `frontend/package.json`), runs `cargo fetch --locked` and
   `pnpm install --frozen-lockfile` (the only network step, since preparation
   builds offline), then `scripts/release.sh <version> --target <target>`. Linux
   then runs `scripts/release/glibc_floor.py` on the manifest, and both run
   `scripts/release.sh --verify` on the candidate. The two archives, their
   `.sha256` files and `manifest.json` (as `manifest-<target>.json`) are uploaded
   as the job's artifact. The preparation log is kept outside the checkout,
   because preparation requires a clean tree to its end.
3. **publish** (tags only, `contents: write`). It needs the four archives, checks
   each `.sha256`, writes one `checksums.txt`, creates the pre-release
   `mike <version>` with the archives, `checksums.txt` and the two manifests,
   then updates the tap.

The tap step clones `temet-ai/homebrew-mike`, runs
`scripts/release/homebrew_alpha.py <version> <repo> checksums.txt tap/Formula` and
pushes `Formula/mike.rb` and `Formula/mike-mcp.rb` to the tap's default branch. The
formulas pin Apple Silicon macOS, Linux x86_64 and Linux ARM64 archives by the digests just
published and share their install, caveats and test methods with the offline
proposal in `homebrew_proposal.py`. Without the secret `HOMEBREW_TAP_TOKEN` the
step warns and succeeds; write the formulas by hand with the same command from
the release's `checksums.txt`.

**Public repository configuration.** The secret `HOMEBREW_TAP_TOKEN` (a token that
can push to `temet-ai/homebrew-mike`) and the existing tap repository. Actions are
enabled on the public repository (all actions allowed), with the default
`GITHUB_TOKEN` allowed to write contents on the tag workflow (the job asks for it
itself). Tags matching `v*-alpha.*` should be protected from deletion. No other
secret is read.

**Scope.** Only the three targets `scripts/release.sh` can build natively. There is no
Intel macOS archive until the script supports it. The workflow
publishes only when every target builds, so a failure on one holds the alpha for all
three. It does not run the acceptance matrix: a published archive is an alpha, not a
qualified release, and the tap formulas are not the qualified `homebrew.py`
route below. The three real tag runs so far are described under
[Where things stand](#where-things-stand-2026-11-06); alpha.3 is the first to reach `publish`.

Preparation uses one Cargo build job and disables incremental compilation to
keep native local verification resource-bounded. Ambient Cargo resource overrides
do not change this build contract.

## The guide on temet.ai

The user guide in `docs/user-guide` is published at <https://temet.ai/docs/mike/>.
The website shows the guide of the latest published release, so its pages describe
the mike that Homebrew and mise install. Once a release is published, run:

```sh
scripts/publish-user-guide
```

It fetches `docs/user-guide` of the public repository at the release's tag, runs
`scripts/check-user-guide --complete` on it, copies it into a checkout of the
website's repository with that repository's own sync, page-list check and build,
commits it there, pushes the website's main and waits until the website reports that
commit live. It reads the public repository only, so a page that is not public
cannot reach the website. It refuses a website checkout that is not on main, has
unpushed commits or has changed tracked files other than the guide's copy. The
website's checkout is the folder `temet-ai` beside this repository unless `--site`
or `MIKE_SITE_CHECKOUT` names another. When the guide adds or removes a page, the
command stops and names the page list in the website's repository that needs the
same change.

`scripts/publish-user-guide --status` changes nothing. It prints which guide the
website's pushed main holds and how the latest release, the public main and this
checkout compare with it, and exits 1 when the website holds neither the guide of
the latest release nor a later one from the public main. Run it after a release to
confirm the website followed.

Between releases the guide on main may describe what is not released yet, and that
text stays off the website. A correction that should not wait for the next release
goes to the public main first and is then published by hand:

```sh
scripts/publish-user-guide --ref main
```

Four tests in `crates/mike/tests/cli/user_guide.rs` hold the guide to the code. They
fail when `docs/user-guide/command-line.md` does not name a `mike` command, when a
page shows a command mike does not have, when `settings.md` does not name a feature
switch, and when `pages.md` does not name an entry of the web UI's menu.

## What is prepared

- `mike`: the proxy and inspect API, with the production SPA built first
- `mike-mcp`: the separately installed MCP stdio bridge

Both inherit `[workspace.package].version`. `Cargo.lock` must agree with that
version for both packages. `proxy-forward` is excluded.

The first target is native `x86_64-unknown-linux-gnu`. Native
`aarch64-apple-darwin` uses the same preparation contract on Apple Silicon, and
native `aarch64-unknown-linux-gnu` the same on Linux ARM64. The reliability journeys
and isolated acceptance runner remain x86_64 only.
Cross-compilation and Intel macOS are unsupported by this script.
The macOS path's fixture tests do not qualify a real Mac archive or its install
channel. All three scoped platforms need their own real artifact and clean-account
observations.

Mike archives also carry the tracked runtime files from `mods/` under
`share/mike/mods/`, excluding mod tests. The manifest hashes each file and records
its executable mode. Archive installs retain these files in the same immutable
generation as the binary; Homebrew installs them under the formula's share
folder. Both layouts are discovered by `mike claude-code mods` without a checkout.
The forward proxy is the `mike forward` subcommand in the Mike binary.

## Prerequisites

Use a clean, reviewed Git commit, with no tracked edits or untracked source files.
Make a version change in a separate reviewed commit before preparation; passing a
different version to the script is an error and does not edit or reset anything.

Provision these tools and dependencies before running the script:

- Python 3.11 or newer, Bash, Git, and the native platform tools
- The exact Rust/Cargo version in `rust-toolchain.toml`, already installed for
  the host; no rustup target or toolchain installation occurs
- Node matching `.nvmrc` and the exact pnpm version in
  `frontend/package.json`'s `packageManager`
- Cargo and pnpm dependency caches sufficient for a locked, offline build
- `readelf` on Linux, or `otool` on macOS, for recording runtime requirements

The manifest records actual tool versions. `.nvmrc` pins Node `26.11.1` exactly,
and preparation rejects a major-only pin or a different installed version.
Native system linkers/libraries still require a reviewed build baseline.

No `.env` file is sourced. Inherited database URLs, `MIKE_*`, `VITE_*`, Rust
flags, build-cache identity overrides, and release credentials are not forwarded
to build tools. Active frontend `.env`, `.env.local`, `.env.production`, and
`.env.production.local` files are rejected because the frontend build could
load them itself. Use a dedicated clean checkout rather than moving private
configuration around a live checkout.

Cargo uses `--offline --locked`, `SQLX_OFFLINE=true`, and the committed `.sqlx`
metadata. pnpm uses `install --offline --frozen-lockfile`. Missing tools or cache
entries fail with an error; the script does not install tools or download a
missing toolchain. End users should eventually receive these prebuilt binaries,
not run this maintainer build procedure.

## Prepare and inspect locally

The script resolves its checkout from its own location; the caller can start in
an unrelated directory. Preparation must have exclusive use of that checkout's
frontend build directory. Do not edit source or run a competing frontend build
while it runs.

```bash
./scripts/release.sh

# Optional explicit version must already equal Cargo.toml and Cargo.lock.
./scripts/release.sh 0.1.0

# Optional target must match this machine and its Rust host.
./scripts/release.sh --target x86_64-unknown-linux-gnu
```

Preparation performs these steps:

1. Reject dirty source, mismatched versions, unsupported/cross targets, missing
   prerequisites, and unsafe build inputs before building.
2. Acquire `dist-out/.prepare.lock`. A second preparation fails instead of
   racing the SPA or Cargo outputs. If a process is killed, first establish that
   it has stopped before removing its stale lock.
3. Remove the old ignored `frontend/build` directory, build the current SPA, and
   require a nonempty `index.html` plus JavaScript assets. Reject symlinks and
   obvious private-file extensions. This is not a comprehensive secret scan.
4. Build both binaries with the `dist` profile in `dist-out/.build`, ignoring an
   inherited `CARGO_TARGET_DIR`. Previous binary outputs are removed so a failed
   or incomplete build cannot silently reuse them.
5. Package each binary with `LICENSE` and `NOTICE.json`. The notice explicitly
   marks third-party notice review as incomplete; it does not claim that the
   project's MIT license covers every dependency or embedded font. The `mike`
   archive also carries `mike-maintenance` and its deterministic standard-library
   module bundle, `mike-maintenance.pyz`. Both are separately listed and hashed
   in the archive inventory. This optional Bash/Python 3.11+ kit provides local
   installation/recovery; neither native binary depends on it to run.
6. Extract the archives with a strict file allowlist, regular-file/mode checks,
   and member hashes. Run archived `--version` for both binaries, `mike --help`,
   and `mike-mcp list-tools --json` from an unrelated temporary directory with a
   new HOME and empty PATH. Also run each archived `--build-info` and require its
   product/version, full source commit, clean-source flag, and UTC build timestamp
   to match the selected commit and its `SOURCE_DATE_EPOCH`. A matching semantic
   version alone cannot establish executable source identity.
   Separately exercise the archived maintenance helper's help with isolated
   Python and only its explicit interpreter/system-tool path. Its smoke output
   hash is recorded without qualifying database recovery or maintenance journeys.
7. Record runtime library/loader/symbol-version reports using `readelf` or
   `otool`. These reports are evidence for review, not a compatibility verdict.
8. Confirm source still matches the original clean commit, then finalize the
   candidate under `dist-out/v<VERSION>/<TARGET>/<SOURCE_SHA>-<MANIFEST_ID>/`.

Each candidate contains:

- `mike-v<VERSION>-<TARGET>.tar.gz` and `.tar.gz.sha256`
- `mike-mcp-v<VERSION>-<TARGET>.tar.gz` and `.tar.gz.sha256`
- `manifest.json`

Checksum files use relative basenames, so they can be verified after copying the
whole candidate directory elsewhere. Archives normalize ownership, modes, order,
and timestamps. Candidates are content-addressed and never overwritten; a repeat
with identical bytes reuses the existing directory. This does not promise that
recompiling the same source yields identical binaries: native toolchain and
other build inputs can still change. Both binaries derive their embedded build
time from the validated commit `SOURCE_DATE_EPOCH`, rather than the wall clock.
Qualification and future publication must use the selected archive, without
rebuilding it.

The manifest includes the source commit, workspace and observed executable
versions, tool versions, lockfile hashes, SPA source-asset inventory, archive and
member hashes, the observed JSON build identity of each executable, runtime
reports, and explicit passed/not-run checks. It always says `qualified: false`. Source-asset inventory does not prove the embedded UI
works from the archived executable. The manifest is local evidence, not a signed
attestation or proof that an untrusted archive is authentic.

## Verify the same archived bytes

Use the exact directory printed after `Candidate:`:

```bash
./scripts/release.sh --verify /path/to/selected/candidate
```

Verification needs Python and the native runtime, but does not invoke Cargo,
Rust, Node, pnpm, a build database, or publication tools. It validates both
archive/checksum pairs and the allowlisted archive members, then repeats the
isolated CLI smoke checks. Any byte change, absolute-path checksum, unexpected
member, version, source identity, clean-source flag, or source timestamp mismatch
fails. It does not rebuild or edit the candidate.

## Executable source identity

`mike --build-info` and `mike-mcp --build-info` are standalone, read-only JSON
commands. They print only schema version, product, semantic version, source SHA,
source-dirty status, and build timestamp. They run before loading configuration or
starting services and do not print environment values or filesystem paths.
Additional arguments are rejected. Existing `--version` output is unchanged.

Both build scripts watch the selected Git worktree's `HEAD` and index plus its
existing loose/packed reference files. A detached linked worktree switching to a
Python-only commit therefore invalidates the embedded source stamp. Git status
failures are treated as dirty rather than silently claiming a clean build.
`SOURCE_DATE_EPOCH` changes also invalidate the build stamp; malformed or
out-of-range values fail instead of falling back to the current time. Ordinary
non-release builds without that variable still use the wall clock.

Preparation and same-byte verification execute the archived identity command and
compare its complete response to the manifest's selected commit and timestamp.
Unknown/cache-placeholder, dirty, stale, or mismatched identity is rejected for
either binary. Older candidate archives without this command, timestamp, or
recorded identity evidence cannot pass the current verifier and must be prepared
again from reviewed source. This is an exact-source consistency check, not a
signature, reproducibility guarantee, or proof of authenticity for untrusted
binaries; run verification only on explicitly trusted candidate bytes.

## Local archive installation and acceptance foundation

For explicitly trusted prepared bytes, the extracted archive's
`/absolute/extracted/mike-maintenance install --candidate /absolute/candidate
--prefix /private/user/prefix` installs without a source checkout. Keep the
adjacent `mike-maintenance.pyz` file with its launcher; retain the original
manifest and both archive/checksum pairs in the selected candidate directory.
The development `scripts/install-release` entrypoint uses the same implementation.
It validates the copied manifest, both archive/checksum pairs, native platform
and binary versions before changing the prefix. Bash and Python 3.11+ are optional
installation/maintenance prerequisites, not native runtime dependencies.
Checksums establish integrity, not source authenticity; validation executes the
selected binaries and helper. There is no download, compilation, sudo, service
restart, migration or shell-profile edit.

Stable commands live under `PREFIX/bin`. An owned immutable generation contains
both binaries and one atomic generation switch updates the pair. Repeating the
same candidate is a no-op; previous generations remain. Unrelated/package-manager
binaries, modified owned files, downgrades and changed same-version replacements
are refused. Interruption recovery is tested only in disposable fixture prefixes.
The hashed maintenance kit stays in that same generation and is invoked through
`PREFIX/lib/mike-archive/current/mike-maintenance`; the launcher pins the physical
generation before starting Python. No extra stable command is added to `PATH`.
See [recovery and optional prerequisites](RECOVERY.md) for backup, verification,
fresh-instance restore, update checks and data-preserving removal.

`scripts/release-acceptance --candidate /absolute/candidate --output
/new/outside-source/evidence` checks local integrity, disposable installation,
installed CLI smoke and repeat installation with fresh HOME/cwd and an empty
runtime PATH. Its private `result.json` and `summary.txt` bind observations to
candidate hashes. The bare form can record AC08 at most and leaves the other five
alpha cases (AC01, AC02, AC03, AC07, AC22) to the opt-in journeys; every other
case stays NOT_RUN. Exit 0 means all six alpha cases hold PASS, exit 2 means one
is still NOT_RUN, exit 1 means a helper failed. A PASS is a real journey's
observation on that candidate, not a release qualification, and does not replace
native accounts, browser/MCP, real-channel or Apple Silicon evidence.

## Running the acceptance matrix

`scripts/release-acceptance --candidate /absolute/candidate --output /new/outside-source/evidence`
runs the matrix against the one candidate directory `scripts/release.sh` printed after
`Candidate:`, in a fresh account, container or VM. The journeys, their flags, the
PostgreSQL 19 fixture they need and the ledger they write are described in
[`RELEASE_ACCEPTANCE.md`](RELEASE_ACCEPTANCE.md); the rows each journey must satisfy are
in [`RELEASE_ACCEPTANCE_MATRIX.md`](RELEASE_ACCEPTANCE_MATRIX.md).

## Where the matrix rows run

The Linux rows run through `scripts/release-acceptance` and `scripts/release-journey-mise`,
each in a fresh HOME; the Apple Silicon Homebrew row needs a Mac and stays NOT RUN until the
maintainer runs it through the Homebrew channel. Which helper covers which row, and what
the browser and reliability options add, is in
[`RELEASE_ACCEPTANCE_MATRIX.md`](RELEASE_ACCEPTANCE_MATRIX.md#where-the-harness-lives) and
[`RELEASE_ACCEPTANCE.md`](RELEASE_ACCEPTANCE.md).

## Local regression tests and remaining gates

```bash
scripts/test-release-preparation
```

Run `scripts/test-release-installation` for interruption, ownership, integrity,
cleanup and honest-acceptance fixture regressions, and `scripts/test-release-recovery`
for maintenance and checkout-free packaging fixtures. No test installs into the
operator's prefix or touches their data.

This suite is also registered in `scripts/test-suite scripts`. It uses isolated
Git fixtures, shell-script fixture binaries, and tool stubs, plus bounded
standalone `rustc` compilation of the build scripts (no workspace build). It
checks source purity, repeated preparation, version/pin/target failures, stale SPA/binary
rejection, failed builds, private-input filtering, unsafe archive paths,
checksums, locks, no-build verification, worktree HEAD/ref watches, deterministic
UTC epochs, failed-status handling, stale executable identities, and refusal to
publish. **Passing it is not a real Mike build, Linux ABI test, macOS test, or
clean-account installation.**

The alpha gate is the six cases in [`RELEASE_ACCEPTANCE.md`](RELEASE_ACCEPTANCE.md).
Before any claim of a qualified (non-alpha) release, also record the not-run
evidence below on the selected candidate:

- Real native builds and runtime baselines, with conservative Linux library
  requirements and actual execution on the supported distributions
- Embedded UI startup/assets/API and complete first-run behavior from the
  extracted binary, without the source checkout or developer dependencies
- Private-material review of archives and embedded assets, complete dependency
  license/notice inventory, and the separately required public-source review
- Safe archive/download installation, repeat installation, ownership checks,
  PATH guidance, upgrade/backup/restore, and incompatible-downgrade handling
- Actual macOS signature/quarantine behavior through the intended channel;
  a maintainer-machine run does not prove another user's execution policy
- Destination/tap permission preflight before any external mutation, draft
  upload/hash verification, same-byte retry, and immutable published tags/assets
- Final clean-account installation from the actual published URLs or Homebrew
  tap, with downloaded hashes matching the independently qualified archives

These are the AC01/AC08/AC23/AC25/AC26 obligations beyond the alpha bar (AC23,
AC25 and AC26 do not gate an alpha). Local preparation and helper tests alone
close none of them.

## cargo-dist and Homebrew

`dist-workspace.toml` keeps `ci = []` and both native targets: the release
workflow of the public repository calls `scripts/release.sh`, not cargo-dist.
Its cargo-dist pin and `[profile.dist]` remain, but preparation directly invokes
Cargo; it neither requires nor invokes cargo-dist.

`installers = []` avoids cargo-dist's multi-app Homebrew generation failure.
The tap `temet-ai/homebrew-mike` carries `Formula/mike.rb` and
`Formula/mike-mcp.rb`; the tracked `homebrew-formulae/` files are their staging
copy and are not regenerated during preparation. The tag workflow above writes the
tap's formulas with `homebrew_alpha.py`; the separate [same-byte publication guide](RELEASE_PUBLICATION.md)
describes offline archive planning and deterministic Homebrew formula proposals,
then separately gated read-only preflight and explicitly authorized publication.
Formula proposals pin the selected archive hashes and remain unqualified until
independent native review; no preparation command creates a tap, tag, draft or
public release. Actual native Homebrew and published-channel observations remain
NOT RUN until performed against the selected candidate.
