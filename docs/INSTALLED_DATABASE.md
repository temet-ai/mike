---
title: Installed database provider
status: active
created: 2026-10-13
updated: 2026-11-06
summary: Owned Docker provisioning and read-only existing-PostgreSQL prerequisites, with embedded migration ownership and explicit qualification limits.
---

# Installed database provider

`mike database provision` embeds its Docker Compose resource; it does not need a
source checkout, host `psql`, `sqlx`, Rust, or Node. The setup menu uses the same
provider. Trial is an explicit separate choice and never a database failure fallback. `mike db up`
is the shorter route to the same kind of database (container `mike-db`, volume `mike_pg_data`,
the per-user config file); see [`GETTING_STARTED.md`](GETTING_STARTED.md).

## Managed Docker database

```
mike --config /private/path/config.toml database provision --port 6666
mike --config /private/path/config.toml serve
mike --config /private/path/config.toml database check
mike --config /private/path/config.toml database stop
```

The config's parent must be private to its owner. Docker and its Compose plugin
are checked independently. Select a local Docker socket; remote Docker contexts
are rejected because a remote loopback publication is not the local database.
The chosen port is published only on `127.0.0.1`.

Every selected config has a deterministic installation identity. Its database has
an owned Compose project, labeled container and labeled persistent named volume.
An unlabeled or foreign resource with the expected name is rejected. Setup does
not adopt it or delete anything. Repeating setup reuses the same private random
credential and volume. Losing the credential requires restoring it; setup never
rotates a credential behind an existing database. State lives in the selected
installation's state directory, under `database/`; the password file is owner-only.

The default image is `mike/postgres-pgvector:19rc1-0.8.7`, built locally from the
embedded `crates/mike/resources/postgres19.Dockerfile` (`scripts/capture-db/Dockerfile.pg19` mirrors it, checked by the source-routing tests). Capture, disposable tests, development Compose, CI,
and both provisioning commands use the same recipe: PostgreSQL 19rc1 (the
multi-platform base image is pinned by digest) with pgvector 0.8.7. No upstream
pgvector PostgreSQL 19 image is published yet. Provisioning materializes the
Dockerfile beside the private Compose file and builds it before starting.
`PGDATA` is explicit so the owned volume remains the database storage location.

## Release candidate status

The managed database is a release candidate. It runs PostgreSQL 19rc1, not a
final PostgreSQL 19 release. As of 2026-11-05 Docker Hub publishes no
`postgres:19` tag (only `19rc1-bookworm`), and pgvector publishes no PostgreSQL 19
image, so no final base exists to pin. The base stays
`postgres:19rc1-bookworm@sha256:d3d249b3dcc7920ab06f437a49ceb8fb83d483d7863bef31e1bf1d4bdc584cc2`
in both Dockerfiles.

Switching to a final PostgreSQL 19 is one reviewed line change: the `FROM` line of
`crates/mike/resources/postgres19.Dockerfile`, mirrored byte-for-byte in
`scripts/capture-db/Dockerfile.pg19`, with the new digest, the pinned image tag in
`crates/mike/src/setup/docker_resource.rs` (`DEFAULT_IMAGE`) and the source-routing test,
and a rebuild. Changing the image tag does not recreate an existing managed
database (setup refuses an image mismatch), so that switch needs its own reviewed
upgrade of the cluster.

### Measured first managed setup

Measured 2026-11-05 on one development machine with a scratch config, a `mike 0.1.0` binary and
the Dockerfile above. The base layers were already in the
local image store; the build itself ran with `--no-cache`.

| Step | Wall time | Notes |
|---|---|---|
| Cold build of the recipe (`docker build --no-cache`) | 1 min 25 s | Downloads apt packages and the pgvector 0.8.7 git tag; image 636 MB |
| `database provision --port 6670` | 6.1 s | Image already present under the managed tag; reuses it, no rebuild needed |
| `database check` | not timed | Reports PostgreSQL 19rc1 and 312 embedded migrations pending |
| `database stop` | 1.3 s | Keeps the volume and credential |

The provisioned volume held 46 MB of PostgreSQL data after provisioning, before any migrations.
Network needs are the Debian apt mirror and `github.com` during the first build, and the
Docker Hub pull of the pinned base; provisioning itself makes no outbound calls. Disk needs
are about 640 MB for the image plus its build layers, and about 50 MB for a fresh volume before
migrations and capture data.

The Docker state was not fully clean: the managed image tag and the pinned base were already
cached.

An explicit `--image pgvector/pgvector@sha256:...` can select a reviewed upstream
digest, but it must run PostgreSQL 19. Other mutable tags and registries are
refused. Setup refuses to recreate an existing container on a different image;
upgrade its cluster separately before starting this version of mike.

## Existing PostgreSQL

Set `database_url` and `persist_enabled = true` in the selected private config,
then run `database check`. Setup validates the same URL before saving it. No URL
or driver error containing credentials is printed. A password is normally needed
for authenticated servers; socket/trust credentials remain the database owner's
choice. No packages or unrelated database are created.

The current baseline requires exactly PostgreSQL 19, `public` as the role's current schema,
and the `vector`, `pgcrypto`, and `pg_trgm` server extension packages. Extensions
already enabled must live in `public`. Missing trusted extensions can be created
by the migration role. Missing untrusted extensions, including vector, require a
superuser migration role or prior administrator installation in the target database.
The migration role must own the database and application objects and have database
CREATE plus public CREATE/USAGE permissions, or be a superuser.

Validation is authenticated, read-only and bounded to ten seconds. Managed setup
allows a further bounded readiness period. Existing tables without the original
migration ledger, failed rows, unknown newer migrations, changed checksums and
out-of-order histories fail before configuration is saved. Setup never fabricates
history or replays migration SQL. Only `mike serve` applies embedded migrations.
Prerequisites passing is distinct from persistence being ready after migration.

## Development wrappers and fallback

`scripts/local-db/up.sh --config PRIVATE_CONFIG` and the matching `down.sh` invoke
the installed provider. They require an explicit selected config, do not fall back
to native `/tmp` clusters, and never run raw migration files. `down.sh` preserves
data and has no `--destroy` option. Legacy unlabeled local-db resources are not
adopted; preserve them and migrate deliberately using a supported backup workflow.

The old seed resync cursor operation remains only in
`scripts/local-db/baseline-fresh-fallback.sh --confirmed-fresh-fallback`. This
explicit development-only helper requires host `psql` and an explicitly selected
`MIKE_LOCAL_DATABASE_URL`. Use it only after `mike serve` has migrated a fresh
fallback and before its first capture or resync. Primary setup never invokes it.
