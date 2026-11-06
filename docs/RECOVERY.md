---
title: Backup, restore, upgrade and removal
status: active
created: 2026-10-13
updated: 2026-10-13
summary: Private installed maintenance, explicit tool selection, and recovery qualification requirements.
---

# Backup, restore, upgrade and removal

Status: implementation and disposable-fixture contract for #281. This is not
AC21–AC24 qualification. New native `mike` candidate archives include a
checkout-free optional maintenance kit. Real PostgreSQL, UI/MCP and predecessor
rehearsals remain release blockers.

## Optional installed maintenance kit

`mike` and `mike-mcp` remain self-contained native executables; neither needs
Python, Bash, Rust, Node or a source checkout to run. The separate maintenance
helper requires **Bash and Python 3.11+** with only its standard library. Install
those optional prerequisites through your normal trusted OS/package-manager
procedure if needed. No helper installs dependencies or downloads code.

The `mike` archive contains `mike-maintenance` and `mike-maintenance.pyz` beside
the binary and its license/notices. Keep the two helper files together. Each is
listed with its exact SHA-256, size and mode in the candidate manifest. The
installer preserves them in the same immutable, manifest-addressed generation
as the matching native pair, without adding another command to `PATH`:

```sh
MAINTENANCE="$HOME/.local/lib/mike-archive/current/mike-maintenance"
"$MAINTENANCE" --help
```

For an extracted trusted archive, use `MAINTENANCE="/absolute/path/to/extracted/mike-maintenance"`.
Both forms work from an unrelated working directory with no checkout. The
launcher uses isolated Python and suppresses bytecode writes; all helper modules
come from the adjacent deterministic zipapp, not the working directory,
`PYTHONPATH`, or user site packages. An equivalent explicit invocation is
`python3 -I -B /absolute/path/to/mike-maintenance.pyz --help`.

The extracted kit can also install the complete trusted archive pair without
source tooling:

```sh
"$MAINTENANCE" install --candidate "$CANDIDATE_DIRECTORY" --prefix "$HOME/.local"
```

Select the local directory containing the original `manifest.json` and both
archive/checksum pairs, not just an unpacked binary. This invokes the same
installer as the development `scripts/install-release` entrypoint. It makes a
private snapshot, verifies hashes, native platform and executable versions, then
atomically selects the owned generation. Checksums establish integrity, not
authenticity: use only explicitly trusted candidate files, whose binaries and
helper are executed during verification. There is no download, compilation,
profile edit, service change or automatic migration. Existing foreign commands,
modified installed files, downgrades and changed same-version candidates are
refused; repeating the same verified candidate is safe.

Database operations additionally require PostgreSQL 19 client tools (`psql`,
`pg_dump`, `pg_restore`) for a BYO database, or Docker and those client tools
inside the explicitly selected local managed container. Host commands default
to the fixed `/usr/bin:/bin` tool path. For a trusted installation elsewhere,
pass `--tool-directory /absolute/physical/bin` to each `backup`, `restore`, or
`upgrade-check` command. The selected directory must contain all three PostgreSQL
tools for BYO, or `docker` for managed recovery. It is an explicit executable
selection, not an extra search path; missing tools do not fall back to another
directory. Child `PATH` stays `/usr/bin:/bin`; inherited `PATH`, `PG*`, database
URLs, Python settings and dynamic-library overrides are not imported.

The directory, its ancestors and its executables must be plain paths owned by
the current user or root, with no group/other write permissions. Root-owned
sticky temporary ancestors are allowed, but the selected directory itself must
never be shared-writable. Symlinks, foreign ownership, hard-linked tools,
nonexecutable files and setuid/setgid executables are refused. Use trusted
package-manager installations with safe permissions and stop package updates
during maintenance. The helper never changes their permissions or installs tools.

For example, a Homebrew PostgreSQL installation can be selected by reviewing its
physical Cellar directory, rather than passing the symlink under `opt`:

```sh
PG_TOOLS="$(cd -- "$(brew --prefix postgresql@19)/bin" && pwd -P)"
# Review this physical path and its ownership/permissions first.
"$MAINTENANCE" backup --tool-directory "$PG_TOOLS" ...
```

For Docker Desktop, select its trusted physical binary directory, typically
`/Applications/Docker.app/Contents/Resources/bin`, using the same option. Review
the installed location and permissions; unsafe package-manager directories need
operator remediation before use. This option does not change the database
target, import credentials, select a remote daemon or alter container tool lookup.
The selected Docker context still must resolve to a local Unix socket.

`--help` and backup `verify` need no database,
Docker or server. Operational commands require the selected owned archive
installation and explicit private paths. These optional dependencies and the
helper's own smoke tests do not establish native macOS maintenance support.

For development only, `scripts/release-maintenance` provides the same commands
from a checkout. The examples below use the shipped helper.

## State that must survive

- PostgreSQL contains captures (including request/response bodies), stable IDs,
  runtime settings/feature toggles and the immutable `_sqlx_migrations` ledger.
  Back up the whole database, not selected conversation tables.
- The selected private TOML contains boot configuration and potentially secrets.
  Preserve its credentials. A redacted `print-config` report is not a backup.
  Save any intentional environment/CLI overrides into this selected configuration
  before maintenance, then remove those overrides for the maintenance session.
- Include the complete resolved `data_dir` for filesystem/body state and complete
  installation state directory, including its managed database credential and
  durable queues/identity records. Future external blob stores require their own
  coordinated snapshot; these helpers do not discover arbitrary external stores.
- Process identity, lifecycle/start locks, consumed boot snapshots and the control
  socket are ephemeral. They are excluded. The database provider's operation lock
  is also excluded. No PID file is used to signal a process.
- Preserve the exact predecessor archive pair, manifest/checksums, source SHA and
  license files. A database backup without compatible executables is incomplete
  recovery preparation. The local archive installer keeps immutable generations.

Default Linux paths are selected through the XDG directories. An explicitly
selected non-default configuration uses a sibling `.mike-<installation-id>/`
with `data/`, `state/`, and `logs/`. An explicit `data_dir` overrides the data path;
relative values are relative to the selected TOML's parent. Do not guess the
selected root from the working directory. See [database setup](INSTALLED_DATABASE.md).

Each input/output root must be owned by the current user and private (0700 for
root directories and 0600 for secret-bearing files). Symlink traversals, hard links,
FIFOs and unexpected special files fail closed; the documented ephemeral paths
are the only exceptions. The helper asks the verified installed binary for `print-config --paths` and
refuses data/state roots that differ, rather than duplicating its path resolver.
No system prefix or package-manager installation is adopted. The helper refuses
overlapping config/data/state roots without moving or changing existing inputs.

### macOS layout and fresh installations

The existing macOS default configuration remains
`~/Library/Application Support/mike/config.toml`. Its default data root is that
same `mike` directory and its state root is the nested `state/` directory. Those
roots overlap, so the maintenance helper refuses that legacy layout. Passing the
same default file explicitly with `--config` preserves its existing paths; it does
not opt into another layout or relocate data.

For a **fresh installation with no history to migrate**, select a distinct private
configuration from the first setup, for example:

```sh
CONFIG="$HOME/Library/Application Support/mike-archive-install/config.toml"
mike --config "$CONFIG" print-config --paths
mike --config "$CONFIG" setup --mode trial --accept-trial-loss
```

The first command only reports paths; it does not create the configuration or
move data. The setup command explicitly chooses ephemeral trial mode and starts
the selected server. Select Docker or an existing PostgreSQL database through
the same configuration when durable history is required. The private writer
creates missing managed directories, but refuses an unsafe existing directory.

This distinct config uses its own sibling `.mike-<installation-id>/data`,
`state`, and `logs` directories. Keep using the same absolute `--config` for
setup, serve, wrapper, doctor and MCP commands. Use the exact config/data/state
paths reported by `print-config --paths` for maintenance. Avoid `data_dir`
overrides that contain the config file or state directory.

Do not apply this fresh-install example to an existing installation as a repair.
Changing the config selection changes installation ownership, including managed
database resources; changing `data_dir` alone does not move captured filesystem
state. Existing overlapping installations need a separately reviewed, consistent
state-migration procedure and recoverable backup. No automatic migration is
implemented here. Portable path-policy tests establish neither native macOS
execution nor backup/restore qualification; both remain required before claiming
macOS maintenance support.

## Coordinated backup

1. Finish or cancel active capture clients. Stop the selected Mike installation
   through its owned lifecycle controls. Stop other Mike installations, jobs or
   external writers using the same database/files. Leave PostgreSQL running.
2. Verify the boot config selects durable persistence and the expected database.
   Review the resolved config/data/state paths and select a private backup parent
   outside those roots. These backups contain complete private conversations and
   credentials; protect/encrypt their storage and never attach them to support.
3. Choose exactly one database method:
   - Managed Docker: supply `--docker-identity INSTALLATION_ID`. The helper checks
     the exact Compose project, container/volume ownership labels, volume mount
     and loopback-only publication, pins the container ID and selected local Unix
     socket, then runs `psql`, `pg_dump` and `pg_restore` inside that container.
     It never copies a running PostgreSQL data directory or deletes a volume.
   - Existing PostgreSQL: create a private libpq service file containing an explicit
     host, port, database, user and authentication settings. Supply
     `--pg-service-file PRIVATE_FILE --pg-service mike_backup`. The service target
     and connected database/role must match the selected TOML. Connection strings
     and passwords are never accepted as command-line values by the helper.
     Install compatible PostgreSQL 19 client tools through a trusted source and
     use `--tool-directory` when they are outside `/usr/bin` and `/bin`.
     Inherited `PG*`, `DATABASE_URL` and shell startup files are not used.
4. Run the helper, replacing the example paths with the reviewed private paths:

```sh
"$MAINTENANCE" backup \
  --prefix "$HOME/.local" --config "$CONFIG" \
  --data-root "$DATA" --state-root "$STATE" \
  --pg-service-file "$SERVICE_FILE" --pg-service mike_backup \
  --destination "$BACKUP_PARENT/before-update" --confirm-stopped-writers
"$MAINTENANCE" verify --backup "$BACKUP_PARENT/before-update"
```

For Docker, replace the two service options with `--docker-identity "$ID"`.
Add `--tool-directory "$PG_TOOLS"` (or the reviewed Docker binary directory) when
needed; repeat the selection on restore and upgrade-check invocations.
The explicit confirmation means all writers are stopped; it does not stop them.
The helper holds the same installation start/daemon leases as Mike and, when
present, the managed provider operation lease. It never kills an existing process.
Other installations/external clients remain the operator's responsibility.

Before dumping, the installed binary's read-only `database check` must confirm
that its embedded migration history is compatible with the selected database.
The snapshot includes a custom-format whole-database dump, file-by-file SHA-256
payloads and a private restore index. It compares file inventories and the source
migration ledger before/after dumping. A conservative PostgreSQL change token
also includes WAL position and transaction snapshot, so writes after a backup
cannot pass upgrade preflight merely because the migration ledger stayed equal.
Activity/checkpoints elsewhere on a shared PostgreSQL cluster can invalidate this
token; keep the cluster quiet or repeat the backup. It is not a row-level diff.

The output is owner-only. The public-shaped `manifest.json` and terminal receipt
contain only allowlisted version/time/count/hash fields, with no original paths,
URLs, keys, prompts or filenames. `restore-index.json`, `database.dump`, and every
numbered payload are private. They must not be shared. Checksums establish local
integrity, not origin or authenticity: restore only your own trusted backups.
PostgreSQL archives can contain executable SQL.

The final destination appears only after every required step succeeds. A failed
command exits nonzero, suppresses private child diagnostics and cleans its own
incomplete work on normal failure/termination. A hard kill/power loss can leave a
private `.mike-recovery-incomplete-*` directory; it is not a completed backup. Do
not overwrite an earlier backup or reuse a destination. The previous backup is
never removed. Disk-full errors fail the operation rather than publishing a
successful receipt. Reserve room for both the database dump and all file copies.

## Restore into a fresh instance

1. Keep the damaged/original instance and its database offline and intact.
2. Install the exact backed-up archive pair into a **fresh user-owned prefix**.
   The helper requires its installed manifest hash, not just a version label, to
   equal the backup. Do not force an older executable into the upgraded prefix.
3. Have the database owner create a fresh, empty PostgreSQL 19 target with the
   required extension packages available. For Docker, provision a distinct owned
   installation/volume, keep Mike stopped, and use that new installation ID.
   Do not delete or reuse the original volume. Review target connection details.
4. Restore to a nonexistent private file destination:

```sh
"$MAINTENANCE" restore \
  --backup "$BACKUP_PARENT/before-update" --prefix "$RECOVERY_PREFIX" \
  --pg-service-file "$FRESH_TARGET_SERVICE_FILE" --pg-service mike_restore \
  --destination "$RECOVERY_PARENT/restored" --confirm-stopped-writers
```

The target is checked for existing user tables, views, sequences and foreign
objects. The helper uses `pg_restore --single-transaction --exit-on-error` without
`--clean`, `--create` or role/ACL restoration. A SQL failure rolls back the import;
the tool never drops an existing database. The destination files are staged
privately before import and published only after the restored migration ledger
matches. If import commits but the subsequent ledger/filesystem check fails,
the target may contain the restored data: leave it offline and inspect that fresh
target before retrying. Do not automatically empty it to make a retry pass.

The returned directory is an **offline review area**, not an activated
installation. It contains `config.toml`, `data/`, `state/` and a small
`RESTORED.json` receipt. Review the copied config before any start: its database
URL and absolute paths still refer to the original installation. Select the new
target URL/credentials and data directory. A new config path has a new installation
identity/state root. Retain the backup's old managed credential for original-volume
recovery; never overwrite a freshly provisioned target's different credential or
provider metadata with it. Review portable durable state before placing it in the
new selected state root. Files are restored private; executable bits are not
implicitly re-enabled.

Before starting, run the matching binary's read-only `database check` using the
reviewed new config. Compare captured session/request IDs and exact table counts,
body checksums, settings and every external file hash against the source fixture.
Then start only the new instance, read the same records through both browser and
MCP, and capture a new session. A checksum/ledger pass does not substitute for
these installed-product checks. Record any loss window; trial/in-memory data and
uncommitted/failed writes cannot be recovered from a durable backup.

## Upgrade and downgrade

Keep all writers stopped from backup through installation and startup. Record the
backup's `created_utc`, its actual private location, source version and manifest
identity before any migration. With a trusted prepared next candidate:

```sh
"$MAINTENANCE" upgrade-check \
  --backup "$BACKUP_PARENT/before-update" --prefix "$HOME/.local" \
  --config "$CONFIG" --data-root "$DATA" --state-root "$STATE" \
  --pg-service-file "$SERVICE_FILE" --pg-service mike_backup \
  --candidate "$NEXT_CANDIDATE" --confirm-stopped-writers
```

This verifies that the backup still matches the installed predecessor, selected
configuration, file contents, DB change token and migration ledger. It verifies
the candidate archives and requires a strictly newer SemVer. It does **not**
certify the candidate's migration compatibility or install anything. Run the
candidate's installed/extracted `mike --config "$CONFIG" database check` next;
that read-only path must reject changed/unknown/newer/failed migration history.
Only after it passes, run `"$MAINTENANCE" install --candidate "$NEXT_CANDIDATE" --prefix "$HOME/.local"` and restart the
selected instance. The installer atomically selects a complete pair, permits
retry of the same verified generation, and rejects changed same-version content
and downgrades. It never stops services, applies migrations or modifies data.

For recovery after a failed migration, use the compatible backup **and** matching
binary in a fresh instance. Reinstalling an old binary against newer data is not
rollback. Preserve all historical migration checksums; do not repair mismatches
by rewriting the ledger or edited historical SQL. First-preview AC23 must freeze
two internal artifact versions; the next release must use the actual published
predecessor and actual channel mechanism.

## Removal preserves data

Stop the installation first, and stop any other installation using the same
executable prefix. Unregister an MCP/client entry through its owned registration
command only if it remains unchanged; preserve user-edited/foreign entries and
unrelated client configuration. No shell profile, package-manager entry or system
service is edited by these helpers. Use the relevant package manager for a
package-manager install.

```sh
"$MAINTENANCE" uninstall \
  --prefix "$HOME/.local" --config "$CONFIG" --state-root "$STATE" \
  --confirm-stopped-writers
```

This validates the complete owned installed generation and both exact symlink
targets before unlinking `PREFIX/bin/mike` and `PREFIX/bin/mike-mcp`. Repetition
is safe, including after interruption between the two unlinks. Changed binaries,
foreign links and unexpected generation contents are refused before removal.
Configuration, PostgreSQL/container/volume, body files, queues, logs, backups,
client entries and immutable recovery generations are retained. Reinstalling the
same reviewed candidate recreates the commands; after restarting with the same
selected config, history must still be available. This conservative operation
removes active commands, not the retained recovery executables on disk.

There is deliberately no purge option. Storage reclamation is a separate, explicit
operator decision after a successful restore rehearsal. Review database size,
backup/dump size, retained generations and file roots. Do not silently enable body
compaction, deletion or destructive retention to satisfy a soak test. Existing
retention/compaction commands require their own reviewed scope and recovery plan.
No automatic backup rotation is enabled by this tool.

## Evidence still required

Local synthetic coverage proves ownership/path validation, lease refusal,
byte-exact file restoration, private permissions, safe failure, fresh-target and
matching-binary checks, source-target agreement, Docker resource identity,
stale-backup detection, and retained state on command unlink/reinstall. It does
not prove a live DB or installed user journey.

The separate `scripts/test-release-recovery-db` runner exercises the actual
PostgreSQL backup/restore backend against an explicitly owned disposable PG19
cluster. It requires `MIKE_TEST_ADMIN_URL` and `MIKE_RECOVERY_TEST_PGDATA`;
missing or unsafe fixtures fail rather than silently skip. Optional test-only
client-directory/library overrides support a relocated PostgreSQL installation.
These test-only environment overrides remain separate from the installed
helper's validated `--tool-directory` option; runtime library overrides are
never inherited.

The local PG17.11 run passed 16 backend/bootstrap cases, including exact restored
rows/body bytes/settings/ledger/files, source and decoy isolation, nonempty-target
refusal, frozen-service redirection refusal, transactional rollback, failed dump
cleanup, tamper refusal, concurrent-write detection, and retaining a failed
post-import target offline. This exposed and fixed a missing explicit restore
connection target. These are backend integration results; the complete installed
AC21–AC24 journeys below remain **NOT RUN**.

Before closing #281, record candidate/predecessor SHAs and hashes, PostgreSQL and
client versions, commands/exit codes and independent observations for:

- AC21: disposable DB interruption, upstream 429/500, cancelled stream and an
  actual quota-limited data volume; observable degraded/partial capture, bounded
  retry/resource cleanup, recovery and preservation of earlier durable captures.
- AC22: real coordinated DB+filesystem backup/restore, failed dump/disk-full
  injection, exact IDs/counts/body hashes/settings, and UI/MCP reads after restore.
- AC23: real predecessor/candidate install, migration, interruption/retry, new
  capture and incompatible downgrade refusal before mutation.
- AC24: actual owned removal/reinstall with capture history, unrelated client
  config and a verified private backup retained.

Checkout-free packaging fixtures additionally exercise the actual shipped helper
after deleting its producing source tree, isolated help/validation from unrelated
working directories, installed/extracted synthetic backup verification, recursive
candidate verification inside the zipapp, and refusal of tampered support hashes,
foreign ownership, hard links, path traversal and unsafe partial installations.
The source-deleted helper also installs/repeats a fixture candidate and refuses
foreign prefix commands and corrupted bytes. Core binary smoke runs with an
empty `PATH`, independently of the optional maintenance interpreter.
Hermetic tool-selection fixtures run PostgreSQL-shaped and Docker-shaped local
executables through the source-deleted zipapp, restore synthetic private files,
and check path/ownership/permission rejection and ambient credential isolation.
These tests do not use real credentials, real backups, PostgreSQL or services.

All four complete journeys are **NOT RUN** by the helper tests. Native macOS
maintenance and published-channel upgrade are also **NOT RUN**.
No hosted CI is required to run the disposable Linux rehearsal when that local
environment is available.

PostgreSQL references: [pg_dump snapshot and formats](https://www.postgresql.org/docs/19/app-pgdump.html),
[transactional pg_restore](https://www.postgresql.org/docs/19/app-pgrestore.html),
[transaction snapshot functions](https://www.postgresql.org/docs/19/functions-info.html),
[WAL functions](https://www.postgresql.org/docs/19/functions-admin.html).


### Removing the selected MCP registration

Before removing the binaries, use `mike --config /absolute/config.toml mcp
unregister --claude-config /absolute/claude.json` for the exact selected client.
Pass the same explicit stable `--binary` path if registration used one. The daemon
may be stopped; the companion still needs to exist, but it is not executed.

New registrations include the nonsecret `MIKE_REGISTRATION_INSTANCE_ID` marker in
the entry's environment. Removal requires that marker and the complete unchanged
binary/argument/endpoint entry to match the selected config-derived installation.
Two configs cannot remove each other's registration even if their binary and
endpoint match. Unmarked legacy entries are ambiguous and are left untouched.
Review a legacy entry before explicitly using `mike mcp register --replace` to
migrate it with a private backup. There is no forced unregister option. Unrelated
client settings, project registrations and trust choices remain unchanged.
