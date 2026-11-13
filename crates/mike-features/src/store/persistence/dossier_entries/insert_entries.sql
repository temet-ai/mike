INSERT INTO memory.dossier_entries
        (session_id, operator_turn_id, kind, data, source, source_ref, dedup_key,
         extractor_version, effective_at)
 SELECT x.session_id, x.operator_turn_id, x.kind, x.data, 'capture', x.source_ref, x.dedup_key,
        x.extractor_version, x.effective_at
   FROM jsonb_to_recordset($1::jsonb) AS x(
        session_id bigint, operator_turn_id bigint, kind text, data jsonb,
        source_ref text, dedup_key text, extractor_version text, effective_at timestamptz)
 ON CONFLICT (kind, dedup_key) DO NOTHING
