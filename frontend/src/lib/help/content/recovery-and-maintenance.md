## If no session appears

1. Open the help drawer and check Inspect API, or open [same-origin health](/api/health). A network error is different from an empty capture database.
2. Use the running addresses in [Getting started](/help/guides/getting-started). Do not assume the default ports or reuse an address from another installation.
3. Send a real Claude Code prompt. Check that it reaches the proxy and includes the capture session header described in [Connections and privacy](/help/docs/connections-and-privacy).
4. Check the selected persistence settings and database availability. With durable capture enabled, an unavailable database is a setup failure; do not quietly switch to trial and assume history will persist.
5. Open [Fleet](/fleet) after a project has been captured. An empty project list is normal before first capture; a project-list error should be retried, not treated as an empty database.

## Restart safely

Let active sessions finish before stopping mike; stopping a proxy can interrupt in-flight answers. Use Stop in the terminal control centre for the stack it controls, or Ctrl-C in the terminal running `mike serve`. Then start with the same selected config file. Closing a browser tab does not stop the proxy.

Review `mike print-config` after restarting if environment variables or flags changed. Confirm the browser URL, a fresh capture and the earlier saved session. A successful health check does not establish that durable history survived.

## Optional checkout-free maintenance

New candidate `mike` archives include the optional `mike-maintenance` launcher and adjacent `mike-maintenance.pyz` bundle. Keep these two files together. The helper needs Bash and Python 3.11+; the native `mike` and `mike-mcp` executables do not. No source checkout, Rust or Node is needed for maintenance. The helper does not download code or install prerequisites.

For an owned archive installation, use the helper under the selected prefix; it is not added to PATH. Replace the example prefix if you installed elsewhere:

```sh
PREFIX="$HOME/.local"
MAINTENANCE="$PREFIX/lib/mike-archive/current/mike-maintenance"
"$MAINTENANCE" --help
```

You can instead select the launcher in an explicitly trusted extracted archive. Its `install --candidate "$CANDIDATE" --prefix "$PREFIX"` command needs the original candidate directory containing `manifest.json` and both archive/checksum pairs. Verification executes the selected binaries and helper, so checksums alone do not make unknown downloads trustworthy.

Backup, restore, upgrade checks and removal require an owned archive installation. The kit does not adopt package-manager installations; help and backup verification need no installed prefix or running database. If you have no kit installed, use your database provider's supported backup/restore tools and the installation channel's documented update/removal process. Securely preserve the selected config, complete resolved data and state roots, and matching binaries alongside the whole PostgreSQL database. Copying the config directory alone does not back up capture history.

## Back up before changing versions

The kit accepts separate roots and the shared-root layout where `CONFIG` is exactly `DATA/config.toml` and `STATE` is exactly `DATA/state`, including the default macOS layout. It records each retained file once, excludes temporary lifecycle files, and does not relocate your live files. Other overlapping layouts are refused. Restore still writes a fresh `config.toml`, `data/` and `state/` for offline review; review the restored configuration and resolved runtime paths before starting it. This layout support does not establish native macOS qualification.

Finish active sessions and stop the selected installation and every other client or job writing to its database or files. Leave PostgreSQL running. `--confirm-stopped-writers` confirms that you have done this; it does not stop processes. Before running examples, set each uppercase variable to the reviewed absolute path or identifier it describes. Select your private config as `CONFIG`, resolve its paths with `mike --config "$CONFIG" print-config --paths`, and use the exact data and state paths for `DATA` and `STATE`. Store `BACKUP` at a new private destination outside these roots, with enough room for the database dump and file copies. Save intentional environment/CLI overrides into the selected config and remove them for maintenance.

For an existing database, use a private libpq service file (`SERVICE_FILE`) whose host, port, database, user and authentication settings match the selected TOML. The example service is named `mike_backup`. Database operations require PostgreSQL 19 client tools, or Docker and those tools inside the selected managed container. Tools default to `/usr/bin:/bin`; when needed, add `--tool-directory /absolute/physical/bin` with a reviewed, safely owned directory containing all three PostgreSQL tools (`psql`, `pg_dump`, `pg_restore`) or Docker. Symlinks and unsafe permissions are refused. The helper does not import inherited database credentials or use passwords in command-line arguments.

```sh
"$MAINTENANCE" backup \
  --prefix "$PREFIX" --config "$CONFIG" \
  --data-root "$DATA" --state-root "$STATE" \
  --pg-service-file "$SERVICE_FILE" --pg-service mike_backup \
  --destination "$BACKUP" --confirm-stopped-writers
"$MAINTENANCE" verify --backup "$BACKUP"
```

For a managed local Docker database, replace the two service options with `--docker-identity "$INSTALLATION_ID"`, using the selected installation's exact ID. Repeat any explicit tool-directory selection on restore and upgrade checks. Keep backups private: they contain conversations and credentials. `verify` checks bundle integrity without a database; it does not prove a restore works. Never post a backup, database URL, authentication key or raw captured conversation in a support report.

## Restore into a fresh instance

Keep the original instance and database offline and intact. Install the exact backed-up candidate pair into a fresh user-owned `RECOVERY_PREFIX`; matching version labels alone are insufficient. Have the database owner prepare a fresh, empty PostgreSQL 19 target. Select that target through a private `FRESH_TARGET_SERVICE_FILE` with service `mike_restore`, and choose a nonexistent private `RESTORE_DESTINATION`:

```sh
"$MAINTENANCE" restore \
  --backup "$BACKUP" --prefix "$RECOVERY_PREFIX" \
  --pg-service-file "$FRESH_TARGET_SERVICE_FILE" --pg-service mike_restore \
  --destination "$RESTORE_DESTINATION" --confirm-stopped-writers
```

For Docker, provision a distinct owned installation/volume and use its new ID instead of the service options. Do not reuse the original volume. Restored files form an offline review area, not a running installation. Review the copied config's original database URL and paths before starting; select the fresh target and new data/state roots, and do not overwrite new managed credentials with old ones. If a restore fails after database import, keep the fresh target offline and inspect it before retrying; do not empty it automatically.

Run the matching binary's read-only `database check` with the reviewed new config. Verify earlier session IDs, body contents, settings and external files, then start only the fresh instance, read the saved records through the browser and MCP, and capture a new session. A checksum or migration-ledger pass is not a successful restore rehearsal. Trial history and failed or uncommitted writes cannot be recovered from a durable backup.

## Check and install an update

Keep all writers stopped from backup through installation and startup. Retain the previous candidate pair and backup until the replacement is verified. Select an explicitly trusted newer local `NEXT_CANDIDATE`:

```sh
"$MAINTENANCE" upgrade-check \
  --backup "$BACKUP" --prefix "$PREFIX" --config "$CONFIG" \
  --data-root "$DATA" --state-root "$STATE" \
  --pg-service-file "$SERVICE_FILE" --pg-service mike_backup \
  --candidate "$NEXT_CANDIDATE" --confirm-stopped-writers
```

This checks the backup against the installed predecessor, selected files and current database, plus candidate integrity and a strictly newer version. It does not install or certify migration compatibility. Run the candidate's extracted `mike --config "$CONFIG" database check` next. Only after it passes, install with `"$MAINTENANCE" install --candidate "$NEXT_CANDIDATE" --prefix "$PREFIX"`, then restart the selected instance and verify earlier captures plus a new capture. This is a local update workflow, not an automatic downloader or updater.

The installer switches the complete binary pair, retains prior generations and refuses downgrades or changed same-version replacements. It does not restart services or migrate data. `mike migrate-check` and `mike database check` are read-only compatibility checks; only `mike serve` applies database migrations. Reinstalling an older binary against newer data is not rollback. Use the matching backup and binaries in a fresh instance after a failed upgrade.

Use only a candidate qualified for your platform and the documented release-channel instructions supplied with that candidate. Publication and independent upgrade qualification may still be pending. The kit's availability does not establish recovery qualification or native macOS maintenance support. This help does not turn an unqualified download into a supported release.

## Remove without losing data accidentally

Stop the process you started before removing its binaries. Keep config, state and the database unless you intend to erase them. Removing the executables does not delete PostgreSQL history. Managed services or containers must be stopped through the same control mechanism that created them; do not delete an unrelated database or kill every process named mike.

If you registered MCP, remove the unchanged registration before uninstalling the binaries:

```sh
mike --config "$CONFIG" mcp unregister --claude-config /absolute/private/claude.json
```

Use the same selected config, stable companion path and client JSON file used for registration. Supply `--binary /absolute/stable/mike-mcp` if it was explicitly selected. The daemon may be stopped, but the matching companion must still exist. This command does not launch it or contact a provider. It preserves a private backup and every unrelated client setting, refuses a modified or foreign `mike` entry, and does nothing if the entry is already absent. There is no forced removal option.

For an owned archive installation, stop every instance using the prefix, then run:

```sh
"$MAINTENANCE" uninstall \
  --prefix "$PREFIX" --config "$CONFIG" --state-root "$STATE" \
  --confirm-stopped-writers
```

This validates ownership before unlinking the two active commands. Config, database/container/volume, files, queues, logs, backups and immutable recovery generations are retained; there is no purge option. Modified or foreign commands are refused. Reinstalling the same reviewed candidate recreates the commands. For package-manager installs, use that package manager's removal process instead.

If you enabled optional sidecar CA trust, shell environment exports, MCP registration or folder auto-trust, review those changes separately. Removing mike does not automatically undo trust decisions in other applications.

## Report a problem safely

Give your release contact the candidate version, operating system and architecture, installation method, expected behaviour, exact error, and steps to reproduce with harmless sample data. Note whether the database is durable or trial, and whether you changed ports. Remove credentials, private paths, prompt text and personal information from logs and screenshots. Public repository access is not required to read this guide or investigate locally.
