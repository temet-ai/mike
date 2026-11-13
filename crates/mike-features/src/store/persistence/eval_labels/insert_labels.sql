WITH wanted AS (
    SELECT * FROM jsonb_to_recordset($1::jsonb) AS x(
        axis text, label text, action text, tier text, method text, note text,
        provenance jsonb, supersedes_id bigint)
), cur AS (
    SELECT DISTINCT ON (c.axis) c.id, c.axis
      FROM eval.labels_current_v c
     WHERE c.subject_kind = $2 AND c.subject_id = $3 AND c.tier = $9
     ORDER BY c.axis, c.id DESC
), ins AS (
    INSERT INTO eval.labels AS l
        (subject_kind, subject_id, operator_turn_id, session_id, session_uuid, axis, label,
         action, tier, method, split, note, source, provenance, verdict_snapshot,
         verdict_bundle_id, supersedes_id)
    SELECT $2, $3, $3, $4, $5, w.axis, w.label, w.action, w.tier, w.method, $6, w.note, $7,
           w.provenance, $8, ($8::jsonb ->> 'bundle_id')::bigint, COALESCE(w.supersedes_id, cur.id)
      FROM wanted w LEFT JOIN cur ON cur.axis = w.axis
    RETURNING l.id, l.subject_kind, l.subject_id, l.operator_turn_id, l.session_id, l.session_uuid,
              l.axis, l.label, l.action, l.tier, l.method, l.split, l.tags, l.severity,
              l.confidence, l.evidence, l.note, l.source, l.provenance, l.inclusion_prob,
              l.verdict_snapshot, l.verdict_bundle_id, l.import_batch_id, l.supersedes_id,
              l.created_at
)
SELECT l.id AS "id!",
       l.subject_kind AS "subject_kind!",
       l.subject_id AS "subject_id!",
       l.operator_turn_id AS "operator_turn_id!",
       l.session_id AS "session_id!",
       l.session_uuid AS "session_uuid!",
       l.axis AS "axis!",
       l.label AS "label!",
       l.action AS "action!",
       l.tier AS "tier!",
       l.method AS "method!",
       l.split AS "split!",
       l.tags AS "tags!",
       l.severity AS "severity?",
       l.confidence AS "confidence?",
       l.evidence AS "evidence?",
       l.note AS "note?",
       l.source AS "source!",
       l.provenance AS "provenance!",
       l.inclusion_prob AS "inclusion_prob?",
       l.verdict_snapshot AS "verdict_snapshot?",
       l.verdict_bundle_id AS "verdict_bundle_id?",
       l.import_batch_id AS "import_batch_id?",
       l.supersedes_id AS "supersedes_id?",
       l.created_at AS "created_at!"
  FROM ins l
 ORDER BY l.id
