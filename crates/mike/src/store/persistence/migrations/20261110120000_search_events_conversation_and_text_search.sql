-- memory.search_events also records search_session_content (conversation search) and find_text (text search), so the
-- search log covers the two tools sessions use most to find an answer. Only the accepted values of the tool check change.

ALTER TABLE memory.search_events DROP CONSTRAINT search_events_tool_check;

ALTER TABLE memory.search_events ADD CONSTRAINT search_events_tool_check
    CHECK (tool IN ('search_project_knowledge', 'recall_similar_turns', 'search_session_content', 'find_text'));

COMMENT ON TABLE memory.search_events IS 'One row per search_project_knowledge, recall_similar_turns, search_session_content or find_text call: the query (the needle for find_text), the references returned in order, and how the reranker took part. Written off the response path by the handlers; used_references and used_final_at are filled by the search_events worker.';
COMMENT ON COLUMN memory.search_events.returned IS 'JSON array, in the order the caller saw it. search_project_knowledge and recall_similar_turns: {reference, score, rerank_score?}, where score is the plain fused rank. search_session_content: {reference: "session:<uuid>", matches: <n>}. find_text: {reference: "request:<id>"} for each distinct request with a match, at most 50.';
