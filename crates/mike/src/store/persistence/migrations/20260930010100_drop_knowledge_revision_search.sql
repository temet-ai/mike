-- no-transaction
-- GIN over memory.knowledge_revisions.search_tsv, 338 MB, unusable behind the OR in search.sql; the generated search_tsv column stays for ts_rank_cd.
DROP INDEX CONCURRENTLY IF EXISTS memory.knowledge_revision_search;
