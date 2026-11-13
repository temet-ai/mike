-- Durable storage shelf for the per-request TransformationManifest
-- (crates/mike/src/compress/manifest.rs): an ordered, versioned record of
-- every original->sent delta (filters, injections, compression,
-- composition), keyed by content SHA256 hashes and byte ranges -- never
-- content bytes -- so it is safe to store and later serve in full.
--
-- Opaque jsonb: the manifest's internal shape (schema_version, sha chain,
-- stages) is owned by crates/mike/src/compress, not the store layer. No
-- normalized stage/target tables -- normalization is only justified once a
-- query the manifest can't answer is demonstrated (see
-- docs/plans/compression-toolset-design.md, "Paired capture and
-- explainability contract"). No index: the manifest is read whole per
-- request, never filtered/sorted on by its contents.
--
-- Nullable, no default: populated only once a later unit wires the live
-- pipeline to build and pass the manifest; every existing and
-- not-yet-migrated row reads back as NULL unchanged.

ALTER TABLE requests.requests
    ADD COLUMN IF NOT EXISTS transformation_manifest jsonb;
