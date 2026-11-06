INSERT INTO launch.told (run_id, what) SELECT 'fleet-change-record-3-30a0427e', w FROM unnest(ARRAY['push-failed','digest:7']::text[]) AS w ON CONFLICT (run_id, what) DO NOTHING
