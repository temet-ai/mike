# Install mike

mike comes as two programs that you install together from the same release: `mike`, which captures and shows your sessions, and `mike-mcp`, which lets Claude Code query them. This page covers the three ways to get them and what each leaves on your machine.

## Before you install

- **Current release.** `0.1.0-alpha.5` is a pre-release, on the [releases page](https://github.com/temet-ai/mike/releases).
- **Platforms.** Linux on x86_64 or ARM64 with glibc 2.35 (Ubuntu 22.04) or newer, and Apple Silicon Macs. A Raspberry Pi needs a 64-bit OS, so `getconf LONG_BIT` should print `64`. There is no 32-bit ARM, Intel Mac or Windows build. macOS archives are not signed, which is why Homebrew and mise are the supported routes there.
- **Claude Code** must be installed and signed in before you capture anything. `claude --version` shows whether it is installed.
- **A database** is chosen later, in [setup](setup.md). Docker is needed only if you want mike to run a PostgreSQL for you.

These examples select the published alpha.5 release, not a build from `main`.
[Match the guide to your version](README.md#match-the-guide-to-your-version) before expecting
newer settings or health fields to be present.

## Choose one installation method

| Choose | When it fits | Keep in mind |
|---|---|---|
| Homebrew | You already use Homebrew and want it to manage both programs | Use Homebrew for later updates and removal. |
| mise | You already use mise and want explicit version selection | Select the same release for both programs; a project-local version can override the global one. |
| Archive | You need a separate prefix managed by mike's maintenance kit | Follow the staging and checksum steps below. The archive kit does not manage Homebrew or mise installations. |

Use one method for the pair. Mixing methods can leave the shell, MCP registration and
background service pointing at different copies. The [install check](#check-the-install)
verifies what your shell will run; [Upgrade](backup-and-recovery.md#upgrade) also checks the
running server.

## Install with Homebrew

```sh
brew install temet-ai/mike/mike temet-ai/mike/mike-mcp
```

Homebrew installs both programs on your path and puts the Claude Code mods mike ships under the formula's share folder, where [`mike claude-code mods`](mods.md) finds them. Upgrade with `brew upgrade mike mike-mcp`, remove with `brew uninstall mike mike-mcp`.

## Install with mise

```sh
mise use -g github:temet-ai/mike@0.1.0-alpha.5 \
  'ubi:temet-ai/mike[matching=mike-mcp,exe=mike-mcp]@0.1.0-alpha.5'
```

The two programs are separate downloads from one release, so the line names both. Keep the quotes around the second one. Other versions are on the [releases page](https://github.com/temet-ai/mike/releases). For upgrades, [back up and choose the next version for both programs](backup-and-recovery.md#upgrade).

## Install from an archive

These steps are for Linux and need Python 3.11 or newer, `tar` and `sha256sum`.
On Apple Silicon macOS, use Homebrew or mise above; the archives are unsigned.

### Download the matching files

Open the [0.1.0-alpha.5 release](https://github.com/temet-ai/mike/releases/tag/v0.1.0-alpha.5).
Use `uname -m` to choose the target:

| Your Linux machine | Target |
|---|---|
| `x86_64` | `x86_64-unknown-linux-gnu` |
| `aarch64` | `aarch64-unknown-linux-gnu` |

Download these four assets from that one release into an otherwise empty folder, replacing
`<target>` with the target above:

- `manifest-<target>.json`
- `checksums.txt`
- `mike-v0.1.0-alpha.5-<target>.tar.gz`
- `mike-mcp-v0.1.0-alpha.5-<target>.tar.gz`

Choose the named release assets, not GitHub's **Source code** archives. Keep the two programs
from the same release and target. Checksums show that the files are intact, not who made
them. Use only release files you trust: the installer runs the programs to check them.

### Prepare the folder and install

The downloads folder is not yet an installer input. The installer expects a separate
**candidate** folder containing exactly five files: `manifest.json`, the two archives,
and one `.sha256` file beside each archive. It refuses extra files.

Open a terminal in your downloads folder and run the following block. It creates new
`candidate` and `mike-unpacked` folders and stops if either name is already in use.
It copies the expected hashes from the published `checksums.txt`, checks both archives
before unpacking, and stops on any failed command. It does not generate expected hashes
from the downloaded archives or run the installer.

```sh
(
  set -eu
  version=0.1.0-alpha.5
  case "$(uname -s):$(uname -m)" in
    Linux:x86_64) target=x86_64-unknown-linux-gnu ;;
    Linux:aarch64) target=aarch64-unknown-linux-gnu ;;
    *) echo "Use a supported Linux machine for these archive steps." >&2; exit 1 ;;
  esac
  mkdir candidate
  mkdir mike-unpacked
  cp "manifest-$target.json" candidate/manifest.json
  for binary in mike mike-mcp; do
    archive="$binary-v$version-$target.tar.gz"
    cp "$archive" "candidate/$archive"
    awk -v name="$archive" '
      $2 == name { print; matches++ }
      END { if (matches != 1) exit 1 }
    ' checksums.txt > "candidate/$archive.sha256"
  done
  (cd candidate && sha256sum --strict --check ./*.sha256)
  tar -xzf "candidate/mike-v$version-$target.tar.gz" -C mike-unpacked
)
```

Both checksum lines must say `OK` and the block must finish successfully. If this is an
upgrade, stop here and follow [Upgrade](backup-and-recovery.md#upgrade) before installing.
For a new archive installation, run:

```sh
./mike-unpacked/mike-maintenance install \
  --candidate "$PWD/candidate" --prefix "$HOME/.local"
```

The installer then checks
the manifest, archive contents, platform and both programs. On success it prints a JSON
receipt with `status: "installed"`, the version and the selected prefix; the stable commands
are `~/.local/bin/mike` and `~/.local/bin/mike-mcp`. Installing the identical release again
reports `status: "unchanged"`; installing an older release over a newer one is refused.

Keep `checksums.txt`, the original target-named manifest and the unpacked files outside
`candidate`. Do not put them back into it after verification. If staging fails, correct
the reported problem and use new staging folder names consistently before retrying;
do not continue straight to the installer after a failed checksum.

**Known alpha.5 installer limitation:** `archived maintenance smoke output changed` can
mean that Python formatted the maintenance helper's help text differently from the release
runner. The alpha.5 verifier compares that text's exact hash even across Python minor
versions; the fix was added after alpha.5 in
[commit d2c97891](https://github.com/temet-ai/mike/commit/d2c9789160a6d033188c67341a78b7db5c2d4ff6).
Python 3.11 or newer is required, but does not by itself avoid this failure. Do not edit the
manifest or bypass verification. Stop and use Homebrew or mise for a new installation, or
wait for a release containing the fix. If this is an upgrade of an existing archive-managed
installation, keep it and its backup intact rather than mixing installation methods.

The installer downloads nothing, uses no `sudo`, restarts no service and edits no shell
profile. [Backup and recovery](backup-and-recovery.md) separates the tested recovery paths
from the ones that remain untested.

## Check the install

```sh
command -v mike
command -v mike-mcp
mike --version
mike-mcp --version
```

The first two commands show the paths selected by this shell. Both programs should belong
to the method you chose, and both version checks must match the intended release.
`mike` also prints the build's commit and time. If a command is not found, add the folder the installer or package manager used to your `PATH`. Then continue with [setup](setup.md).

## What the install leaves

Installing puts the two programs, and the mods, on your machine. It does not start mike, create a database, change Claude Code or send anything anywhere. Removing the programs does not remove your history: see [Backup and recovery](backup-and-recovery.md) before you uninstall.

## When it does not work

| What you see | What to do |
|---|---|
| Homebrew refuses a formula from an "untrusted tap" | You used the short name. Install with the full names above, `temet-ai/mike/mike` and `temet-ai/mike/mike-mcp`. |
| `mike: command not found` after installing | Add the install folder (`~/.local/bin` for an archive install) to `PATH`, and open a new terminal. |
| The installer says it needs Python | Install Python 3.11 or newer. mike itself needs no Python. |
| The installer refuses a downgrade or a changed same-version release | Keep your backup and see [Backup and recovery](backup-and-recovery.md). |
| `mike` fails with a missing `GLIBC_` version on Linux | The machine is older than glibc 2.35, which mike does not support. |
