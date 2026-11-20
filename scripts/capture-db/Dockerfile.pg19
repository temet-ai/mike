# PostgreSQL 19 with pgvector, for the capture DB and the test DB.
# pgvector publishes no pg19 image yet, so this builds it from the release tag.
# OPTFLAGS="" keeps the build portable instead of tuned to the build host's CPU.
FROM postgres:19.0-bookworm@sha256:f4859caf96de494e38efa69ed703b08a5bbd9605e152e604f2daec8841c87c41

ARG PGVECTOR_VERSION=0.8.7

RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
        build-essential ca-certificates git "postgresql-server-dev-${PG_MAJOR}"; \
    git clone --branch "v${PGVECTOR_VERSION}" --depth 1 \
        https://github.com/pgvector/pgvector.git /tmp/pgvector; \
    make -C /tmp/pgvector OPTFLAGS=""; \
    make -C /tmp/pgvector install; \
    rm -rf /tmp/pgvector; \
    apt-get purge -y --auto-remove build-essential git "postgresql-server-dev-${PG_MAJOR}"; \
    rm -rf /var/lib/apt/lists/*

CMD ["postgres", "-c", "autovacuum_max_parallel_workers=4", "-c", "shared_preload_libraries=pg_stat_statements"]
