export interface HelpEntry {
	slug: string;
	term: string;
	short: string;
	long: string;
}

export const HELP_CONTENT: Record<string, HelpEntry> = {
	session: {
		slug: 'session',
		term: 'Session',
		short: 'A run of Claude Code grouped by `x-claude-code-session-id`.',
		long: 'A run of Claude Code (or any Anthropic-API client). Requests are grouped by the `x-claude-code-session-id` header. Sessions are the top-level grouping in the SPA.'
	},
	request: {
		slug: 'request',
		term: 'Request',
		short: 'A single `/v1/messages` POST, captured.',
		long: 'A single `/v1/messages` POST, captured. Each request has a numeric id, full request and response bytes, and metadata (model, status, timing, token usage).'
	},
	project: {
		slug: 'project',
		term: 'Project',
		short: 'The directory the client was operating in when the session started.',
		long: 'The filesystem directory (or other identifier) the client was operating in when it spawned the session. Projects let operators apply pin / TTL overrides at coarser-than-session granularity.'
	},
	'byte-identity-forwarding': {
		slug: 'byte-identity-forwarding',
		term: 'Byte-identity forwarding',
		short: 'No-op forwarding when no rewrite, injection or intercept applies.',
		long: 'Forwarding can change requests and responses when filtering, injections, compositions, proxy tools or intercepts apply. Original incoming request capture is separate from those changes; inspect the record’s fidelity information.'
	},
	'sse-accumulator': {
		slug: 'sse-accumulator',
		term: 'SSE accumulator',
		short: 'Per-request store of streaming response events captured verbatim.',
		long: 'The per-request store of streaming response events. Every event/data pair is captured verbatim alongside the request, preserving byte identity for full-fidelity replay and analysis.'
	},
	'tool-call': {
		slug: 'tool-call',
		term: 'Tool call',
		short: 'An invocation of a Claude tool emitted in the response stream.',
		long: 'An invocation of a Claude tool emitted in the response stream — the assistant naming a tool and supplying arguments. Full arguments and results are captured; nothing is truncated.'
	},
	'inspect-api': {
		slug: 'inspect-api',
		term: 'Inspect API',
		short: 'The read/write REST and WebSocket API for this installation.',
		long: 'The read/write API powers the browser, MCP bridge and operator scripts. The browser uses same-origin `/api/*` and `/api/ws`; Getting started shows this running instance’s addresses.'
	},
	'anthropic-base-url': {
		slug: 'anthropic-base-url',
		term: 'ANTHROPIC_BASE_URL',
		short: 'Env var Claude Code reads to choose its API target.',
		long: 'The environment variable Claude Code reads to choose its API target. Use the running proxy address in Getting started, without adding `/v1`. Capture also requires a valid session header.'
	},

	'request-status': {
		slug: 'request-status',
		term: 'Request status',
		short: 'Lifecycle state — Complete, Streaming, Pending, Error, or Unknown.',
		long: 'The lifecycle state of a captured request. Computed from the upstream interaction (not the client), so a streaming response stays Complete even if the client disconnected mid-stream.'
	},
	'http-method': {
		slug: 'http-method',
		term: 'HTTP method',
		short: 'The HTTP verb on the captured request — almost always POST.',
		long: "The HTTP verb on the captured request. Anthropic's `/v1/messages` endpoint is POST-only, so almost every captured request is `POST /v1/messages`. Every method is forwarded verbatim."
	},
	'http-status': {
		slug: 'http-status',
		term: 'HTTP response status',
		short: 'The numeric HTTP status code Anthropic returned, preserved verbatim.',
		long: "The numeric HTTP status code Anthropic returned. It's preserved verbatim — a 429 stays 429, a 529 stays 529 — because rewriting status would break Claude Code's retry logic."
	},
	'streaming-vs-non-streaming': {
		slug: 'streaming-vs-non-streaming',
		term: 'Streaming vs. non-streaming',
		short: 'Whether the response arrived as SSE chunks or one JSON body.',
		long: 'Whether the response arrived as Server-Sent Events (`stream: true`, the Claude Code default) or as one JSON body. Streaming responses use SSE; non-streaming responses return a single JSON body.'
	},
	'request-parameters': {
		slug: 'request-parameters',
		term: 'Request parameters',
		short: 'Fields a client sends on /v1/messages to control generation.',
		long: 'The fields a client sends on `/v1/messages` to control generation — `max_tokens`, `temperature`, `streaming`, and derived flags like `tool_loop` and `response_status`. The full request body is reachable via the body drawer.'
	},
	'thinking-mode': {
		slug: 'thinking-mode',
		term: 'Thinking mode',
		short: "Anthropic's extended-thinking toggle: off, on, or adaptive.",
		long: "Anthropic's extended-thinking feature: the model emits a hidden reasoning trace before its visible response. Modes: `off` (no thinking), `on` (always), `adaptive` (per-turn decision)."
	},
	'thinking-blocks': {
		slug: 'thinking-blocks',
		term: 'Thinking blocks',
		short: "Content blocks containing the model's reasoning trace and signature.",
		long: 'Content blocks of type `thinking` that appear in the response when extended thinking is on. Each carries a cryptographic signature; the original wire bytes are preserved verbatim so Anthropic can verify them on the next turn.'
	},
	'thinking-block-signature': {
		slug: 'thinking-block-signature',
		term: 'Thinking-block signature',
		short: 'The cryptographic signature on each thinking block — preserved byte-for-byte.',
		long: "Anthropic's cryptographic signature attached to every thinking content block. The signature covers the exact JSON encoding, which is the canonical reason message bodies are never reserialised."
	},
	'tool-result': {
		slug: 'tool-result',
		term: 'Tool result',
		short: 'A user-message content block returning the output of a previous tool call.',
		long: 'A `tool_result` content block in a USER message — the client returning the output of a previous `tool_use` (tool call) the assistant emitted. The full output is captured; nothing is truncated.'
	},
	'tool-loop': {
		slug: 'tool-loop',
		term: 'Tool loop',
		short: 'Request that is part of a tool-use cycle (assistant ↔ tool ↔ assistant).',
		long: "A request whose latest user message contains a `tool_result` — the assistant is responding to a tool execution rather than generating fresh output. Useful as a filter: 'show me only the requests where the assistant is in a tool loop'."
	},
	'stop-reason': {
		slug: 'stop-reason',
		term: 'Stop reason',
		short: 'Why the model stopped — end_turn, max_tokens, stop_sequence, or tool_use.',
		long: 'The Anthropic-supplied reason the model finished generating. `end_turn` is the default; `max_tokens` means truncated mid-thought; `tool_use` means the model is waiting for a tool result.'
	},
	'upstream-request-id': {
		slug: 'upstream-request-id',
		term: 'upstream_request_id',
		short: "Anthropic's `request-id` (req_xxx). Quote this in support tickets.",
		long: "Anthropic's `request-id` response header (`req_xxx`). Distinct from the captured numeric `request.id` — quote this id in support tickets because Anthropic's logs are keyed on it."
	},
	'upstream-message-id': {
		slug: 'upstream-message-id',
		term: 'upstream_message_id',
		short: "Anthropic's message_start.id (msg_xxx). Streaming-only.",
		long: "Anthropic's `message_start.id` (`msg_xxx`). Identifies the assistant message inside Anthropic's system. Streaming-only — non-streaming responses do not emit `message_start`."
	},
	'model-id': {
		slug: 'model-id',
		term: 'Model id',
		short: "Anthropic's opaque model identifier (claude-{family}-{tail}).",
		long: "Anthropic's opaque model identifier — the canonical form is `claude-{family}-{tail}` (e.g. `claude-opus-4-7`)."
	},
	subagent: {
		slug: 'subagent',
		term: 'Subagent',
		short: 'A Claude Code instance spawned by the parent agent via the Agent tool.',
		long: "A Claude Code instance spawned by the parent agent via the Agent tool. The subagent has its own system prompt and task scope but shares the parent's session id; the persona (e.g. `frontend-developer`) is extracted and surfaced as a chip."
	},

	composition: {
		slug: 'composition',
		term: 'Composition',
		short: 'A captured system-prompt structure, identified by its prompt_sha256.',
		long: 'A captured system-prompt composition — the deduplicated structure of system blocks observed across requests. Identified by `prompt_sha256` and tracked with a mode (observed / file-backed / dynamic).'
	},
	'prompt-sha256': {
		slug: 'prompt-sha256',
		term: 'prompt_sha256',
		short: 'The deduplicated identity of a system-prompt composition.',
		long: "SHA-256 of the canonical structural bytes of a composition's block list, with volatile lines normalised. Same value means same shape; different value means a structural difference, not just wording drift."
	},
	'prompt-registration': {
		slug: 'prompt-registration',
		term: 'Registration',
		short: 'Optional bookkeeping — an unregistered prompt still passes through untouched.',
		long: 'Registering a prompt class ties it to an operator-chosen label and agent identity so you can track drift and (in observe mode) confirm what is actually being seen. It is optional bookkeeping, not a gate — traffic from an unregistered class still passes through unchanged.'
	},
	'block-hash': {
		slug: 'block-hash',
		term: 'Block hash',
		short: 'SHA-256 of one canonical system-prompt block.',
		long: 'SHA-256 of the canonical bytes of a single system-prompt block. Used to deduplicate identical block bodies across compositions and drive cross-composition lookups.'
	},
	'composition-mode': {
		slug: 'composition-mode',
		term: 'Composition mode',
		short: 'observed / file-backed / dynamic — how this composition is treated.',
		long: 'Per-composition state describing whether outbound system blocks are just being observed (`observed`) or actively rewritten via a TOML template (`file-backed`) or a dynamic recipe (`dynamic`).'
	},
	'mode-observed': {
		slug: 'mode-observed',
		term: 'observed',
		short: 'Captured but not rewritten — client bytes pass through unchanged.',
		long: "This composition's shape has been captured but its outbound system blocks are not being rewritten. The client's bytes pass through unchanged. Default for every newly seen composition."
	},
	'mode-file-backed': {
		slug: 'mode-file-backed',
		term: 'file-backed',
		short: 'Rewrites the outbound system block from a TOML template.',
		long: 'The outbound system block is rewritten using a TOML-backed template the operator authored. The captured shape sourced the template; the template is authoritative once promoted.'
	},
	'mode-dynamic': {
		slug: 'mode-dynamic',
		term: 'dynamic',
		short: 'Rewrites the outbound system block via a dynamic recipe.',
		long: 'The outbound system block is rewritten using a dynamic recipe — typically the captured baseline plus filter-rule deltas. Useful when shape should be normalised but content stays operator-controlled.'
	},
	anomaly: {
		slug: 'anomaly',
		term: 'Anomaly',
		short: 'A captured system prompt that could not be parsed into a class at all.',
		long: "A request whose `system` field is missing, malformed, or not a clean 3-block shape — the composer can't extract a (lane, persona) key from it, so it can't appear in the prompt catalogue. Distinct from an unregistered class (a clean shape with no composition yet, which IS in the catalogue): this is a pipeline problem, not a naming gap."
	},
	'drift-queue': {
		slug: 'drift-queue',
		term: 'Drift queue',
		short: 'Captured prompts resembling this composition but differing in detail.',
		long: 'Captured system prompts that resemble this composition but differ in detail — candidates for promotion or rule change. Surfaces structural diffs against the canonical shape.'
	},
	'composition-match': {
		slug: 'composition-match',
		term: 'Composition match',
		short: 'Per-block flag — matched / unknown / extra against the registered composition.',
		long: "A per-block flag describing how each captured system block compares to its registered composition. `matched` = bytes identical; `unknown` = drift in expected position; `extra` = block isn't in the composition."
	},
	'match-matched': {
		slug: 'match-matched',
		term: 'matched',
		short: 'Block bytes are identical to the composition (modulo volatile lines).',
		long: "Block bytes are identical to the composition's expected bytes (modulo volatile lines). The happy path."
	},
	'match-unknown': {
		slug: 'match-unknown',
		term: 'unknown',
		short: "Block sits in an expected position but its content doesn't match — drift.",
		long: "Block is in a position the composition expects, but its content doesn't match — drift. The drift queue picks these up."
	},
	'match-extra': {
		slug: 'match-extra',
		term: 'extra',
		short: 'Block exists in the captured request but not in the composition.',
		long: 'Block exists in the captured request but not in the composition. Either the composition needs extending or the block belongs in a filter rule.'
	},
	'system-block': {
		slug: 'system-block',
		term: 'System block',
		short: 'One element of the `system` array on a /v1/messages request.',
		long: 'One element of the `system` array on a `/v1/messages` request. Claude Code typically sends a 3-block array: model preamble, cache marker, operator instructions. The composition machinery hashes these structurally.'
	},
	'system-reminder': {
		slug: 'system-reminder',
		term: 'system-reminder',
		short: "Claude Code's wire convention for injecting operator-supplied reminders.",
		long: 'A wire convention Claude Code uses to inject operator-supplied reminders before the user message. Renders as `<system-reminder>…</system-reminder>` text — surfaces CLAUDE.md, agent definitions, and hook output to the model.'
	},
	'anomaly-sub-pattern': {
		slug: 'anomaly-sub-pattern',
		term: 'Anomaly sub-pattern',
		short: "A more specific classification within an anomaly's top-level kind.",
		long: "A more specific classification within an anomaly's kind (e.g. `unregistered-subagent` under `unknown-class`). Each sub-pattern carries a recommended-action hint that drives the highlighted CTA on the row."
	},

	'weasel-pattern': {
		slug: 'weasel-pattern',
		term: 'Weasel pattern',
		short: 'Diagnostic findings on rhetorical patterns in response bodies.',
		long: 'Behavioural patterns detected in captured response bodies — diagnostic findings, not blocks. Findings link back to the request that produced them; nothing is ever blocked based on them.'
	},
	'weasel-categories': {
		slug: 'weasel-categories',
		term: 'Weasel-pattern categories',
		short: 'Five categories: shortcuts, blame, work, understanding, overselling.',
		long: 'Five analyser categories: justifying shortcuts, deflecting blame, avoiding work, faking understanding, and overselling. Each is a separate pattern set; a request can fire findings in multiple categories.'
	},
	'cat-justifying-shortcuts': {
		slug: 'cat-justifying-shortcuts',
		term: 'Justifying shortcuts',
		short: 'Language framing scope cuts as a virtue ("pragmatically", "for now").',
		long: 'Language that frames cutting scope as a virtue — "pragmatically", "for now", "lightweight version". The advice: pragmatism is not a licence to reduce scope; "later" rarely arrives.'
	},
	'cat-deflecting-blame': {
		slug: 'cat-deflecting-blame',
		term: 'Deflecting blame',
		short: 'Language suggesting a problem isn\'t the assistant\'s ("pre-existing").',
		long: 'Language suggesting a problem isn\'t the assistant\'s — "pre-existing", "it appears", "as expected". The advice: cite the commit, link the issue, or own the change.'
	},
	'cat-avoiding-work': {
		slug: 'cat-avoiding-work',
		term: 'Avoiding work',
		short: 'Language that punts effort ("trivial", "out of scope", "revisit later").',
		long: 'Language that punts effort — "straightforward", "trivial", "revisit later", "out of scope". The advice: do it now or state explicitly that you didn\'t.'
	},
	'cat-faking-understanding': {
		slug: 'cat-faking-understanding',
		term: 'Faking understanding',
		short: 'Claims of pattern alignment without evidence ("following convention").',
		long: 'Language claiming alignment with code patterns without evidence — "consistent with existing pattern", "following convention". The advice: cite file:line, show 3+ examples, or admit it\'s new.'
	},
	'cat-overselling': {
		slug: 'cat-overselling',
		term: 'Overselling',
		short: 'Language overstating quality ("robust", "production-ready", "clean").',
		long: 'Language that overstates quality — "robust", "comprehensive", "production-ready", "clean code". The advice: enumerate error cases, list what\'s covered, name the specific quality attribute.'
	},

	grader: {
		slug: 'grader',
		term: 'Turn judge',
		short: "Grades each closed operator turn on the operator's reaction and the delivery.",
		long: "Grades each closed operator turn on two axes, reading the prompt head, the assistant's final text, tool counts and the operator's next genuine message. Unlike a steerer, which nudges a live session as it runs, the judge only grades turns after the fact; it never intervenes."
	},
	analyzer: {
		slug: 'analyzer',
		term: 'Analyzer',
		short: 'Scans captured assistant output for a pattern and records findings.',
		long: 'A check that scans the captured assistant output for a particular pattern, such as weasel phrasing, and records any matches as findings. It can run after a turn finishes or in-flight as the response streams through.'
	},
	finding: {
		slug: 'finding',
		term: 'Finding',
		short: 'One result an analyzer recorded for a request.',
		long: 'One result an analyzer recorded against a request — the matched span of text together with its category and supporting metadata. Findings are diagnostic records; they link back to the request that produced them.'
	},

	'filter-rule': {
		slug: 'filter-rule',
		term: 'Filter rule',
		short: 'Operator-configured rewrite of the outbound `system` field.',
		long: 'A user-configured rewrite applied to the outbound `system` field before requests reach Anthropic. Each rule has a kind (`text_substring` / `xml_tag`), value, and action (drop / replace / append).'
	},
	'filter-rule-kind': {
		slug: 'filter-rule-kind',
		term: 'Filter rule kind',
		short: "How a rule's value is matched — text_substring or xml_tag.",
		long: "How a filter rule's `value` is matched against the captured system block. `text_substring` is a literal case-sensitive match; `xml_tag` matches a `<tag>…</tag>` element by name."
	},
	'kind-text-substring': {
		slug: 'kind-text-substring',
		term: 'text_substring',
		short: 'Literal case-sensitive substring match anywhere in a block.',
		long: 'Match on a literal substring anywhere in the block text. The match is case-sensitive and does not interpret regex metacharacters. Use this for short, distinctive operator-supplied phrases.'
	},
	'kind-xml-tag': {
		slug: 'kind-xml-tag',
		term: 'xml_tag',
		short: 'Match a <tag>…</tag> element by tag name.',
		long: 'Match a `<tag>…</tag>` element by tag name (no angle brackets in the value). Convenient for the `<system-reminder>` and similar wrapping conventions Claude Code emits.'
	},
	'filter-rule-action': {
		slug: 'filter-rule-action',
		term: 'Filter rule action',
		short: 'What the rule does on match — drop, replace, or append.',
		long: "What the rule does when its match fires. `drop` removes the matched span; `replace` substitutes the rule's `text` payload; `append` inserts the payload after the matched block."
	},
	'action-drop': {
		slug: 'action-drop',
		term: 'drop',
		short: 'Remove the matched block / span from the outbound system field.',
		long: 'Remove the matched block (or matched span, for substring rules) from the outbound system field. The Anthropic-bound bytes do not include it.'
	},
	'action-replace': {
		slug: 'action-replace',
		term: 'replace',
		short: "Replace the matched span with the rule's text payload.",
		long: "Replace the matched span with the rule's `text` payload. Useful for redacting operator-secret strings without dropping the surrounding context."
	},
	'action-append': {
		slug: 'action-append',
		term: 'append',
		short: "Insert the rule's text payload after the matched block.",
		long: "Insert the rule's `text` payload after the matched block. Useful for injecting an operator-controlled instruction in front of a known marker."
	},
	'prompt-assistant': {
		slug: 'prompt-assistant',
		term: 'Prompt assistant',
		short: 'A Claude Agent SDK chat that proposes filter-rule rewords in the workbench.',
		long: 'The chat under "Your edits" in the filter workbench (`/settings/assistants` configures it). It reads the prompt you are looking at, your filter edits, and — if turned on — the project\'s CLAUDE.md, other captured versions, and project memory, then answers with a before/after proposal you preview and apply yourself unless "What it can change" is set to apply automatically. It runs only when you send it a message.'
	},

	steering: {
		slug: 'steering',
		term: 'Steering',
		short: 'Pointing a live session back at its task when it drifts.',
		long: 'Pointing a live Claude Code session back at its task when it drifts, via an autonomous correction agent (a steerer) that watches the session and sends a corrective message on your behalf. The session keeps running; the steerer rides alongside it.'
	},
	steerer: {
		slug: 'steerer',
		term: 'Steerer',
		short: 'The autonomous agent attached to sessions to watch for drift.',
		long: 'The autonomous agent attached to sessions. A steerer has a thread-anchor prompt that defines what "on track" means, a model that runs drift detection, and thresholds controlling how sensitive and forceful it is. One steerer can watch many sessions; one session can have many steerers.'
	},
	intervention: {
		slug: 'intervention',
		term: 'Intervention',
		short: 'The steering payload injected into a steered turn.',
		long: 'The steering payload injected into a steered turn — the corrective message a steerer sends when it fires. Each intervention carries an authority level and a mode that together determine how forcefully it lands on the session.'
	},
	'steering-authority': {
		slug: 'steering-authority',
		term: 'Steering authority',
		short: 'supply_context / redirect / command — escalating power over the session.',
		long: 'How forceful a steering intervention is, across three escalating levels: supply_context (background info the session may use or ignore), redirect (a suggestion to change direction), and command (a binding directive treated as an operator instruction). Start with supply_context and escalate only for trusted steerers.'
	},
	'steering-mode': {
		slug: 'steering-mode',
		term: 'Steering mode',
		short: 'augment vs drive — how much the steerer takes over the session.',
		long: "How much the steerer takes over the session when it fires. augment adds the steering message alongside the session's context so its Claude still generates the next turn; drive replaces the assistant turn entirely with the steerer's message. augment is the day-to-day default; drive is the most invasive option, reserved for breaking a session out of a loop."
	},

	ttl: {
		slug: 'ttl',
		term: 'TTL (time-to-live)',
		short: 'How long captured sessions are retained before eviction.',
		long: 'How long captured sessions are retained before automatic eviction. Configurable globally, per-project, and per-session. Pinned sessions are exempt from TTL eviction entirely.'
	},
	'project-pin': {
		slug: 'project-pin',
		term: 'Project pin',
		short: 'Marks every session under a project exempt from TTL eviction.',
		long: 'Pinning a project marks every session under it as exempt from TTL-based eviction. Sessions stay until the operator explicitly unpins or deletes them.'
	},

	'usage-tokens': {
		slug: 'usage-tokens',
		term: 'Usage tokens',
		short: 'Anthropic-billed token counts: input, output, and cache subsets.',
		long: "The token counts Anthropic returns in the response's `usage` object. Cache fields (`cache_read_input_tokens`, `cache_creation_input_tokens`) are SUBSETS of input — adding all four would double-count."
	},
	'prompt-cache': {
		slug: 'prompt-cache',
		term: 'Prompt cache',
		short: "Anthropic's prefix-caching feature — cache reads cost 10× less than input.",
		long: "Anthropic's prompt-caching feature. Reported as `cache_read_input_tokens` (cheap, 1/10 input rate) and `cache_creation_input_tokens` (slightly more expensive than uncached, but pays off on the next hit)."
	},
	'cache-control': {
		slug: 'cache-control',
		term: 'cache_control',
		short: 'Request-side opt-in marker that tells Anthropic to cache the prefix.',
		long: "A request-side opt-in marker (`cache_control: { type: 'ephemeral' }`) the client adds to a content block to ask Anthropic to cache the prefix up to and including that block. Without it `cache_read_input_tokens` stays zero."
	},
	'cache-efficiency': {
		slug: 'cache-efficiency',
		term: 'Cache efficiency',
		short: 'Share of input-side tokens served from cache. Higher is cheaper.',
		long: 'The share of input-side tokens served from prompt cache, as a percentage. Computed as `cache_read / (input + cache_read + cache_create)`. Rendered as a ring chart on the Analysis tab.'
	},
	'request-cost': {
		slug: 'request-cost',
		term: 'Request cost',
		short: 'USD figure computed locally from token counts × Anthropic published rates.',
		long: 'A USD figure computed locally from the captured token counts and published per-million-token rates for the request’s model. Nothing here bills you — Anthropic does. Prices live in `frontend/src/lib/modelPricing.ts`; a model it does not list is priced at a default and marked as an estimate.'
	},
	'token-estimate': {
		slug: 'token-estimate',
		term: 'Token estimate',
		short: 'Frontend approximation: ~Math.round(charCount / 4). Always prefixed with ~.',
		long: "A frontend-only approximation of token count, computed as `Math.round(charCount / 4)`. Always rendered with a leading `~` to signal it's an estimate. Anthropic's exact `usage` numbers supersede it once the response lands."
	},

	'conversation-message': {
		slug: 'conversation-message',
		term: 'Conversation message (turn)',
		short: 'One message in the `messages` array — alternates user / assistant.',
		long: 'One message in the `messages` array of a `/v1/messages` request. Anthropic alternates between `user` and `assistant` roles; tool calls and results appear as content blocks inside those turns, not as their own messages.'
	},
	'tool-result-message': {
		slug: 'tool-result-message',
		term: 'Tool-result message',
		short: 'A user-role API message carrying tool_result blocks — the harness, not the operator.',
		long: 'Anthropic\'s `/v1/messages` API requires tool outputs to be sent as `tool_result` blocks inside a `user`-role message. These are NOT operator-typed input — Claude Code (the harness) is replying to the model on the operator\'s behalf. These are labeled "← Read", "← Bash ×3", etc. so the tool name is visible at a glance instead of a generic "USER MESSAGE" badge that hides who is actually talking.'
	},
	'harness-message': {
		slug: 'harness-message',
		term: 'Harness message',
		short:
			'A user-role API message carrying only `<system-reminder>` blocks injected by Claude Code.',
		long: 'Claude Code injects `<system-reminder>`-wrapped text blocks into user-role messages to remind the model of context (open files, recent edits, hooks, MCP server instructions). When a user-role message contains ONLY reminders — no operator text, no tool result — it is labeled "HARNESS" so the operator knows nothing they typed flows through this turn.'
	},

	'anthropic-headers': {
		slug: 'anthropic-headers',
		term: 'Anthropic request/response headers',
		short: 'Header conventions Anthropic emits — auth, version, diagnostics, rate limits.',
		long: 'Every header is preserved verbatim. Auth headers (`x-api-key`, `authorization`, `cookie`) are MASKED in the SPA. `anthropic-version` pins the API version; `anthropic-beta` carries beta-feature flags.'
	},
	'rate-limit-headers': {
		slug: 'rate-limit-headers',
		term: 'Rate-limit headers',
		short: 'Twelve `anthropic-ratelimit-*` headers across four families on every response.',
		long: 'Anthropic emits four rate-limit families (`requests`, `input-tokens`, `output-tokens`, `tokens`), each with `-limit`, `-remaining`, and `-reset` members. Twelve headers total. All twelve are preserved.'
	},
	'retry-after': {
		slug: 'retry-after',
		term: 'retry-after',
		short: 'Seconds to wait before retrying — emitted alongside 429 / 529 responses.',
		long: 'A response header Anthropic emits alongside 429 (rate-limited) and 529 (overloaded) statuses. Tells the client how many seconds to wait before retrying. The value is preserved verbatim.'
	},

	'volatile-lines': {
		slug: 'volatile-lines',
		term: 'Volatile lines',
		short: 'Patterns normalised before computing the prompt class hash.',
		long: 'Substring patterns normalised before computing the prompt-class hash. Normalising volatile lines (timestamps, request ids, etc.) prevents superficial drift from spawning new compositions.'
	},
	'agent-registry': {
		slug: 'agent-registry',
		term: 'Agent registry',
		short: 'Registered Claude agents observed in captured sessions.',
		long: 'The catalogue of named Claude sub-agents observed in captured request traffic. Each entry tracks the source (observed / file-backed / dynamic) plus the prompt versions captured for it.'
	},
	'agent-version': {
		slug: 'agent-version',
		term: 'Agent version',
		short: "A captured snapshot of an agent's system prompt at a point in time.",
		long: "A captured snapshot of an agent's system prompt at a point in time. A new version is recorded only when the prompt structure changes — volatile-line normalisation prevents trivial timestamp changes from minting a new version."
	},
	'mode-builtin': {
		slug: 'mode-builtin',
		term: 'builtin',
		short: 'Ships with the harness itself; no on-disk .md definition.',
		long: "An agent identity that ships with the Claude Code harness itself — the fork worker, general-purpose, Explore, Plan, and similar built-in tools. There's no on-disk .md definition to point at; the identity ships inside Claude Code."
	},
	'agent-dispatches': {
		slug: 'agent-dispatches',
		term: 'Dispatches',
		short: 'Captured Agent-tool invocations of this agent, each with the full prompt.',
		long: 'Every captured Agent-tool invocation of this agent — one row per time a parent session spawned it, each carrying the full dispatch prompt, the dispatching session, and the request it landed on.'
	},
	'agent-prompt-versions': {
		slug: 'agent-prompt-versions',
		term: 'Prompt versions (section)',
		short: 'Distinct captured system-prompt shas observed for this agent.',
		long: 'The distinct system-prompt shas captured for this agent across real traffic — one row per content_sha256 actually observed, linking through to the full prompt.'
	},
	'agent-instances': {
		slug: 'agent-instances',
		term: 'Instances',
		short: 'Bound subagent sessions that ran as this agent, one per dispatch.',
		long: 'The subagent sessions bound to this agent identity — one row per Claude Code sub-instance that ran as this agent, with its session, persona, and first request.'
	},

	bigbro: {
		slug: 'bigbro',
		term: 'Bigbro',
		short: 'The role a steerer plays: an honestly-attributed steered turn, not a fake operator.',
		long: 'The role a steerer plays when it fires — it sends a first-class user turn into the session, honestly attributed (`turn_origin = steered`) rather than disguised as the operator. The BIGBRO ATTRIBUTION card names the steerer, authority, mode, verdict, and anchor in full.'
	},
	'refocus-steerer': {
		slug: 'refocus-steerer',
		term: 'Refocus steerer',
		short: 'Steerer type that re-grounds a session drifting off its anchored task.',
		long: 'A steerer type that re-grounds a drifting session. Its LLM reads recent turns against the thread anchor and, when the work has wandered off topic, fires a corrective turn pointing the session back at its task.'
	},
	'verifier-steerer': {
		slug: 'verifier-steerer',
		term: 'VerifierSteerer',
		short: 'LLM judge that cross-checks completion claims against actual response + tool calls.',
		long: 'A steerer type that judges completion claims. A keyword/Overselling gate fires, then an LLM judge cross-checks the claim against the assistant\'s response text and tool calls; if evidence is missing it injects a plain-prose nudge naming what\'s absent, e.g. "You claimed the tests pass, but no test command ran this turn."'
	},
	'steering-enrolment': {
		slug: 'steering-enrolment',
		term: 'Enrolment',
		short: 'The link between a session and a steerer — one enrolment, one steerer watching.',
		long: 'The link between a session and a steerer. A session can enrol multiple steerers (run in priority order); a steerer can be enrolled in multiple sessions. Disenrolling removes the link at the next turn evaluation. Listed in the Steering activity section on the session page.'
	},
	'outcome-corrected': {
		slug: 'outcome-corrected',
		term: 'corrected',
		short: 'A later user turn corrected the agent — stamped by the correction classifier.',
		long: 'A later user turn corrected the agent. Stamped by the correction classifier; flags turns where the operator (or a steer) had to step in to redirect the work.'
	},

	lesson: {
		slug: 'lesson',
		term: 'Lesson',
		short: 'A captured unit of learning, consolidated RAW → DAILY → LONG_TERM.',
		long: 'A captured unit of learning written by an agent. A background worker embeds, dedups, and promotes it RAW → DAILY → LONG_TERM. DAILY and LONG_TERM lessons are injected at session start so new sessions inherit what earlier ones learned.'
	},
	'memory-stages': {
		slug: 'memory-stages',
		term: 'Memory stages (RAW → DAILY → LONG_TERM)',
		short: 'Three consolidation tiers, like human memory. Only DAILY/LONG_TERM inject.',
		long: 'Lessons consolidate across three stages: RAW (fresh), DAILY (promoted after embedding + dedup), LONG_TERM (survived nightly consolidation and trimmed to the per-project cap). Only DAILY and LONG_TERM are injectable at session start.'
	},
	'stage-raw': {
		slug: 'stage-raw',
		term: 'RAW',
		short: 'A freshly written lesson, not yet consolidated.',
		long: 'A freshly written lesson, not yet consolidated. Picked up by the session-consolidation pass once it has been idle long enough.'
	},
	'stage-daily': {
		slug: 'stage-daily',
		term: 'DAILY',
		short: 'A RAW lesson promoted after embedding + cosine-similarity dedup. Injectable.',
		long: 'A RAW lesson promoted after embedding and cosine-similarity dedup against existing lessons. Injectable at session start.'
	},
	'stage-long-term': {
		slug: 'stage-long-term',
		term: 'LONG_TERM',
		short: 'A DAILY lesson that survived nightly consolidation. The most durable tier.',
		long: 'A DAILY lesson that survived the nightly consolidation pass and was kept within the per-project cap. The most durable tier; injectable at session start.'
	},
	'turn-outcome': {
		slug: 'turn-outcome',
		term: 'Turn outcome',
		short: 'How a judge model graded one operator ask: did it get what it wanted?',
		long: "How a judge model graded one operator turn. A cheap model reads the prompt, the final response, and the turn's tool-use counts, then assigns one of accepted / shipped / corrected / abandoned / unknown. It reads the transcript after the fact, so treat it as a fallible second opinion, not a measurement."
	},
	'outcome-accepted': {
		slug: 'outcome-accepted',
		term: 'accepted',
		short: 'Real work happened and nothing signals it was rejected. The positive default.',
		long: 'The positive default. The assistant satisfied the request with real work — code written, files changed, a question answered substantively — with no ship action and no sign of rejection, correction, or abandonment.'
	},
	'outcome-shipped': {
		slug: 'outcome-shipped',
		term: 'shipped',
		short: 'Accepted, plus a delivery the assistant confirmed: commit, push, PR, deploy.',
		long: 'Like accepted, and the turn additionally shows a concrete delivery the assistant explicitly confirmed happened: a git commit, a pushed branch, a PR created or merged, a deploy, or a published release.'
	},
	'outcome-abandoned': {
		slug: 'outcome-abandoned',
		term: 'abandoned',
		short: 'Cut short or produced no real work — gave up, refused, or never acted.',
		long: 'The turn was interrupted or produced no real work: the operator gave up mid-task, or the assistant produced only a refusal or an empty plan with no follow-through.'
	},
	'outcome-unknown': {
		slug: 'outcome-unknown',
		term: 'unknown',
		short: 'Not enough signal to decide — also how a turn reads before it is graded.',
		long: 'Genuinely insufficient signal for the judge to decide. Also the state a turn sits in before the judge has read it, so a fresh session tallies its turns here until the judge worker catches up.'
	},
	'operator-turn': {
		slug: 'operator-turn',
		term: 'Operator turn',
		short: 'One row per Claude Code prompt, spanning all work until the next prompt.',
		long: 'One row per Claude Code user prompt, spanning every request, tool call, result, and dispatch until the next operator input. OPEN while trailing a live session, frozen once the next prompt arrives. The unit semantic recall embeds and retrieves over.'
	},
	'semantic-recall': {
		slug: 'semantic-recall',
		term: 'Semantic recall',
		short: 'Retrieval of past turns and lessons by meaning (cosine ANN), FTS when down.',
		long: 'Retrieval of past operator turns and lessons by meaning rather than keyword. Uses cosine ANN over embeddings when Infinity is reachable; degrades gracefully to Postgres full-text search (FTS) when it is not.'
	},
	embeddings: {
		slug: 'embeddings',
		term: 'Embeddings / Infinity',
		short: 'Vectors from an out-of-process Infinity service (nomic-embed-text-v1.5, 768-dim).',
		long: 'The vectors that power semantic recall, computed out-of-process by an Infinity HTTP service running `nomic-embed-text-v1.5` (768-dim). Enabled by default and settable from /settings; recall falls back to FTS if Infinity is unreachable.'
	},

	sediment: {
		slug: 'sediment',
		term: 'Sediment layers',
		short: 'Visual metaphor: each captured system-prompt composition is a horizontal layer.',
		long: "A visual representation of captured system-prompt compositions stacked like geological strata. Each layer's width encodes its byte size; the largest composition sits at the bottom. Layers accumulate as Claude Code varies its system prompt across sessions, agents, and tools."
	},
	fleet: {
		slug: 'fleet',
		term: 'Fleet',
		short: 'The roster of all steerers — the agents watching live sessions for drift.',
		long: 'The collection of configured steerers available to watch live sessions. Each steerer in the fleet has a name, an authority level, and a drive mode. The fleet view shows which steerers are active and lets you assign them to sessions.'
	}
};

export function helpFor(slug: string): HelpEntry | undefined {
	return HELP_CONTENT[slug];
}
