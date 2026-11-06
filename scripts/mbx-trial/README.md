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

## The opt-in

Nothing changes unless you ask. With `MIKE_BUILD_CACHE` unset, `scripts/cx` runs
exactly the command it always did.

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
- `MBX_TARGET_VIEWS=0`, `MBX_GC_AUTO=0` and `MBX_TARGET_SEED=0` are set, so mbx
  never moves, links or collects a real `target/`.

- `crates/mike/build.rs` gives the build a fixed identity (sha, short sha and
  time `mbx-build`, dirty `false`) instead of the commit and the clock. Without
  it the compile key of crate `mike` changed on every build and mbx reported
  "compiler environment input changed: MIKE_BUILD_SHA". A build without the
  opt-in, and the deployed binary, report the real commit as before. Binaries
  built through mbx are for checks, never for deploy.

`scripts/test-cx-build-cache` checks this without building anything.

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
