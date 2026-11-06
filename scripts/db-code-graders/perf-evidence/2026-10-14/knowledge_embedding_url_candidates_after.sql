SELECT url FROM (
  SELECT unnest(urls_seen) AS url FROM memory.semantic_embedders
  UNION
  SELECT value #>> '{}' FROM config.settings WHERE key = 'embedding.url'
) candidates WHERE url IS NOT NULL
