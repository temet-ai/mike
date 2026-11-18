# Back up and recover

Your history is the sessions mike has captured: in the guide's fictional example, the `docs` session in `~/projects/site` with the correction "Put Homebrew first on the install page." This page covers keeping that history safe: restarting mike, backing it up, restoring it into a fresh copy, upgrading and removing mike. mike leaves your history alone when you remove its programs.

Read the table first, because what works depends on how you installed:

| | Archive install | Homebrew or mise install |
|---|---|---|
| Backup, verify and restore with `mike-maintenance` | Works. Checked end to end on Linux for the first alpha. | Not available. The kit does not manage these installs. Back up by hand. |
| Backup and restore on macOS | Not tested | Not tested |
| Upgrade to the next release | Not rehearsed | Not rehearsed |
| Removal, then reinstall with the same history | Not rehearsed | Not rehearsed |
| `mike uninstall --purge` | Removes only what `mike db up` set up; see [Remove mike](#remove-mike) | Same |

"Not rehearsed" means the commands below exist and are described from the source, but nobody has run that path end to end for this release. Take a backup before you try them.

To see where your own history is, run `mike print-config --paths`. It prints the configuration file, data folder, state folder, log folder and the installation's `installation_id` as JSON, and contacts nothing.

## What needs to survive

Your history is in the database, and the database is not enough alone. Keep:

- **The whole PostgreSQL database.** Captures, settings, feature switches and the record of which database changes were applied. Do not back up selected tables.
- **The configuration file.** It holds the database address and any key you pasted into setup. A report from `mike print-config` is not a backup.
- **The data and state folders.** `mike --config <FILE> print-config --paths` prints them, with the installation's `installation_id`. For a managed Docker database, the state folder holds its password.
- **The matching `mike` and `mike-mcp`.** Keep the release you ran, so you can start a restored copy with the version that wrote it.

Backups hold complete conversations and credentials. Keep them where you keep the projects themselves, and do not attach one to an issue.

## Restart safely

1. Let running Claude Code sessions finish. Stopping mike can cut an answer short.
2. Stop mike: Ctrl-C in the terminal running `mike serve`, or the **Stop** screen of the control centre (`mike`). A background service starts itself again after a crash and at login; `mike service restart` restarts it and `mike service uninstall` stops and removes it. Closing a browser tab does not stop mike.
3. Start it again with the same configuration file: `mike serve`, or `mike --config <FILE> serve`. Check `mike print-config` if you changed any variables or flags.
4. Open [Sessions](reading-sessions.md) and confirm an earlier session is still there, then send a new prompt and confirm it is captured.

A `trial` installation keeps captures in memory. They are gone after a restart, and no backup can bring them back. If the database is a managed Docker one, `mike database stop` stops it and keeps the data; `mike database provision` starts it again and reuses the same database and volume. [Troubleshooting](troubleshooting.md) covers a database that will not come up.

## Back up

mike ships an optional maintenance kit, `mike-maintenance`, that makes a verified backup of the database, configuration, data and state in one step. It needs Bash and Python 3.11 or newer, and it works on installs made from the release archive with the kit's own `install` command (the kit is not on your path). **It does not manage Homebrew or mise installs.** If you installed that way, use [Back up by hand](#back-up-by-hand).

For an archive install the kit sits at:

```sh
MAINTENANCE="$HOME/.local/lib/mike-archive/current/mike-maintenance"
"$MAINTENANCE" --help
```

Its commands are `install`, `backup`, `verify`, `restore`, `upgrade-check` and `uninstall`. Database work also needs PostgreSQL 19 client tools (`psql`, `pg_dump`, `pg_restore`) for a database you run yourself, or Docker for the managed one. Tools are looked for in `/usr/bin:/bin`; if yours are elsewhere, pass `--tool-directory /absolute/path`, a folder you own that contains all three (or `docker`).

Before you start:

1. Stop mike, and every other program that writes to the same database. Leave PostgreSQL running. The flag `--confirm-stopped-writers` states you have done so; it does not stop anything.
2. Move any overrides that you pass as environment variables or flags into the configuration file, and remove them for the backup.
3. Set `CONFIG` to the absolute path of the configuration used by the installation you are backing up. `mike config path` prints the default selection; if you normally pass `--config`, use that file instead. Run `mike --config "$CONFIG" print-config --paths` and check its `config_file` is the one you intended.
4. Set `DATA` and `STATE` to the exact `data_dir` and `state_dir` from that output. For a managed Docker database, also set `INSTALLATION_ID` to its `installation_id`, not a container name or a value from another install.
5. Pick a new folder for the backup, outside those folders, with room for the database dump and a copy of the files. The example uses `~/mike-backups/before-update`; change it if that destination already exists.

The capitalised names below are shell variables you set, not values the kit discovers.
For a managed Docker backup, check them before continuing:

```sh
: "${CONFIG:?Set CONFIG to the selected configuration file}"
: "${DATA:?Set DATA from print-config --paths}"
: "${STATE:?Set STATE from print-config --paths}"
: "${INSTALLATION_ID:?Set INSTALLATION_ID from print-config --paths}"
```

These checks reject missing or empty values; they do not prove you selected the right
installation. Compare the paths with the selected installation before continuing.
For a managed Docker database, pass that `installation_id`:

```sh
"$MAINTENANCE" backup \
  --prefix "$HOME/.local" --config "$CONFIG" \
  --data-root "$DATA" --state-root "$STATE" \
  --docker-identity "$INSTALLATION_ID" \
  --destination "$HOME/mike-backups/before-update" --confirm-stopped-writers &&
"$MAINTENANCE" verify --backup "$HOME/mike-backups/before-update"
```

For a database you run yourself, replace `--docker-identity` with `--pg-service-file FILE --pg-service NAME`, where the file is a private PostgreSQL service file (host, port, database, user, authentication) that matches the database in your configuration. The kit never accepts a password on the command line.

A successful backup prints a JSON receipt with `status: "backup-complete"`; the follow-up
verification prints `status: "verified-checksums"`. Both commands must exit successfully.
The `&&` above runs verification only if this backup succeeded, so a failed attempt cannot
be mistaken for verification of an older backup at that path.

The destination appears only when every step succeeded. A failed backup exits non-zero and removes its partial work. A power cut can leave a folder named `.mike-recovery-incomplete-*`; that is not a backup. The kit does not overwrite an earlier backup and does not rotate old ones. `verify` checks the backup's files against their recorded checksums without needing a database; it does not prove a restore works.

### Back up by hand

mike provides and tests no command for this. If you installed with Homebrew or mise, you can still copy the same things yourself with standard tools: stop mike, copy your configuration file and the data and state folders from `print-config --paths`, and take a custom-format dump of the database. For the managed Docker database that is:

```sh
docker exec <container name> pg_dump -U mike -d mike -Fc > mike.dump
```

Find `<container name>` with `docker ps -a --filter label=io.temet.mike.managed-database`. A dump taken this way has not been through mike's own restore checks, so rehearse a restore on a spare machine before you rely on it.

## Restore into a fresh copy

A restore never writes over the instance you are recovering from. It builds a copy for you to review, and you start that copy yourself.

1. Leave the original installation and its database stopped and untouched.
2. Install the same release the backup came from into a new prefix. The kit compares the release itself, not just the version number.
3. Create a fresh, empty PostgreSQL 19 database (for the managed Docker database, a new installation with its own `installation_id`). Do not reuse the original volume.
4. Set `RECOVERY_PREFIX` to the new archive installation's prefix, and `FRESH_SERVICE_FILE`
   to the private service file for the fresh database. The example service name
   `mike_restore` must be an entry in that file. Confirm it does not point at the original
   database. Restore to a destination folder that does not exist yet:

```sh
"$MAINTENANCE" restore \
  --backup "$HOME/mike-backups/before-update" --prefix "$RECOVERY_PREFIX" \
  --pg-service-file "$FRESH_SERVICE_FILE" --pg-service mike_restore \
  --destination "$HOME/mike-restored" --confirm-stopped-writers
```

The kit refuses a target that already has tables. The import runs as one transaction, so an SQL error rolls it back. The destination folder contains `config.toml`, `data/`, `state/` and a small `RESTORED.json`; it is a place to review files, not a running install.

Then check the copy before you switch to it:

1. Open the copied `config.toml`. Its database address and paths still point at the original installation; change them to the new database and folders. Do not overwrite a freshly created database's password with the old one.
2. Run `mike --config <new config> database check`.
3. Start only the new copy, and open an earlier session in the web UI and through the [MCP companion](mcp.md).
4. Capture a new session.

If the import commits but a later check fails, leave that target alone and look at it before retrying; the kit does not empty it for you. Anything captured after the backup, and anything from a `trial` run, is not in it.

## Upgrade

An upgrade has three separate checkpoints: the new programs are installed, the new server
is running against the intended database, and old and new history are readable. A package
manager reporting success proves only the first one. The tested/not-rehearsed limits at the
top of this page still apply.

### Before changing versions

1. Choose a release from the [releases page](https://github.com/temet-ai/mike/releases) and
   read its notes. Keep `mike` and `mike-mcp` on the same release.
2. Record `mike --version`, `mike-mcp --version` and the configuration path you use. Keep
   the previous release files as well as the backup.
3. While the UI is available, note one older session you can find again after the upgrade.
   You will use it to check continuity; an empty but healthy new database is not a successful
   history migration.
4. Let active sessions finish, stop capture writers and [take a backup](#back-up). Keep
   writers stopped while checking and upgrading. Trial captures cannot survive this step.

### Install the selected pair

Use the channel that owns the current installation. Do not install a second copy with
another package manager to repair the first one; it can hide which binary the shell or
background service uses.

For Homebrew:

```sh
brew upgrade temet-ai/mike/mike temet-ai/mike/mike-mcp
```

For mise, choose the same explicit version for both entries. The example below selects
`0.1.0-alpha.5`; change `VERSION` to the release you chose if it is different:

```sh
VERSION=0.1.0-alpha.5
mise use -g "github:temet-ai/mike@${VERSION}" \
  "ubi:temet-ai/mike[matching=mike-mcp,exe=mike-mcp]@${VERSION}"
```

The installation guide pins a version. A bare `mise upgrade` preserves the configured
version range, so do not assume it selected a newer alpha. See
[mise's upgrade reference](https://mise.jdx.dev/cli/upgrade.html). Project-local mise
configuration can also override your global choice; check the commands from the project
folder where you use mike.

For an archive installation, stage the new release as described in
[Install from an archive](install.md#install-from-an-archive). That example selects alpha.5:
for your upgrade, download the selected newer release's manifest, checksums and both archives,
and set the staging block's `version` to that same release (without the leading `v`). Keep
the target consistent across all files. Use a new staging folder rather than an old candidate.
The installed kit's
`upgrade-check` takes the same location and database flags as `backup`, plus `--backup`
and `--candidate`. It checks that the backup still matches the stopped installation and
that the candidate is newer. Its receipt says `schema_compatibility: "NOT_RUN"`: it has not
proved the new program can use the database. After staging as shown in the install guide,
run `./mike-unpacked/mike --config "$CONFIG" database check` before installing it. The install command is
`mike-maintenance install --candidate <folder> --prefix <prefix>`.
Use the existing archive-owned prefix. The kit switches both programs together; it refuses
downgrades and changed files with the same version, and starts no service.

Then check what this shell will actually run:

```sh
command -v mike
command -v mike-mcp
mike --version
mike-mcp --version
```

Both versions must match the selected release. A different version usually means another
installation is earlier on `PATH`, or mise selected a project-local version. Fix that before
restarting anything.

### Check the database and restart

For durable storage, use the new binary and the same configuration file as the installation
you are upgrading. `mike migrate-check` and `mike database check` read the database without
applying migrations. With a custom configuration, use `mike --config "$CONFIG" migrate-check`
and `mike --config "$CONFIG" database check`.

Pending migrations alone are expected when a release adds them. A checksum mismatch,
missing-from-binary migration or out-of-order migration is a blocker: stop and read
[The program and the database disagree](troubleshooting.md#the-program-and-the-database-disagree).
Do not edit the migration ledger to make the check pass.

**Inspect a managed service's saved command before restarting it.** `mike service restart`
restarts that command; it does not regenerate it. This matters with version-specific
archive or mise paths. `mike service status` prints the service-file locations.

For a mike-managed service using the default configuration, run
`mike service install --dry-run` to see the command the new binary would write. If the saved
command points at the old binary, or you removed the service while stopping writers, review
the plan and use `mike service install` while the old service is stopped. It refreshes the
service files and starts the new server. Do not replace a custom service configuration
blindly.

Otherwise, start the foreground server with `mike serve` (or
`mike --config "$CONFIG" serve` for your custom file), or use `mike service restart` for an
existing managed service whose saved command already selects the new binary. Only
`mike serve` applies the new version's migrations on startup. After `mike-mcp` changes,
[check Claude's saved MCP registration](mcp.md#after-an-upgrade) before starting a new
Claude Code session. Restarting alone does not update an old executable path.

### Verify before calling it done

1. Open <http://localhost:9989/api/health>, or your configured API address. Compare
   `build.sha_short` with the commit printed by the new `mike --version`; the health response is
   from the running server, while `--version` describes the binary in your shell.
2. With durable storage, check `persistence.enabled` and `persistence.reachable` are both
   true, then open the older session you noted and read a request and reply.
3. Send a new prompt and find that exact turn in [Sessions](reading-sessions.md). Check
   [capture health](troubleshooting.md#check-that-history-is-being-saved) if the answer
   arrives but the turn does not.
4. If you use MCP, run `mike mcp probe` and read the same session from a newly started
   Claude Code session.

Keep the backup until you have verified those steps. A newer release can change the database
in ways an older program cannot read. Putting the old executable back does not undo those
changes. To go back, [restore the backup into a fresh copy](#restore-into-a-fresh-copy) with
the matching old release, leaving the changed database untouched.

## Remove mike

Remove the connections first, then the programs. Removing programs never removes your history.

1. Stop mike, and the background services if you installed them.
2. Run `mike uninstall`. It takes out the MCP entry in `~/.claude.json` (only if it points at `mike-mcp`), the shell function, the mods and route choice it added, and the background services. It keeps the database, its volume and your configuration file. If you registered the companion with `mike mcp register`, run `mike mcp unregister` too.
3. Remove the programs through the channel you used:

```sh
brew uninstall mike mike-mcp
```

```sh
mise uninstall github:temet-ai/mike@<version> ubi:temet-ai/mike@<version>
```

   For an archive install, `"$MAINTENANCE" uninstall --prefix "$HOME/.local" --config "$CONFIG" --state-root "$STATE" --confirm-stopped-writers` unlinks the two commands. It keeps the configuration, database, files, logs, backups and the kit's stored copies of past releases. It has no option to purge.
4. Keep or delete your history. [Privacy](privacy.md#delete-what-mike-stored) lists what to delete.

   `mike uninstall --purge` does less than its name suggests. After the steps in 2 it stops the container named `mike-db`, then asks `[y/N]` whether to delete the Docker volume `mike_pg_data`, then asks whether to delete your configuration file. It leaves the state folder alone. Those names belong to the database that `mike db up` starts. A database made by setup or `mike database provision` has other names (`mike-<id>-database` and `mike-<id>-data`), so `--purge` does not remove it: delete that one yourself as [Privacy](privacy.md#delete-what-mike-stored) shows. Answering anything but `y` or `yes` keeps the item.

Also review, separately, the changes you made outside mike: a sidecar certificate you chose to trust, shell lines you added yourself, and a folder you told `mike claude` to trust. Removing mike does not undo those.

If you reinstall the same release later and use the same configuration file, your history is still there. That path was not tested for this alpha either.

## What has and has not been tested

- **Works, on Linux archive installs:** installing from the release archive, a database-backed run that survives a restart, and the kit's backup, verify and restore into a fresh database.
- **Not rehearsed:** upgrade from one release to the next, removal and reinstall, including the Homebrew and mise upgrade and uninstall commands above. This page describes how those commands work, not a rehearsed result. Take a backup first.
- **Not tested on macOS:** the maintenance kit and the restore path.
- **Not covered by any check:** a hand-made backup, and a backup of a Homebrew or mise install.
- **Missing:** a command that purges a database made by setup, and an option to purge in the kit.

Report problems to <https://github.com/temet-ai/mike/issues> with the output of `mike --version`, your operating system and how you installed. Remove credentials, private paths and prompt text first.
