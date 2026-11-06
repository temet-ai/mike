ALTER TABLE projects.projects ADD COLUMN project_uuid uuid NOT NULL DEFAULT gen_random_uuid();
CREATE UNIQUE INDEX projects_project_uuid ON projects.projects(project_uuid);
CREATE TABLE projects.machines (
    machine_uuid uuid PRIMARY KEY,
    name text NOT NULL CHECK (btrim(name) <> '')
);
CREATE TABLE projects.locations (
    location_uuid uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id bigint NOT NULL REFERENCES projects.projects(id),
    machine_uuid uuid NOT NULL REFERENCES projects.machines(machine_uuid),
    root_path text NOT NULL CHECK (btrim(root_path) <> ''),
    last_seen timestamptz NOT NULL DEFAULT now(),
    UNIQUE(machine_uuid, root_path)
);
CREATE INDEX locations_project ON projects.locations(project_id);
ALTER TABLE sessions.sessions ADD COLUMN location_uuid uuid REFERENCES projects.locations(location_uuid);
CREATE INDEX sessions_location ON sessions.sessions(location_uuid) WHERE location_uuid IS NOT NULL;
CREATE TABLE memory.knowledge_sources (
    source_uuid uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    location_uuid uuid NOT NULL REFERENCES projects.locations(location_uuid),
    name text NOT NULL CHECK (btrim(name) <> ''),
    root_path text NOT NULL,
    generation bigint NOT NULL DEFAULT 0,
    last_scan_at timestamptz,
    UNIQUE(location_uuid, name)
);
CREATE TABLE memory.knowledge_documents (
    document_uuid uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    source_uuid uuid NOT NULL REFERENCES memory.knowledge_sources(source_uuid),
    relative_path text NOT NULL,
    current_revision bigint,
    deleted_at timestamptz,
    UNIQUE(source_uuid, relative_path)
);
CREATE TABLE memory.knowledge_revisions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    document_uuid uuid NOT NULL REFERENCES memory.knowledge_documents(document_uuid),
    title text NOT NULL,
    body text NOT NULL,
    content_hash text NOT NULL,
    kind text NOT NULL,
    status text NOT NULL DEFAULT 'unknown',
    metadata jsonb NOT NULL DEFAULT '{}',
    observed_at timestamptz NOT NULL DEFAULT now(),
    search_tsv tsvector GENERATED ALWAYS AS
        (to_tsvector('english', title || ' ' || body)) STORED,
    UNIQUE(document_uuid, id)
);
ALTER TABLE memory.knowledge_documents ADD CONSTRAINT knowledge_current_revision
    FOREIGN KEY(document_uuid, current_revision)
    REFERENCES memory.knowledge_revisions(document_uuid, id) DEFERRABLE INITIALLY DEFERRED;
CREATE INDEX knowledge_revision_search ON memory.knowledge_revisions USING gin(search_tsv);
CREATE INDEX knowledge_documents_source ON memory.knowledge_documents(source_uuid, deleted_at);
COMMENT ON TABLE projects.locations IS 'Machine-qualified checkouts of a logical project; paths do not define portable project identity.';
COMMENT ON TABLE memory.knowledge_sources IS 'Explicitly registered local roots collected out of band. Successful complete scans advance generation; offline roots do not delete documents.';
COMMENT ON TABLE memory.knowledge_revisions IS 'Immutable full source text and metadata. Search results refer to these revisions; generated OKF bundles are not original sources.';
