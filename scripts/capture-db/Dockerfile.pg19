# PostgreSQL 19 with pgvector, for the capture DB and the test DB.
# pgvector publishes no pg19 image yet, so this builds it from the release tag.
# OPTFLAGS="" keeps the build portable instead of tuned to the build host's CPU.
FROM postgres:19rc1-bookworm@sha256:d3d249b3dcc7920ab06f437a49ceb8fb83d483d7863bef31e1bf1d4bdc584cc2

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
