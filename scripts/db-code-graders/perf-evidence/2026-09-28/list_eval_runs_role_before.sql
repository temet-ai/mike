SELECT id, created_at, finished_at, purpose, status, bundle_id, rubric_version, input_builder_version, slices, model, split, tiers, holdout_batch, labels_as_of, label_count, label_ids_hash, repeat_of_run_id, max_usd, cost_usd, items_judged, items_refused, items_failed, report, argv, count(*) OVER () AS total FROM eval.runs
              WHERE (NULL::text IS NULL OR status = NULL)
                AND (NULL::text IS NULL OR split = NULL)
                AND (NULL::text IS NULL OR purpose = NULL)
              ORDER BY created_at DESC, id DESC LIMIT 50 OFFSET 0
