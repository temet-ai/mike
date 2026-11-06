---
title: Project knowledge in the UI and running system
status: active
created: 2026-09-20
updated: 2026-11-06
summary: What the web UI shows of project knowledge today, how running sessions meet it, and the gaps between the backend's evidence and what the UI displays.
---

# Project knowledge in the UI and running system

## Organising principle

The logical project owns knowledge. A machine checkout is one location of that
project, and a session runs at a location. The permanent project UUID is the
identity in the API; names and paths are display labels. `/api/knowledge/search`
and `/api/knowledge/read` accept either the UUID or a project path.

The backend (collector, search, read, locations, lessons) is described in
[PROJECT_KNOWLEDGE.md](PROJECT_KNOWLEDGE.md). The reader-facing description of the
panel is in the user guide's [Projects](user-guide/projects.md) page.

## What the UI shows today

| Surface | What it does |
| --- | --- |
| Project detail page (`/projects/<path>`), **Project knowledge** section | A collapsible `SectionCard` beside the sessions, launches and memory panels. It searches the project through `GET /api/knowledge/search` (first eight results, no pagination controls) as soon as the page opens and again on each submitted query. The header shows semantic embedding coverage (`ready/total`), and the result list notes a semantic fallback when the embedding service did not answer. Each result shows its source kind, title and path or reference. |
| Source drawer | Selecting a result calls `GET /api/knowledge/read` and opens a drawer with the whole retained text as plain text, the declared status, a `historical revision` tag when the cited revision is no longer current, and the content hash. |
| Session overview, delivery section | Lists the recall tools the session called (`recall_project_context`, `recall_similar_turns`, `search_project_knowledge`, `read_project_knowledge`) with call and error counts and a link to the first request. It counts calls from the captured tool history (`api/session_delivery/`); it does not record which references were returned. |
| Project page graph and embedding panels | Show session similarity and project embedding state; they are not knowledge browsers. |

The panel searches current material only: the API's `kind` and `include_historical`
parameters exist, but the panel sends neither. Results state what mike retained, not
what is true: age is a freshness signal, never proof that a decision is obsolete, and
similarity is not confidence.

## Not built yet

The backend supplies the data for most of these; none has a UI.

- **A durable knowledge route.** The panel's selection is not in the URL, so a
  revision cannot be linked or survive a reload. There is no `/projects/<uuid>/knowledge`
  route and no source-kind filter or historical toggle in the UI.
- **Locations.** `GET /api/knowledge/locations` returns every project and its machine
  checkouts, including projects created with `define_project` that have no captured
  traffic. The project index and heading do not list locations or machines, and the
  index is built from captured traffic.
- **Collection and delivery health.** The backend records per-source scan generation,
  last success and last failure, and lesson file receipts and conflicts. No REST route
  summarises a location's source health and nothing in the UI shows it. Last-seen time
  alone cannot say a machine is online.
- **Retrieval and injection events.** Nothing persists which references a session read
  or was given. Indexed, retrieved and injected are different facts: the UI may say a
  document is available, never that a session used it. A future event would need the
  session, request or turn, project, query, selected references and source hashes,
  written on the asynchronous persistence path, never in request forwarding.
- **Note promotion from the UI.** `scripts/mike-knowledge promote` writes a note to a
  repository file on the machine that runs it. A UI action would need a job addressed
  to a specific location with collector acknowledgement; mike cannot write to another
  machine's disk.
- **Provenance graph.** Edges would come from recorded provenance and supersession,
  not from search similarity.

## Sessions and projects at run time

At launch, `mike spawn open` reads `.mike/knowledge.json`, registers the checkout and
sends `location_uuid` with the workspace registration; the proxy also binds a session
whose working directory lies under a registered location of its own machine. The
lesson snapshot and recall injector use that binding, so a Mac checkout and a desktop
checkout of one project receive the same project's lessons. Collection, indexing and
embedding run outside request forwarding. Session briefings name the search and read
tools; `recall_project_context` includes knowledge results when given a `query`.

Recall has three separate uses: project briefing (orientation), explicit search and
read (task evidence), and lesson injection (standing guidance). Mike does not inject
artifact paragraphs automatically.

## Embedding and recall

Search is one hybrid function for the UI, MCP and recall: PostgreSQL text rank and
model-matched semantic rank fused by reciprocal rank (constant 60). The embedding
worker chunks artifacts, lessons and notes in a sweep every 30 seconds, one bounded
batch of sources at a time (16 pending sources per batch), so indexing is eventual and
the panel shows pending coverage while a backlog drains. Source and model fingerprints
stop a changed document or embedding configuration from matching obsolete vectors.
The response reports `retrieval_path` (`lexical` or `hybrid`), the embedding model
and coverage, and the lexical fallback when the provider is unavailable.
