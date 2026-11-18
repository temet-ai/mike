# Building through mbx

`mbx` (github.com/jdx/mr-boxington, 1.22.0) is a build cache for Rust. It
restores compiler outputs that an earlier build, in any checkout or build
directory on this machine, already produced, and it gives the builds that run
through it one shared processor and memory budget. A trial on 5 October 2026
restored the binary of crate `mike` at a second checkout path in 5 s against 130 s
cold.

What it is for here: seats of a fleet share one `target/` and queue behind
cargo's lock. With mbx, a seat can build into its own directory in the same
checkout and have identical units restored, so it does not queue.

## The default, and the opt-in

`scripts/cx` sends `check`, `clippy`, `test`, `nextest` and `build` through mbx
whenever mbx is installed and no other build directory is named, into
`target/mbx-<session>` (`select_build_cache` in `scripts/lib/preflight-toolchain.sh`),
so a new worktree restores from the store instead of building cold. `nextest` runs as
`cargo-nextest` with `CARGO` set to mbx, so the `cargo test --no-run` it issues to build
the test binaries goes through the store and nextest only runs them. `build` with
`MIKE_BUILD_IDENTITY=git` (`make install`, the release scripts) stays on plain cargo,
which embeds the commit through build.rs; the deploy build stamps it instead (below).
Every uncached subcommand stays on plain cargo, because mbx runs what it does not cache
as plain cargo inside its own directory. Agents check and test through `scripts/cx
check` and `scripts/cx test` (a project hook refuses raw cargo builds), a deploy runs
once per landed change, not after each step, and heavy builds queue one at a time on
the machine's build lock, at the lowest priority on half the processors
(`scripts/lib/heavy-build.sh`). Plain cargo is refused
in an `mbx-*` directory, and `MIKE_BUILD_CACHE=cargo` forces plain cargo. What
follows describes the explicit form, `MIKE_BUILD_CACHE=mbx` with a named directory,
which the default uses under the hood.

```
MIKE_BUILD_CACHE=mbx CARGO_TARGET_DIR=target/mbx-a scripts/cx check -p mike --lib --tests
```

With `MIKE_BUILD_CACHE=mbx`:

- the cargo subcommand runs through mbx (`MBX_BIN`, else `mbx` on `PATH`, else
  `/mnt/data/mbx-trial/bin/mbx`; a clear error when none exists);
- `CARGO_TARGET_DIR` must name the build directory. `target` and
  `target/fleet-check` are refused, because plain cargo builds there;
- mbx's store is `MBX_CACHE_DIR`, default `~/.cache/mike-mbx`;
- sccache and `RUSTC_WRAPPER` are off, and the lld `--config` arguments cx adds
  for plain cargo are not passed;
- `MBX_TARGET_VIEWS=0` and `MBX_TARGET_SEED=0` are set, so mbx never moves,
  links or collects a real `target/`;
- mbx's automatic collection stays on (`gc.auto`): a build trims the store to
  `gc.max_size` once an hour, and further when the disk has less free than
  `gc.min_free_size`. With it off the store reached 567 GB and filled `/` on
  16 November 2026;
- `MBX_SHARE_WORKSPACE_ROOT=1` is set, so a workspace crate compiles to the same
  bytes in every checkout (see "14 November 2026" below).

- `crates/mike/build.rs` gives the build a fixed identity (sha, short sha and
  time `mbx-build`, dirty `false`) instead of the commit and the clock. Without
  it the compile key of crate `mike` changed on every build and mbx reported
  "compiler environment input changed: MIKE_BUILD_SHA". A build without the
  opt-in reports the real commit as before.

## Diagnose disk pressure before cleaning

Distinguish the shared **mbx store** (`MBX_CACHE_DIR`, normally
`~/.cache/mike-mbx`) from a checkout's **build directory** (`CARGO_TARGET_DIR`,
such as `target/mbx-<session>`). They have different owners and cleanup paths.
The `scripts/cargo-sweep-all` target sweep is not a substitute for mbx store
collection. Its default discovery scope spans other projects too; inspect its
`--dry-run` output before considering a sweep. Do not run plain Cargo in an
mbx build directory or use `cargo clean` as a cache-repair shortcut.

Start with read-only checks on the affected machine: identify the full filesystem,
the configured store and target paths, their sizes, and any active builds. Check
the actual checkout's `prepare_mbx_build_cache` implementation and installed mbx
version/configuration rather than assuming a setting described in a PR is live.
Do not delete a shared cache or an active build directory based only on its size.

[PR #332](https://github.com/temet-ai/mike/pull/332) merged as `4bf85f78`.
Source at `dd8e8e8c` unsets `MBX_GC_AUTO`, replacing the earlier forced-off
override so mbx's own collection configuration can apply,
while keeping target views and seeding disabled. Its reported disk-full incident
is not proof of recovery, and neither the merge nor this guide demonstrates
that collection ran or reclaimed enough space. Check the installed source,
installed mbx settings and post-collection free space separately. Store collection
does not by itself reclaim checkout target directories.

The same PR adds a Linux user-systemd background scope for heavy builds.
That is resource scheduling, not disk cleanup, and its effect depends on the
machine's actual slice configuration. It is not a shipped guarantee that builds
cannot interfere with desktop work.

## Deploy builds

`scripts/deploy-proxy.sh` builds mike through mbx into `target/mbx-deploy`, under
`nice -n 19 ionice -c 3`, and skips the build when the live binary was deployed from
HEAD and reports it (`.deploy/mike.deployed.commit` and `.deploy/mike.deployed.profile`).
The identity problem is solved outside the cached units: `crates/mike-base`
(`change_log/build.rs`) holds a 256-byte slot that opens with
`MIKE-BUILD-STAMP-SLOT-V1:`, and `current_build()` reads it with a volatile read.
Every crate compiles with the fixed `mbx-build` identity, so a commit changes no
compile key; after the build, the deploy copies the binary to `.deploy/mike.staged`
(a new file, never a hard link into the store) and
`scripts/lib/stamp_build_identity.py` writes `sha`, short sha, dirty and `built_at`
into the slot (re-signing ad hoc on macOS). `/api/health`'s `build` and `--build-info`
both read the slot; an unstamped slot falls back to the compiled identity. The deploy
refuses a staged binary whose `--build-info` is not HEAD. `--build-only` builds, stamps
and checks without a cutover; `MIKE_DEPLOY_BUILD_CACHE=cargo` returns to plain cargo.

`scripts/test-cx-build-cache` checks this without building anything.

## Infra deploy builds

infra (aarch64) never builds Rust. `scripts/deploy-infra [--sha <commit>]` builds one
commit on omit, copies the archives to `infra:~/mike-deploy/<short>/` and runs
`scripts/deploy-release-build --local` there (`--build-only` stops after the build). It
builds with `scripts/build-local-release aarch64-unknown-linux-gnu --deploy`, which differs
from a release build in three ways, and only for a deploy:

- profile `infra` (release without cross-crate LTO, 64 codegen units) instead of `dist`
  (thin LTO);
- through mbx into `target/mbx-deploy-<target>` of the `deploy-<target>` worktree, via
  cargo-zigbuild with `CARGO` set to the cargo shim (`mbx_cargo_shim`), on half the
  processors under the build lock, where a release runs one job on plain cargo;
- `SOURCE_DATE_EPOCH` is kept out of the compile, because `build_identity.rs` reruns on
  it. mike compiles with the fixed `mbx-build` identity and is stamped with the commit;
  mike-mcp has no stamp slot and no workspace dependency, so it compiles with the commit's
  identity, and only that crate recompiles per commit.

The archives have the release shape (manifest `build.profile` is `infra`, `build_cache`
`mbx`), so infra's installer verifies and installs them unchanged
(`scripts/lib/deploy-archive-build.sh`, `prepare.py --deploy-build`). Release builds keep
`dist`, one job and plain cargo.

Measured on omit on 15 November 2026, aarch64, the whole `build-local-release` run (SPA,
compile, packaging, qemu smoke), with other sessions idle:

| Path | Cold | After a one-file change (`mike-features`) |
| --- | --- | --- |
| release (`dist`, one job, plain cargo) | 2197 s | 1919 s |
| deploy (`infra`, mbx) | 813 s | 504 s |

The release path rebuilds every workspace crate on any commit: it compiles the commit's
identity into `mike-base` and sets `SOURCE_DATE_EPOCH`, which every workspace build script
reruns on. The deploy path restores every dependency and every workspace crate below the
edited one. The 504 s, from the build directory's timestamps: about 1 min for the fetch,
the SPA and the restores, `mike_features` 2m 10s, `mike_lessons` beside it, the `mike`
library 4 min, its link 1.5 min and `mike_mcp` 25 s. A change only in crate `mike` skips
the first two. What is left is the crates above the change, which no cache can skip; a lower
`opt-level` would shorten them, and has not been measured.

## Three traps

1. **A build directory used through mbx holds read-only hard links** into the
   store. Plain cargo must never build in it: it would write through them.
   Use plain cargo only in `target/` and `target/fleet-check`; use the opt-in
   only in other directories.
2. **Never together with sccache.** mbx and sccache both want to be the compiler
   wrapper. The opt-in turns sccache off; do not set it back.
3. **Never let mbx move `target/`.** Its automatic target management would
   relocate the directory other seats build in. The three `MBX_*` variables above
   prevent it; do not unset them.

## The scripts

- `one-build.sh` builds `check -p mike --lib --tests` into `target/mbx-a`, then
  into `target/mbx-b`, one after the other, niced, with six build jobs. It
  prints seconds, units restored and the lowest available memory for each, and
  stops when available memory falls under 5 GB.
- `two-at-once.sh` starts the same two builds together and records both times and
  the lowest available memory; it stops when a `STOP` file appears in its output
  folder or memory falls under 5 GB. Run it once, when no seat is building. It
  decides whether two builds in one checkout, each in its own directory, finish
  sooner and stay within memory compared with the same two queued behind one
  lock; if they do, seats can be moved to their own directories.

## 6 October 2026: why the first build did not store crate `mike`

**Cause found in mbx's documentation, not yet proven by a recorded build.**
mbx's default `learned_incremental = true` (docs/incremental.md in
jdx/mr-boxington) compiles a workspace crate whose source has changed since the
checkout's last recorded build with private incremental state, and never stores
that result in the shared store. In a fleet checkout crate `mike` always counts
as edited, so the first build compiled it incrementally (the nine incremental
compilations of 5 October) and stored nothing; the second build then missed,
and only the third could restore. The 5 October reports agree: build 1 stored
dependencies only ("0 hits ... 927.8 MiB stored", crate `mike` 1m 56s uncached),
build 2 had 576 hits and 6 misses with crate `mike` compiled (1m 8s).

**Change.** With the opt-in, `scripts/lib/preflight-toolchain.sh` now exports
`MBX_LEARNED_INCREMENTAL=0`, so crate `mike` is compiled and stored like any
dependency. `scripts/test-cx-build-cache` checks that the variable is set.
`one-build.sh` now reads `CARGO_BUILD_JOBS` (default 6).

**Mechanism reproduced on a one-file scratch crate** (mbx 1.22.0, fresh store
each time, `MBX_SUMMARY=full`; cheap enough to run whatever the memory):

| Setting | Build | mbx's lines for the crate |
| --- | --- | --- |
| default | 1, new directory | could not look up 1; 4.3 KiB stored |
| default | 2, source edited | 1 miss; "1 compilations kept their own incremental state, so they were not stored"; 0 B stored |
| default | 3, new directory | 1 miss; 3.7 KiB stored |
| default | 4, new directory | 1 hit |
| `MBX_LEARNED_INCREMENTAL=0` | 1, new directory | could not look up 1; 4.3 KiB stored |
| `MBX_LEARNED_INCREMENTAL=0` | 2, source edited | 1 miss; 3.7 KiB stored |
| `MBX_LEARNED_INCREMENTAL=0` | 3, new directory | 1 hit |

So the edited crate is stored only with the setting off: with the default, a
restore starts at the fourth build (the pattern seen for crate `mike`), with the
setting off at the third build, one build earlier, and the edit build itself
is stored. This establishes the mechanism and the fix on mbx 1.22.0; it does not
replace the build of crate `mike` below, which was never completed.

**Proof on crate `mike`: not recorded.** On 6 October four attempts at a fresh-store, two-build
proof (`CARGO_BUILD_JOBS=2` to 6, `MBX_CACHE_DIR` in a scratch folder) were
stopped by the script's own guard (available memory fell to 4.8 to 5.0 GB while
other seats built) and two more failed on other seats' uncommitted files. A
last attempt (`CARGO_BUILD_JOBS=2`, guard lowered to 1.5 GB, fresh store) built
every dependency and reached `Compiling mike`, then was ended at 347 s when
available memory fell to 1.3 GB; it wrote no stats report, so it says nothing
about crate `mike`. What is established without a completed build: the 5 October
reports (build 1 stored dependencies only; build 2 compiled `mike` in 1m 8s),
mbx's documented rule, and that the opt-in now turns that rule off. Nothing has
been tried and ruled out; the cause is neither confirmed nor excluded. To record
it once the machine has room:

```
CARGO_BUILD_JOBS=2 MBX_CACHE_DIR=<empty folder> OUT=<folder> scripts/mbx-trial/one-build.sh
```

Pass if the second build's report lists crate `mike` as a hit (restored) and
not under `slowest uncached crates`. If it is still compiled, the cause above
is ruled out and the next suspects are the compile key differing between the
two directories and the store being written only on a clean build.

### Recorded on crate `mike`, 6 October, after the fleet

Run by the lead on `main` at 52bf4b83, nothing else building, 14 GB or more
available, `CARGO_BUILD_JOBS=6`, a fresh `MBX_CACHE_DIR` each time.

| Build | Second directory's report | Crate `mike` |
| --- | --- | --- |
| `check -p mike --lib` into `target/mbx-c`, then `target/mbx-d` | c: 72 s, 898.9 MiB stored; d: 3 s, 543 hits, 2 misses | **restored** in d (not under slowest uncached; mime_guess 0.4 s was) |
| `one-build.sh` (`--lib --tests`) into `target/mbx-a`, then `target/mbx-b` | a: 120 s, 181.9 MiB stored; b: 48 s, 26 hits, 1 miss | compiled in b: `slowest uncached crates: mike 45.74s` |

So the fix works for the library: a second build directory restores crate
`mike` from a store the first build wrote, from the second build on. The one
miss left is the crate's own test build (`--tests`), which is compiled again in
the second directory; its cause is not established. A seat's narrow loop
(`cx check --lib`) gets the restore; a check with `--tests` does not yet.

## 14 November 2026: why a new worktree rebuilt every workspace crate

On the Mac (mbx 1.22.0, `cx check -p mike --lib`), a new worktree restored
everything when the last build of that command was of the same source: 83 hits,
0 misses. It rebuilt every workspace crate when another checkout had built changed
source in between. That is the everyday case, a branch in the main checkout and a
worktree on main. Omit behaves the same way.

There are two causes. mbx looks a unit up only by a key it predicts from the last
recorded build of the same command. A new checkout inherits those predictions, so
when that build had a different file list for `mike-base` (one added module is
enough), the predicted key misses and `mike-base` compiles again. That costs one
crate. The second cause makes it cost all of them. `mike-base` embeds its checkout
path, through the absolute `.sql` paths that `query_file!` includes. So the
recompiled library differs in bytes from the stored one (mbx: "result was not
stored: local action key already has a different result"), and every crate that
depends on it gets a new key.

`MBX_SHARE_WORKSPACE_ROOT=1` removes the second cause. The same three steps on the
Mac were these: build the commit in worktree A, build A's commit plus one empty
module in worktree B, then build A's commit in a new worktree.

| Setting | New worktree's report | Crates compiled |
| --- | --- | --- |
| unset | 631 hits, 9 misses; 1m 5s compiling | `mike_base`, `mike_launch`, `mike_fleet`, `mike_features`, `mike` and the rest |
| `MBX_SHARE_WORKSPACE_ROOT=1` | 638 hits, 2 misses; 18 s | `mike_base` only (10.4 s), plus `mime_guess` |

The tests that read files through `CARGO_MANIFEST_DIR` (`mike-base`
`analysis::corpus`, including the committed-file comparison) pass with the setting.
The setting is part of every workspace crate's key, so the first build after it
changes compiles the workspace crates once, and a checkout without it (a branch
that has not merged main) keeps separate entries. The test builds of a crate that
reads `CARGO_MANIFEST_DIR` stay keyed to their checkout.
