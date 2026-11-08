# Install mike

mike comes as two programs that you install together from the same release: `mike`, which captures and shows your sessions, and `mike-mcp`, which lets Claude Code query them. This page covers the three ways to get them and what each leaves on your machine.

## Before you install

- **Current release.** `0.1.0-alpha.5` is a pre-release, on the [releases page](https://github.com/temet-ai/mike/releases).
- **Platforms.** Linux on x86_64 or ARM64 with glibc 2.35 (Ubuntu 22.04) or newer, and Apple Silicon Macs. A Raspberry Pi needs a 64-bit OS, so `getconf LONG_BIT` should print `64`. There is no 32-bit ARM, Intel Mac or Windows build. macOS archives are not signed, which is why Homebrew and mise are the supported routes there.
- **Claude Code** must be installed and signed in before you capture anything. `claude --version` shows whether it is installed.
- **A database** is chosen later, in [setup](setup.md). Docker is needed only if you want mike to run a PostgreSQL for you.

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

The two programs are separate downloads from one release, so the line names both. Keep the quotes around the second one. Other versions are on the [releases page](https://github.com/temet-ai/mike/releases). Upgrade with `mise upgrade github:temet-ai/mike ubi:temet-ai/mike`.

## Install from an archive

An archive release is a folder holding a manifest and, for each program, an archive and its checksum file. Keep all of it together.

1. Unpack the `mike` archive.
2. Run the installer it contains, giving the folder that holds the manifest and a prefix you own:

   ```sh
   /absolute/path/to/unpacked/mike-maintenance install \
     --candidate /absolute/path/to/release-folder --prefix ~/.local
   ```

3. The installer needs Python 3.11 or newer and a native `mike`. It checks the manifest, both checksums and the platform before it changes anything, then puts `mike` and `mike-mcp` under `~/.local/bin`. It downloads nothing, uses no `sudo`, restarts no service and edits no shell profile.

Checksums show that the files are intact, not who made them. Install only files you trust, because the installer runs the programs to check them. Installing the same release again changes nothing; installing an older release over a newer one is refused.

## Check the install

```sh
mike --version
mike-mcp --version
```

Both print the version you installed (with the build's commit and time for `mike`). If a command is not found, add the folder the installer or package manager used to your `PATH`. Then continue with [setup](setup.md).

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
