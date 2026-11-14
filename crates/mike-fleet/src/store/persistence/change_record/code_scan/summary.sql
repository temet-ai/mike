SELECT count(*) AS "scans!",
       count(*) FILTER (WHERE sc.skipped IS NULL AND sc.error IS NULL) AS "scanned!",
       count(*) FILTER (WHERE sc.skipped IS NOT NULL) AS "skipped!",
       count(*) FILTER (WHERE sc.error IS NOT NULL) AS "failed!",
       COALESCE(sum(sc.findings_new), 0)::bigint AS "findings_new!",
       COALESCE(sum(sc.findings_fixed), 0)::bigint AS "findings_fixed!",
       COALESCE(percentile_cont(0.5) WITHIN GROUP (ORDER BY sc.duration_us), 0)::bigint AS "median_us!",
       COALESCE(max(sc.duration_us), 0)::bigint AS "max_us!",
       max(sc.scanned_at) AS last_scan_at
  FROM workspace.code_scans sc
 WHERE sc.scanned_at >= $1
