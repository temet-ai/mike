<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { ChevronRight, Search, SearchX, X } from '@lucide/svelte';
	import { getSteeringVocabulary } from '$lib/api/steering';
	import type { SteeringVocabulary, SteeringVocabularyItem } from '$lib/api/steering';
	import FilteredEmptyState from '$lib/components/shared/FilteredEmptyState.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';

	interface Entry {
		slug: string;
		term: string;
		monoTitle?: string;
		body: string[];
		subAnchors?: Array<{ slug: string; label: string; description: string }>;
		seenAt: string[];
		source?: string;
	}

	interface Group {
		id: string;
		title: string;
		blurb: string;
		entries: Entry[];
	}

	const groups: Group[] = [
		{
			id: 'group-capture',
			title: 'Capture & wiring',
			blurb:
				'How traffic is wired between Claude Code and Anthropic, and how requests are recorded as they flow through.',
			entries: [
				{
					slug: 'session',
					term: 'Session',
					body: [
						'A run of Claude Code (or any Anthropic-API client). Requests are grouped by `session_id`, which Claude Code emits in the `x-claude-code-session-id` header. Sessions inherit their project from the client.',
						'Sessions are the top-level grouping in the SPA: the home page lists sessions, and request lists are scoped to a single session.'
					],
					seenAt: ['/', '/sessions/{id}', 'request detail header'],
					source: 'crates/mike/src/store/models.rs (Session)'
				},
				{
					slug: 'request',
					term: 'Request',
					body: [
						'A single `/v1/messages` POST captured on the wire. Each request has a numeric id, full request and response bytes, and metadata (model, status, timing, token usage).',
						'Requests are the lowest grouping in the SPA — every diagnostic, finding, and inspection drill-down ultimately points at a request id.'
					],
					seenAt: ['/sessions/{id}', '/sessions/{id}/requests/{requestId}'],
					source: 'crates/mike/src/store/models.rs (InterceptedRequest)'
				},
				{
					slug: 'project',
					term: 'Project',
					body: [
						'The filesystem directory (or other identifier) the client was operating in when it spawned the session. Sessions inherit their project from the client; one is never invented on capture.',
						'Projects let operators apply pin / TTL overrides at coarser-than-session granularity.'
					],
					seenAt: ['/sessions list (per-session header)'],
					source: 'crates/mike/src/store/models.rs (Project)'
				},
				{
					slug: 'byte-identity-forwarding',
					term: 'Byte-identity forwarding',
					body: [
						"Identity is the no-op default, not a guarantee: when nothing in the pipeline fires, the forwarded bytes match what the client sent (and vice versa on the way back), which is why thinking-block signatures and Claude Code's retry logic normally keep working. There is no SHA/identity middleware enforcing this — it is simply what happens when no rule or injector has anything to do.",
						"Six systems can rewrite the outbound request body, in fixed order: the operator filter, `lazy_sections` re-injection, the recall injector, the lesson/finding reminders pipeline, the steerer-reminder queue drain, and prompt-composition splicing — and the filter and most of the others reach both the `system` block and every message's content, not `system` alone. Inbound, the weasel intercept can discard Anthropic's response entirely and substitute a synthetic correction. What IS invariant regardless: capture always stores the original, pre-mutation inbound bytes, so inspection fidelity holds no matter what the pipeline does to the forwarded copy."
					],
					seenAt: ['internal invariant — visible in capture vs. forwarded body diff'],
					source: 'docs/ARCHITECTURE.md §4, docs/REQUEST_FLOW.md, crates/mike/src/proxy/handler.rs'
				},
				{
					slug: 'sse-accumulator',
					term: 'SSE accumulator',
					body: [
						'The per-request store of streaming response events. Every `event: …` / `data: …` pair the upstream emits is captured verbatim and stored alongside the request, preserving byte identity for full-fidelity replay and analysis.',
						'The accumulator is what makes the analysis pipeline possible — analyzers run against the accumulated text after the stream completes, against the same bytes the client received.'
					],
					seenAt: ['request detail "Response stream" tab'],
					source: 'crates/mike/src/store/updater.rs'
				},
				{
					slug: 'inspect-api',
					term: 'Inspect API',
					body: [
						'The read/write REST API for this installation. Powers the browser, the MCP bridge, and operator scripts. Getting started shows the running addresses. Endpoints under `/api/*` plus a `/api/ws` WebSocket for live event streaming.',
						'The installed mike binary serves the bundled browser and API together. The mike-mcp bridge calls the inspect API; its tools are selected for user workflows, so not every API route has an MCP tool.'
					],
					seenAt: ['help drawer → "Open this instance’s inspect API"'],
					source: 'crates/mike/src/api/, crates/mike/src/api/manifest.rs'
				},
				{
					slug: 'anthropic-base-url',
					term: 'ANTHROPIC_BASE_URL',
					monoTitle: 'ANTHROPIC_BASE_URL',
					body: [
						"The environment variable Claude Code reads to choose its API target. Set it to the running proxy address shown in Getting started to route Claude Code's traffic through mike. Do not add `/v1`. Capture requires a valid session header.",
						"Without this variable Claude Code talks to Anthropic directly and nothing is captured. The home page's empty-state setup snippet shows the exact `ANTHROPIC_BASE_URL=… claude` invocation."
					],
					seenAt: ['/ empty-state onboarding card'],
					source: 'crates/mike/src/config.rs (proxy bind), README.md'
				}
			]
		},
		{
			id: 'group-lifecycle',
			title: 'Request lifecycle & shape',
			blurb:
				'The verbs, statuses, and parameters captured on each request — the metadata you read first when triaging.',
			entries: [
				{
					slug: 'request-status',
					term: 'Request status',
					body: [
						'The lifecycle state of a captured request. One of five canonical statuses is assigned as the request is captured; the SPA renders each with a coloured pill (green for complete, amber for in-flight, red for error).',
						'Status is computed from the upstream interaction, not the downstream client — a streaming response that the client disconnected from is still `Complete` once Anthropic finished sending it. Errors at any layer (network failure, upstream non-2xx, mid-stream drop) collapse into the `Error` state with a structured detail string.'
					],
					subAnchors: [
						{
							slug: 'status-complete',
							label: 'Complete',
							description:
								'Request finished, response stored in full. The captured bytes are authoritative; no further updates will arrive.'
						},
						{
							slug: 'status-streaming',
							label: 'Streaming',
							description:
								'Response is still arriving via SSE. The accumulated text in the store grows as chunks land — open the request detail to watch live. The status flips to Complete on `message_stop`.'
						},
						{
							slug: 'status-pending',
							label: 'Pending',
							description:
								'Request received and forwarded, but no response chunks have arrived yet. Brief — usually flips to Streaming or Complete within a second.'
						},
						{
							slug: 'status-error',
							label: 'Error',
							description:
								'Upstream returned a non-2xx status, the connection dropped mid-stream, or some other failure. The captured error body is reachable from the request detail.'
						},
						{
							slug: 'status-unknown',
							label: 'Unknown',
							description:
								'Status not recognised by the SPA — usually means the backend is on a newer schema. Fallback presentation; the underlying capture is still intact.'
						}
					],
					seenAt: ['session list rows', '/sessions/{id} request list', 'request detail header'],
					source: 'crates/mike/src/store/models.rs (RequestStatus)'
				},
				{
					slug: 'http-method',
					term: 'HTTP method',
					body: [
						"The HTTP verb on the captured request. Anthropic's `/v1/messages` endpoint is POST-only, so almost every captured request is `POST /v1/messages`. Every method is forwarded verbatim — `OPTIONS` preflights from non-Claude-Code clients also appear."
					],
					seenAt: ['request list rows', 'request detail header'],
					source: 'crates/mike/src/extract/request.rs'
				},
				{
					slug: 'http-status',
					term: 'HTTP response status',
					body: [
						"The numeric HTTP status code Anthropic returned. The status is preserved verbatim — a 429 stays 429, a 529 stays 529. Any rewrite would break Claude Code's retry logic.",
						'2xx is success. 4xx is client error (400 bad request, 401 unauthorized, 429 rate-limited). 5xx is server error (502 bad gateway, 529 overloaded). The `retry-after` header travels alongside 429s and 529s.'
					],
					seenAt: ['request detail Parameters section', 'response headers'],
					source: 'crates/mike/src/proxy/handler.rs'
				},
				{
					slug: 'streaming-vs-non-streaming',
					term: 'Streaming vs. non-streaming',
					body: [
						"Anthropic's `/v1/messages` endpoint accepts both streaming (`stream: true`) and non-streaming requests. Claude Code uses streaming exclusively so chunks arrive as the model generates them.",
						'Streaming responses use Server-Sent Events: `event: message_start`, then a sequence of `event: content_block_delta` chunks, `event: message_delta`, and finally `event: message_stop`. Each event is captured byte-for-byte and replayed to the client; the SSE accumulator keeps the full text for analysis.',
						'Non-streaming responses return one JSON body. It is still captured in full; it just renders as a single block in the response viewer.'
					],
					seenAt: ['request detail Parameters section', 'response stream tab'],
					source: 'crates/mike/src/store/updater.rs'
				},
				{
					slug: 'request-parameters',
					term: 'Request parameters',
					body: [
						'The fields a client sends on `/v1/messages` to control generation. The Details tab surfaces the most common ones; the full request body is reachable via the body drawer.'
					],
					subAnchors: [
						{
							slug: 'param-max-tokens',
							label: 'max_tokens',
							description:
								"Upper bound on output tokens the model is allowed to generate. The model may stop earlier (end_turn / stop_sequence / tool_use). When max_tokens is reached the response's stop_reason is `max_tokens`."
						},
						{
							slug: 'param-temperature',
							label: 'temperature',
							description:
								'Sampling temperature: 0.0 produces deterministic output (greedy decoding), 1.0 produces high-variance output. Most production prompts use 0.0 or 0.3.'
						},
						{
							slug: 'param-streaming',
							label: 'streaming',
							description:
								'Whether the response is delivered as SSE chunks (`true`, the Claude Code default) or as one JSON body (`false`).'
						},
						{
							slug: 'param-tool-loop',
							label: 'tool_loop',
							description:
								'A derived flag — true when this request is the client responding to a previous tool_use. Lets operators trace tool-use cycles without parsing message history.'
						},
						{
							slug: 'param-response-status',
							label: 'response_status',
							description:
								'The HTTP status code Anthropic returned. 200 on success; 429 or 529 if rate-limited; 5xx on upstream error. Preserved verbatim — status is never rewritten.'
						}
					],
					seenAt: ['request detail Details tab → Parameters section'],
					source: 'crates/mike/src/extract/request.rs'
				},
				{
					slug: 'model-id',
					term: 'Model id',
					body: [
						"Anthropic's opaque model identifier — the canonical form is `claude-{family}-{tail}`, e.g. `claude-opus-4-7`, `claude-sonnet-4-6`, `claude-haiku-4-5`.",
						'The id is forwarded verbatim. Different ids on consecutive requests in one session is normal — Claude Code may pick haiku for simple turns and opus for complex ones.'
					],
					seenAt: ['request detail header', 'request list rows', 'session overview MODELS row'],
					source: 'crates/mike/src/extract/request.rs'
				}
			]
		},
		{
			id: 'group-response-metadata',
			title: 'Response metadata',
			blurb:
				"Identifiers and stop-reason information Anthropic emits on every response — the keys you'll quote in support tickets.",
			entries: [
				{
					slug: 'stop-reason',
					term: 'Stop reason',
					body: [
						'The Anthropic-supplied reason the model finished generating. Surfaced in the metrics strip and on the messages token bar.'
					],
					subAnchors: [
						{
							slug: 'stop-end-turn',
							label: 'end_turn',
							description:
								'The model decided it was done. The default outcome for normal generation.'
						},
						{
							slug: 'stop-max-tokens',
							label: 'max_tokens',
							description:
								"The response hit the request's `max_tokens` cap before the model finished. The output may be truncated mid-thought."
						},
						{
							slug: 'stop-stop-sequence',
							label: 'stop_sequence',
							description:
								"The output matched one of the request's `stop_sequences`. Rare for Claude Code traffic; common in custom integrations."
						},
						{
							slug: 'stop-tool-use',
							label: 'tool_use',
							description:
								'The model emitted a `tool_use` content block and is waiting for the client to return a `tool_result`. The next request will typically be a tool-loop request.'
						}
					],
					seenAt: ['request detail metrics strip (Stop cell)', 'messages token bar (Stop cell)'],
					source: 'crates/mike/src/extract/response.rs'
				},
				{
					slug: 'upstream-request-id',
					term: 'upstream_request_id',
					monoTitle: 'upstream_request_id',
					body: [
						"Anthropic's `request-id` response header (`req_xxx`). Distinct from the numeric `request.id` recorded here. Quote this id in support tickets — Anthropic's logs are keyed on it.",
						'Available once the response head is captured (the header arrives before any body chunks). Null on requests that errored before reaching Anthropic.'
					],
					seenAt: [
						'request detail Details tab → Identifiers section',
						'response headers (request-id row)'
					],
					source: 'crates/mike/src/extract/response.rs'
				},
				{
					slug: 'upstream-message-id',
					term: 'upstream_message_id',
					monoTitle: 'upstream_message_id',
					body: [
						"Anthropic's `message_start.id` (`msg_xxx`). Identifies the assistant message inside Anthropic's system. Streaming-only — non-streaming responses don't emit `message_start`.",
						'Used for support correlation alongside `upstream_request_id`. It is stored here but not forwarded to Anthropic on subsequent requests.'
					],
					seenAt: ['request detail Details tab → Identifiers section'],
					source: 'crates/mike/src/extract/response.rs'
				}
			]
		},
		{
			id: 'group-prompts',
			title: 'Prompt compositions',
			blurb: 'How the system-prompt structures clients send are classified and tracked.',
			entries: [
				{
					slug: 'composition',
					term: 'Composition',
					body: [
						'A captured system-prompt composition — the deduplicated structure of system blocks observed across requests. A composition is identified by its `prompt_sha256` (the canonical structural hash of its blocks).',
						"Compositions are not the same as a single request's prompt: a composition is the shape the prompt has, repeated across many requests. The `/prompts` page lists every composition observed, with a count of how many requests resolved to it.",
						'Compositions have a mode (observed / file-backed / dynamic) — see `composition mode` for the difference.'
					],
					seenAt: [
						'/prompts',
						'/prompts/{prompt_sha256}',
						'request detail header (system-prompt class chip)'
					],
					source: 'crates/mike/src/compose_prompt/'
				},
				{
					slug: 'prompt-sha256',
					term: 'prompt_sha256',
					monoTitle: 'prompt_sha256',
					body: [
						'The deduplicated identity of a system-prompt composition — sha256 of the canonical structural bytes of its block list, with volatile lines normalised. Informational only: the prompt catalogue is keyed on `(lane, subagent_persona)`, not on this sha (a per-request nonce, not a stable class identity).',
						'Two requests with the same `prompt_sha256` had the same system-prompt structure. Different `prompt_sha256` values mean a structural difference (different blocks, different order, different cache markers) — not just a wording change in a single block.'
					],
					seenAt: ['request detail "System prompt" tab', 'composition list rows'],
					source: 'crates/mike/src/compose_prompt/blocks.rs'
				},
				{
					slug: 'block-hash',
					term: 'Block hash',
					body: [
						'SHA-256 of the canonical bytes of a single system-prompt block. Used to deduplicate identical block bodies across compositions and to drive the cross-composition lookup in the system-prompt breakdown view.',
						'Block hashes answer questions like "which other compositions also include this CLAUDE.md block?" without storing the body text repeatedly.'
					],
					seenAt: ['system-prompt breakdown panel (per-block hash chip)'],
					source: 'crates/mike/src/compose_prompt/blocks.rs'
				},
				{
					slug: 'composition-mode',
					term: 'Composition mode',
					body: [
						'Compositions track how each one is being treated: simply observed as the client sends it, or actively rewritten on the outbound system block based on a stored template. The mode is per-composition state, not per-request.',
						'A new composition starts as `observed`. An operator can promote it to `file-backed` (the rewrite uses a TOML-backed template) or `dynamic` (the rewrite is computed at request time from a captured baseline).'
					],
					subAnchors: [
						{
							slug: 'mode-observed',
							label: 'observed',
							description:
								"This composition's shape has been captured but outbound system blocks are not being rewritten. The client's bytes pass through unchanged. This is the default for every newly seen composition."
						},
						{
							slug: 'mode-file-backed',
							label: 'file-backed',
							description:
								'The outbound system block is rewritten using a TOML-backed template the operator authored. The captured shape is the source for the template, but the template is the authority once promoted.'
						},
						{
							slug: 'mode-dynamic',
							label: 'dynamic',
							description:
								"The outbound system block is rewritten using a dynamic recipe — typically the captured baseline plus filter-rule deltas. Useful when the client's prompt should be normalised at the structural level but content should remain operator-controlled."
						}
					],
					seenAt: ['/prompts/{prompt_sha256} (mode pill in header)'],
					source: 'crates/mike/src/compose_prompt/store.rs'
				},
				{
					slug: 'anomaly',
					term: 'Anomaly',
					body: [
						'A system-prompt shape that could not be classified cleanly — either an unknown class or a structural mismatch against known compositions. Anomalies are an honest "I don\'t recognise this" signal; they don\'t block the request, they record that the shape diverged.',
						'Two failure modes drive an anomaly: (1) `unknown-class` — a brand-new composition never seen before; (2) `structural-mismatch` — a composition that resembles a known one but differs in detail (block count, block order, hash drift on individual blocks).',
						"Anomalies surface in the Anomalies tab on `/prompts`. Each row's recommended action decides where it points: straight to `/prompts` when there's no request or session id to follow, into the triggering request's transformations tab to view the filter diff, to `/prompts/new` to register the composition as a known class, or to the raw request itself. There is no quick-create flow that turns an anomaly directly into a filter rule."
					],
					seenAt: ['/prompts (Anomalies tab)', 'request detail (anomaly chip when present)'],
					source: 'crates/mike/src/compose_prompt/decision.rs'
				},
				{
					slug: 'drift-queue',
					term: 'Drift queue',
					body: [
						'Captured system prompts that resemble this composition but differ in detail — candidates for promotion or rule change. The drift queue is per-composition and shows the structural diff against the canonical shape.',
						"Used during prompt-engineering work: when a client's prompt drifts (an extra reminder, a new tool definition, a reordered block), the drift queue lets the operator decide whether to incorporate the change into the composition's template, fold it into a filter rule, or treat it as noise."
					],
					seenAt: ['/prompts/{prompt_sha256} (Drift tab)'],
					source: 'crates/mike/src/compose_prompt/store.rs'
				},
				{
					slug: 'composition-match',
					term: 'Composition match',
					body: [
						'A per-block flag describing how each captured system block compares to its registered composition. The system-prompt breakdown view renders one chip per block.'
					],
					subAnchors: [
						{
							slug: 'match-matched',
							label: 'matched',
							description:
								"Block bytes are identical to the composition's expected bytes (modulo volatile lines). This is the happy path."
						},
						{
							slug: 'match-unknown',
							label: 'unknown',
							description:
								"Block is in a position the composition expects, but its content doesn't match — drift. The drift queue picks these up."
						},
						{
							slug: 'match-extra',
							label: 'extra',
							description:
								'Block exists in the captured request but not in the composition. Either the composition needs extending or the block belongs in a filter rule.'
						}
					],
					seenAt: ['request detail System tab (per-block composition chip)'],
					source: 'crates/mike/src/compose_prompt/decision.rs'
				},
				{
					slug: 'system-block',
					term: 'System block',
					body: [
						'One element of the `system` array on a `/v1/messages` request. The Anthropic API accepts the system field as either a JSON string (single-block legacy form) or an array of `{type: "text", text: "..."}` objects (current form).',
						'Claude Code sends a 3-block array: `system[0]` is the model-version preamble, `system[1]` is the cache-control marker, `system[2]` is the operator-supplied instructions block. The composition machinery hashes these structurally to derive the `prompt_sha256`.'
					],
					seenAt: [
						'request detail System tab',
						'/prompts compositions list (per-block breakdown)',
						'/prompts anomalies (system[2] preview)'
					],
					source: 'crates/mike/src/compose_prompt/blocks.rs'
				},
				{
					slug: 'system-reminder',
					term: 'system-reminder',
					monoTitle: '<system-reminder>',
					body: [
						'A wire convention Claude Code uses to inject operator-supplied reminders before the user message. Rendered in the user-message block as `<system-reminder>…</system-reminder>` text.',
						"These are not Anthropic-defined — they're Claude Code's convention for surfacing the contents of `CLAUDE.md`, agent definitions, hook output, and other operator-context that should reach the model alongside the user's actual prompt. They are captured and rendered in full."
					],
					seenAt: [
						'request detail Messages tab (REMINDER chip on user blocks)',
						'request body (raw text wrapped in `<system-reminder>` tags)'
					],
					source: 'crates/mike/src/extract/request.rs (system-reminder detection)'
				},
				{
					slug: 'anomaly-sub-pattern',
					term: 'Anomaly sub-pattern',
					body: [
						"A more specific classification within an anomaly's top-level kind (`unknown-class` vs. `structural-mismatch`). Each sub-pattern has a recommended-action hint that drives the highlighted CTA on the anomaly row.",
						"Sub-patterns include `unregistered-subagent` (a subagent persona the operator has never registered), `unknown-class` fallback (no specific sub-pattern matched), and structural-mismatch variants. The sub-pattern's description and recommended action come from the analyzer source."
					],
					seenAt: ['/prompts anomalies (sub-pattern label next to severity dot)'],
					source: 'crates/mike/src/compose_prompt/decision.rs (sub-pattern derivation)'
				},
				{
					slug: 'volatile-lines',
					term: 'Volatile lines',
					body: [
						'Patterns the prompt-class hash normalises away before computing `prompt_sha256`. Without normalisation, every wall-clock-stamped line in a system prompt would produce a unique hash, and the composition list would explode.',
						'The default set covers timestamps, working-directory paths, and other per-invocation variables. Operators can extend the set on the `/prompts/volatile-lines` page; rules apply to all subsequent classifications. Existing captured rows keep their original hash — re-classification is a manual operation.'
					],
					seenAt: ['/prompts/volatile-lines'],
					source: 'crates/mike/src/compose_prompt/blocks.rs (volatile-line normalisation)'
				}
			]
		},
		{
			id: 'group-analysis',
			title: 'Analysis',
			blurb:
				'How rhetorical patterns are detected in captured response bodies and surfaced as findings.',
			entries: [
				{
					slug: 'weasel-pattern',
					term: 'Weasel pattern',
					body: [
						'Behavioural patterns detected in captured request bodies — diagnostic findings, not blocks. A weasel pattern is a regex (or family of regexes) that fires when the assistant uses language that historically correlates with shortcut-taking, blame-shifting, work-avoidance, false confidence, or overselling.',
						'Findings are diagnostic, never prescriptive: a request is never blocked because of a finding. They link back to the request that produced them so the operator can decide whether the language was warranted.',
						'See the next entry for the canonical category list.'
					],
					seenAt: ['/analysis', 'request detail "Findings" tab when present'],
					source: 'crates/mike/src/analysis/'
				},
				{
					slug: 'weasel-categories',
					term: 'Weasel-pattern categories',
					body: [
						'There are five categories. Each is a separate analyzer with its own pattern set; a request can fire findings in multiple categories. The list below is the canonical set as defined in the analyzer source — adding a category is a code change in `crates/mike/src/analysis/categories.rs` and `store/models.rs::DiagnosticCategory`.'
					],
					subAnchors: [
						{
							slug: 'cat-justifying-shortcuts',
							label: 'Justifying shortcuts',
							description:
								'Language that frames cutting scope as a virtue ("pragmatically", "for now", "lightweight version", "minimal implementation"). The advice is: pragmatism is not a licence to reduce scope; "later" rarely arrives.'
						},
						{
							slug: 'cat-deflecting-blame',
							label: 'Deflecting blame',
							description:
								'Language that suggests a problem isn\'t the assistant\'s ("pre-existing", "it appears", "seems to be a known issue", "as expected"). The advice is: cite the commit, link the issue, or own the change.'
						},
						{
							slug: 'cat-avoiding-work',
							label: 'Avoiding work',
							description:
								'Language that punts effort ("straightforward", "trivial", "revisit later", "out of scope", "natural follow-up"). The advice is: do it now or state explicitly that you didn\'t.'
						},
						{
							slug: 'cat-faking-understanding',
							label: 'Faking understanding',
							description:
								'Language that claims alignment with code patterns without evidence ("consistent with existing pattern", "following convention", "aligning with"). The advice is: cite file:line, show 3+ examples, or admit it\'s new.'
						},
						{
							slug: 'cat-overselling',
							label: 'Overselling',
							description:
								'Language that overstates quality ("robust", "comprehensive", "production-ready", "clean code"). The advice is: enumerate the error cases, list what\'s covered, show the tests, name the specific quality attribute.'
						}
					],
					seenAt: ['/analysis (category cards)', 'config under `[analysis.categories.*]`'],
					source: 'crates/mike/src/analysis/categories.rs, crates/mike/src/analysis/patterns.rs'
				},
				{
					slug: 'grader',
					term: 'Turn judge',
					body: [
						"The turn judge grades each closed operator turn on two axes: how the operator reacted, and how the turn was delivered. It reads the head of the operator's prompt, the tail of the assistant's final text, counts of the tool calls in between, and the operator's next genuine message, which is the strongest evidence of how the turn landed. Every call writes a recorded verdict with the configuration bundle that produced it, and the turn's outcome (accepted, corrected, abandoned and so on) is projected from the live verdict.",
						'A judge is distinct from a steerer: a steerer watches a live session and nudges it back on task while it runs, whereas a judge only grades a turn after the fact and never intervenes.'
					],
					seenAt: [
						'operator turn rows (verdict pill and reaction chips)',
						'session detail (turn outcomes)'
					],
					source:
						'crates/mike/src/analysis/judge/, crates/mike/src/store/persistence/turn_verdicts/'
				},
				{
					slug: 'end-label',
					term: 'End label',
					body: [
						'The end label says why a session ended: finished, handed back, abandoned, or closed in frustration. A rule-based label is written first from the dossier facts; the session judge refines it once promoted. A frustration close counts as a correction of the turn it followed, so correction figures include sessions the operator walked away from.'
					],
					seenAt: [
						'session detail (session-end chip)',
						'operator turn rows (counts as correction)'
					],
					source: 'crates/mike/src/session_end/, crates/mike/src/eval/session_end/'
				},
				{
					slug: 'turn-guard',
					term: 'Turn guard',
					body: [
						'The turn guard asks four questions at the end of each turn, and again after the session goes quiet: is the model repeating a mistake the operator already corrected, is it waiting on something nothing will deliver, is it handing back work the operator authorised it to finish, and is it claiming done without having observed the result. Every question starts in shadow, where answers are only recorded. The operator can promote a question to append a nudge to the next turn, or to wake an idle session, once labelled evidence shows it is precise enough; a question that misfires is demoted automatically.'
					],
					seenAt: [
						'/settings/steering (guard questions panel)',
						'session steering timeline (wake state and verdict controls)'
					],
					source: 'crates/mike/src/steerer/guard/'
				},
				{
					slug: 'analyzer',
					term: 'Analyzer',
					body: [
						'A check that scans the captured assistant output for a particular pattern — for example weasel phrasing — and records any matches as findings. Each analyzer owns one pattern family; a single request can be examined by several analyzers.',
						'An analyzer can run after a turn finishes, against the stored response, or in-flight as the response streams through. It only records what it sees; it never blocks or rewrites the response.'
					],
					seenAt: ['/analysis', 'request detail "Findings" tab when present'],
					source: 'crates/mike/src/analysis/'
				},
				{
					slug: 'finding',
					term: 'Finding',
					body: [
						'One result an analyzer recorded against a request — the matched span of text together with its category and supporting metadata. Findings are the unit the Analysis page counts and groups.',
						'Findings are diagnostic records, not blocks: each one links back to the request that produced it so the operator can read the surrounding context and decide whether the language was warranted. Nothing is truncated — the matched span is stored in full.'
					],
					seenAt: ['/analysis (finding rows)', 'request detail "Findings" tab when present'],
					source: 'crates/mike/src/analysis/, crates/mike/src/store/models.rs (Finding)'
				}
			]
		},
		{
			id: 'group-filtering',
			title: 'Filtering',
			blurb: 'How operators rewrite outbound system prompts before they reach Anthropic.',
			entries: [
				{
					slug: 'filter-rule',
					term: 'Filter rule',
					body: [
						"A user-configured rewrite applied to the outbound request. One of six systems that can rewrite the outbound body before it reaches Anthropic (see Byte-identity forwarding, above) — walks both the `system` block and every message's content, not `system` alone.",
						'A filter rule has a kind (`text_substring` or `xml_tag`), a value to match against, and an action (drop, replace, or append). Rules are ordered; the first matching rule applies.',
						'Reached from the workbench\'s own toolbar, or from a per-block "Filter this" link on any request\'s system-prompt view — a generic entry point available on every request, not something anomalies create or prefill. An anomaly row can lead an operator to the triggering request, and from there to that generic link, but nothing about the rule is derived from the anomaly itself.'
					],
					seenAt: [
						'filter workbench (toolbar "filters" button, /filter/workbench)',
						'request detail — "Filter this" link on a system-prompt block'
					],
					source: 'crates/mike/src/filter/, crates/mike/src/store/persistence/filter_rules.rs'
				},
				{
					slug: 'filter-rule-kind',
					term: 'Filter rule kind',
					body: ["How a filter rule's `value` is matched against the captured system block."],
					subAnchors: [
						{
							slug: 'kind-text-substring',
							label: 'text_substring',
							description:
								'Match on a literal substring anywhere in the block text. The match is case-sensitive and does not interpret regex metacharacters. Use this for short, distinctive operator-supplied phrases.'
						},
						{
							slug: 'kind-xml-tag',
							label: 'xml_tag',
							description:
								'Match a `<tag>…</tag>` element by tag name (no angle brackets in the value). Convenient for the `<system-reminder>` and similar wrapping conventions Claude Code emits.'
						}
					],
					seenAt: ['filter workbench rule stack (kind pill on each rule card)'],
					source: 'crates/mike/src/filter/rule.rs'
				},
				{
					slug: 'filter-rule-action',
					term: 'Filter rule action',
					body: ['What the rule does when its match fires.'],
					subAnchors: [
						{
							slug: 'action-drop',
							label: 'drop',
							description:
								'Remove the matched block (or matched span, for substring rules) from the outbound system field. The Anthropic-bound bytes do not include it.'
						},
						{
							slug: 'action-replace',
							label: 'replace',
							description:
								"Replace the matched span with the rule's `text` payload. Useful for redacting operator-secret strings without dropping the surrounding context."
						},
						{
							slug: 'action-append',
							label: 'append',
							description:
								"Insert the rule's `text` payload after the matched block. Useful for injecting an operator-controlled instruction in front of a known marker."
						}
					],
					seenAt: [
						'filter workbench rule stack (action badge on each rule card)',
						'rule edit form'
					],
					source: 'crates/mike/src/filter/rule.rs'
				}
			]
		},
		{
			id: 'group-steering',
			title: 'Steering',
			blurb:
				'How autonomous correction agents watch live sessions and nudge them back on task when they drift.',
			entries: [
				{
					slug: 'steering',
					term: 'Steering',
					body: [
						"Steering lets you point a live Claude Code session back at its task when it drifts. You attach a small agent — a steerer — that watches the session's turns, detects when the work wanders off track, and sends a corrective message on your behalf. The session keeps running; the steerer rides alongside it.",
						"Steering is observational by default and never blocks a session — a steerer only fires when its drift detection crosses the configured threshold, and even then the session's Claude decides how to respond (subject to the intervention's authority)."
					],
					seenAt: [
						'/steering',
						'/steering?tab=monitor',
						'session detail (Session Steering section)'
					],
					source: 'docs steering-concepts guide, crates/mike/src/steer/'
				},
				{
					slug: 'steerer',
					term: 'Steerer',
					body: [
						"A cheap watcher agent — typically an OpenRouter model — attached to a live session. After each turn the steerer reads the session's recent turns against an anchor (what the session should be focused on) and produces a drift verdict. When drift is detected above the steerer's confidence threshold, it fires an intervention.",
						'A steerer is a named configuration: a prompt that defines "on track", a model that runs the evaluation, and thresholds controlling sensitivity. One steerer can watch many sessions; one session can have many steerers. The fleet runs off the session\'s hot path, so it never slows the session down. Two steerer types ship today — see Refocus steerer and VerifierSteerer.',
						'The fleet roster lives on the /steering Fleet tab; live fires appear on the Monitor tab.'
					],
					seenAt: ['/steering (Fleet tab roster, Monitor tab feed)', 'session steering section'],
					source: 'crates/mike/src/steerer/, crates/mike/src/steerer/fleet.rs'
				},
				{
					slug: 'intervention',
					term: 'Intervention',
					body: [
						'The steering payload injected into a steered turn — the corrective message a steerer produces when it fires. Each intervention carries an authority level and a mode that together determine how forcefully it lands on the session.',
						"Interventions surface live in the Monitor tab as soon as a steerer fires, each linking back to the session and turn that triggered it. The intervention is added to the session's context; the next Claude Code request carries it alongside everything else."
					],
					seenAt: [
						'/steering?tab=monitor (intervention feed)',
						'session detail (steering activity)'
					],
					source: 'crates/mike/src/steer/ (intervention payload)'
				},
				{
					slug: 'steering-authority',
					term: 'Steering authority',
					body: [
						'How forceful a steering intervention is. Three escalating levels control how much weight the session gives the steered message.',
						'Start with supply_context. Reserve redirect for when context additions consistently fail, and command for well-tested, trusted steerers where the risk of an incorrect intervention is acceptable.'
					],
					subAnchors: [
						{
							slug: 'authority-supply-context',
							label: 'supply_context',
							description:
								'Background information appears in the context. Claude may use it or ignore it. Example: "Note: this session\'s working task is X." The weakest, safest level.'
						},
						{
							slug: 'authority-redirect',
							label: 'redirect',
							description:
								'A suggestion to change direction, phrased as a request; Claude is free to push back. Example: "You appear to be refactoring the wrong file — please return to main.rs."'
						},
						{
							slug: 'authority-command',
							label: 'command',
							description:
								'A binding directive treated as an operator instruction — Claude Code honours operator turns with higher trust than user turns. Example: "Stop. The current approach will break the migration. Revert and follow the spec." The strongest level.'
						}
					],
					seenAt: ['/steering?tab=fleet (per-steerer authority)', 'steerer create / edit form'],
					source: 'crates/mike/src/steer/ (authority enum)'
				},
				{
					slug: 'steering-mode',
					term: 'Steering mode',
					body: [
						'How much the steerer takes over the session when it fires. Mode controls how the steered message arrives.',
						'For most day-to-day steering, augment + supply_context is the right combination. drive should be rare — it is most useful when a session has entered a loop and needs to be broken out entirely.'
					],
					subAnchors: [
						{
							slug: 'mode-augment',
							label: 'augment',
							description:
								"The steering message is added alongside the session's existing context. The session's Claude still generates the next turn naturally. The default mode."
						},
						{
							slug: 'mode-drive',
							label: 'drive',
							description:
								"The steering payload replaces the session's assistant turn for that exchange — the steerer's message becomes the response and the session's Claude does not generate anything for that turn. The most invasive option; use only when augment has failed."
						}
					],
					seenAt: ['/steering?tab=fleet (per-steerer mode)', 'steerer create / edit form'],
					source: 'crates/mike/src/steer/ (mode enum)'
				},
				{
					slug: 'bigbro',
					term: 'Bigbro',
					body: [
						'The role a steerer plays when it fires: it sends a first-class user turn into the watched session rather than disguising itself as the operator. Steered turns are honestly attributed — the BIGBRO ATTRIBUTION card on a steered request names the steerer, its model, the authority, mode, verdict, confidence, the off-thread topic, and the anchor text that grounded the evaluation, all rendered in full.',
						"The intervention is injected via the capture pipeline's own `/v1/messages` endpoint and carries the `x-proxy-steered-intervention` marker, so it is never confused with operator-typed input."
					],
					seenAt: ['request detail (BIGBRO ATTRIBUTION card on steered requests)'],
					source:
						'crates/mike/src/steerer/send.rs, frontend/src/lib/components/detail/BigbroAttribution.svelte'
				},
				{
					slug: 'refocus-steerer',
					term: 'Refocus steerer',
					body: [
						"A steerer type that re-grounds a drifting session. Its LLM reads the session's recent turns against the thread anchor (the worklog State of play / Next text) and, when the work has wandered off the anchored topic, fires a corrective turn that points the session back at its task.",
						'Refocus is the general-purpose drift corrector — use it when a session needs to stay on a defined objective.'
					],
					seenAt: [
						'/steering (Fleet roster — Refocus steerers)',
						'BIGBRO ATTRIBUTION card (steerer name)'
					],
					source: 'crates/mike/src/steerer/refocus.rs'
				},
				{
					slug: 'verifier-steerer',
					term: 'VerifierSteerer',
					body: [
						"A steerer type that is an LLM judge for completion claims. A stage-1 gate fires when the assistant's response contains a completion keyword (`done`, `verified`, `complete`, `fixed`, `tests pass`) or the request carries an Overselling diagnostic. A stage-2 LLM judge then cross-checks the claim against the assistant's actual response text and tool calls.",
						'When the judge finds a completion claim without backing evidence above the confidence threshold, it injects a plain-prose nudge naming the MISSING evidence (e.g. "You claimed the tests pass, but no test command ran this turn - run the suite and show its output.") so the session supplies it.'
					],
					seenAt: [
						'/steering (Fleet roster — Verifier steerers)',
						'BIGBRO ATTRIBUTION card (steerer name)'
					],
					source: 'crates/mike/src/steerer/verifier.rs'
				},
				{
					slug: 'steering-enrolment',
					term: 'Enrolment',
					body: [
						'The link between a session and a steerer. One enrolment = one steerer watching one session. A session can enrol multiple steerers (they run in priority order); a steerer can be enrolled in multiple sessions. Disenrolling removes the link at the next turn evaluation.',
						'Active enrolments are listed in the Steering activity section on the session page, each row naming the steerer and its mode and authority.'
					],
					seenAt: [
						'session page (Steering activity section)',
						'/steering (Fleet tab — enrol / disenrol)'
					],
					source:
						'crates/mike/src/steerer/registry.rs, frontend/src/lib/components/shell/SessionSteeringSection.svelte'
				}
			]
		},
		{
			id: 'group-config',
			title: 'Configuration',
			blurb: 'Lifecycle controls for captured data — what stays, for how long, and how to keep it.',
			entries: [
				{
					slug: 'ttl',
					term: 'TTL (time-to-live)',
					body: [
						'How long captured sessions are retained before automatic eviction. Configurable globally (default), per-project (overrides the global default), and per-session (overrides both).',
						'TTL is the memory-pressure release valve: capture is unbounded by default, eviction keeps the working set finite. A pinned session is exempt from TTL eviction entirely.'
					],
					seenAt: ['session row metadata', 'help drawer (per-session TTL controls)'],
					source: 'crates/mike/src/store/retention.rs, /settings'
				},
				{
					slug: 'project-pin',
					term: 'Project pin',
					body: [
						'Pinning a project marks every session under it as exempt from TTL-based eviction. Sessions within a pinned project are kept until the operator explicitly unpins them or deletes them.',
						"Use this for the project you're actively investigating — the eviction loop won't silently delete data you're mid-analysis on."
					],
					seenAt: ['session list (pin icon next to project name)'],
					source: 'crates/mike/src/api/handlers.rs (project pin handler)'
				}
			]
		},
		{
			id: 'group-tokens',
			title: 'Tokens & cost',
			blurb:
				'How token consumption, prompt-cache efficiency, and locally-computed cost are reported.',
			entries: [
				{
					slug: 'usage-tokens',
					term: 'Usage tokens',
					body: [
						"The token counts Anthropic returns in the response's `usage` object. Drives the metrics strip and messages token bar on the request detail.",
						"Four canonical fields: `input_tokens` (uncached input billed at full rate), `output_tokens` (model output), `cache_read_input_tokens` (input served from prompt cache, 10× cheaper), and `cache_creation_input_tokens` (input written to cache, 1.25× input rate). Cache fields are SUBSETS of input — adding all four would double-count, which is why the messages token bar's Total cell renders `input + output` only."
					],
					seenAt: ['request detail metrics strip', 'request detail messages token bar'],
					source: 'crates/mike/src/extract/response.rs (usage extraction)'
				},
				{
					slug: 'prompt-cache',
					term: 'Prompt cache',
					body: [
						"Anthropic's prompt-caching feature: a client marks parts of its prompt as cacheable and Anthropic stores the model state for that prefix. Subsequent requests with the same cached prefix re-use the state, charging input tokens at 1/10 the normal rate.",
						'Two cache fields are reported per request: `cache_read_input_tokens` (tokens served from cache, the cheap path) and `cache_creation_input_tokens` (tokens written to cache, slightly more expensive than uncached input but the investment pays off on the next hit). The `Cache%` cell is `cache_read / (input + cache_read + cache_create)` — share of input-side tokens served from cache.'
					],
					seenAt: [
						'request detail metrics strip (Cache read / Cache create / Cache% cells)',
						'request detail Analysis tab (cache efficiency ring + breakdown)'
					],
					source: 'frontend/src/lib/format.ts (cachePercent / cost calc)'
				},
				{
					slug: 'cache-control',
					term: 'cache_control',
					monoTitle: 'cache_control',
					body: [
						'A request-side opt-in marker: the client adds `cache_control: { type: "ephemeral" }` to a content block (or system block) to ask Anthropic to cache the prefix up to and including that block.',
						'Without `cache_control`, Anthropic does not cache the request — `cache_read_input_tokens` stays zero. The marker travels in the captured request body; the SPA highlights it on system blocks via the system-prompt breakdown view.'
					],
					seenAt: ['request body (raw view)', 'request detail System tab (per-block cache marker)'],
					source: 'crates/mike/src/compose_prompt/blocks.rs (cache marker detection)'
				},
				{
					slug: 'cache-efficiency',
					term: 'Cache efficiency',
					body: [
						'The share of input-side tokens served from prompt cache, expressed as a percentage. Computed as `cache_read / (input + cache_read + cache_create)`.',
						"Reported on the Analysis tab as a ring chart with a four-row breakdown (Cache read / Cache create / Uncached input / Output). Higher is cheaper. The mockup's green/amber thresholds (>=80% green, >=40% amber) are an opinion, not a rule — the underlying number is just a percentage."
					],
					seenAt: ['request detail Analysis tab', 'request detail metrics strip (Cache% cell)'],
					source: 'frontend/src/lib/components/CacheEfficiency.svelte'
				},
				{
					slug: 'request-cost',
					term: 'Request cost',
					body: [
						'A USD figure computed locally from the captured token counts and the published per-million-token rates for the request’s model. This is NOT a bill — Anthropic does the billing. The cost figure is an estimate for budgeting and per-request triage.',
						'Prices are listed per model in `frontend/src/lib/modelPricing.ts` and are updated by hand when Anthropic changes them. Dated model ids (for example `claude-haiku-4-5-20251001`) use their family’s price. A model that is not in the list is priced at a default and marked as an estimate. Cache writes are priced at the 5-minute rate unless the usage reports the 5-minute and 1-hour split, in which case each part uses its own rate. The line items in the breakdown on the Analysis tab sum to the total cell on the metrics strip (tested in `format.test.ts`).'
					],
					seenAt: [
						'request detail metrics strip (Cost cell)',
						'request detail Analysis tab (cost breakdown table)'
					],
					source:
						'frontend/src/lib/modelPricing.ts (estimateCost), frontend/src/lib/format.ts (formatCost)'
				},
				{
					slug: 'token-estimate',
					term: 'Token estimate',
					body: [
						"A frontend-only approximation of token count, computed as `Math.round(charCount / 4)`. Always rendered with a leading `~` so the reader knows it's an estimate, not Anthropic-billed.",
						'Used on system-prompt blocks and on the session-overview "first user message" preview, where the captured response (and the authoritative token count) hasn\'t arrived yet. Once the response lands, the metrics strip\'s exact `usage` numbers supersede this estimate.'
					],
					seenAt: [
						'session overview PROMPT row',
						'request detail System tab (per-block token estimate)'
					],
					source: 'frontend/src/lib/components/SystemPromptBreakdown.svelte (estimateTokens)'
				}
			]
		},
		{
			id: 'group-content',
			title: 'Conversation & content blocks',
			blurb:
				"How Anthropic message structure is parsed — turns, content blocks, tool exchanges, and the assistant's reasoning trace.",
			entries: [
				{
					slug: 'conversation-message',
					term: 'Conversation message (turn)',
					body: [
						'One message in the `messages` array of a `/v1/messages` request. Anthropic alternates between `user` and `assistant` roles; tool calls and tool results appear as content blocks inside those turns, not as their own messages.',
						'Each message contains a `role` and a `content` array of typed blocks: `text`, `tool_use`, `tool_result`, `thinking`, and others. The Messages tab walks this array to render the conversation; it never renders less than the captured content.',
						'A "request" is a single POST to `/v1/messages` and may carry many messages — typically the entire conversation history up to that point.'
					],
					seenAt: ['request detail Messages tab (USER / ASSISTANT turn rows)'],
					source: 'frontend/src/lib/components/message-view/messageWalker.ts'
				},
				{
					slug: 'tool-call',
					term: 'Tool call',
					body: [
						'An invocation of a Claude tool emitted in the response stream — the assistant naming a tool (e.g. `bash`, `read`, `web_search`) and supplying arguments for the client to execute.',
						'The full arguments and the full result are captured. Tool calls render in the request detail with their inputs and outputs available in full — no preview, no truncation. The list page shows tool names alongside a request as a discoverability hint.'
					],
					seenAt: [
						'request detail "Messages" tab',
						'request list (tool-name chips on assistant turns)'
					],
					source: 'crates/mike/src/extract/response.rs'
				},
				{
					slug: 'tool-result',
					term: 'Tool result',
					body: [
						'A content block of type `tool_result` in a USER message — the client returning the output of a previous `tool_use` (tool call) the assistant emitted.',
						'Tool results carry the matching `tool_use_id` so Anthropic can pair them with the call. The full output is captured in the request body; nothing is truncated.'
					],
					seenAt: ['request detail Messages tab (RESULT chips)'],
					source: 'crates/mike/src/extract/request.rs'
				},
				{
					slug: 'tool-loop',
					term: 'Tool loop',
					body: [
						'A request that is part of a back-and-forth tool-use cycle. The assistant emits a `tool_use`, the client executes the tool, returns the output as a `tool_result` content block on the next user message, and the assistant continues.',
						'Every request whose latest user message contains a `tool_result` is flagged `is_tool_loop: true`. Useful for filtering: "show me only the requests where the assistant is responding to a tool, not generating fresh output".'
					],
					seenAt: ['request detail Details tab → Parameters section'],
					source: 'crates/mike/src/extract/request.rs'
				},
				{
					slug: 'thinking-mode',
					term: 'Thinking mode',
					body: [
						"Anthropic's extended-thinking feature: the model emits a hidden reasoning trace before its visible response. The trace is not shown to the user but is fed back to the model on the next turn for context.",
						'Modes: `off` (no thinking), `on` (always thinks), `adaptive` (decides per-turn based on prompt complexity). When thinking is on the response includes thinking content blocks, each with a cryptographic signature that must be preserved byte-identical for retry safety.'
					],
					seenAt: ['request detail Details tab → Parameters section'],
					source: 'crates/mike/src/extract/request.rs'
				},
				{
					slug: 'thinking-blocks',
					term: 'Thinking blocks',
					body: [
						"Content blocks of type `thinking` that appear in the model's response when extended thinking is on. They contain the model's internal reasoning trace and a signature.",
						"The signature is what makes byte-identity forwarding load-bearing: Anthropic verifies the signature on the next turn to confirm the thinking content wasn't tampered with. Re-serialising the JSON breaks the signature even if the bytes are semantically equivalent — and the filter's rewrite action does exactly that reparse-and-reserialise round trip on any thinking block a rule matches. The original wire bytes are preserved verbatim only when nothing in the pipeline touches the block; a filter rule that matches a thinking block will break it."
					],
					seenAt: ['request detail Messages tab (thinking content blocks)'],
					source: 'crates/mike/src/proxy/handler.rs'
				},
				{
					slug: 'thinking-block-signature',
					term: 'Thinking-block signature',
					body: [
						"A cryptographic signature Anthropic attaches to every thinking content block. Submitted back on subsequent turns so Anthropic can verify the thinking content wasn't tampered with.",
						"This is the canonical reason byte-identity forwarding matters: the signature covers the exact JSON encoding, so even whitespace differences break verification. It is not a guarantee against reserialisation, though — the filter and several of the other request-mutating stages round-trip the body through `serde_json`, and any of them touching a thinking block's content reserialises it and breaks the signature."
					],
					seenAt: ['request detail Messages tab (chip on thinking blocks)'],
					source: 'crates/mike/src/proxy/handler.rs'
				}
			]
		},
		{
			id: 'group-protocol',
			title: 'Anthropic protocol',
			blurb:
				'Header conventions, rate-limit signals, and metadata Anthropic sends alongside `/v1/messages` responses.',
			entries: [
				{
					slug: 'anthropic-headers',
					term: 'Anthropic request/response headers',
					body: [
						'Every header Anthropic sends and the client requests is preserved verbatim. Several have specific operator-facing meaning surfaced via header-row tooltips.',
						'Authentication headers (`x-api-key`, `authorization`, `cookie`, `set-cookie`) are MASKED in the SPA — eight bullets in place of the value. The mask is applied client-side AND server-side; both sides must be bypassed for a leak to occur.',
						'Versioning headers: `anthropic-version` pins the API version (e.g. `2023-06-01`); `anthropic-beta` carries comma-separated beta-feature flags. Both round-trip verbatim.',
						"Diagnostic headers: `request-id` (Anthropic's opaque id), `cf-ray` and `cf-cache-status` (Cloudflare fronting Anthropic), `user-agent` (Claude Code identification)."
					],
					seenAt: ['request detail Headers tab (request and response sub-tabs)'],
					source:
						'crates/mike/src/proxy/headers.rs, frontend/src/lib/components/HeadersTable.svelte'
				},
				{
					slug: 'rate-limit-headers',
					term: 'Rate-limit headers',
					body: [
						'Anthropic emits four rate-limit families on every response, each with three members: `-limit`, `-remaining`, and `-reset` (epoch seconds when the bucket resets).',
						'Families: `anthropic-ratelimit-requests-*` (per-minute request count), `anthropic-ratelimit-input-tokens-*`, `anthropic-ratelimit-output-tokens-*`, and `anthropic-ratelimit-tokens-*` (combined). Twelve headers total. All twelve are preserved as captured — Claude Code reads these to decide whether to slow down.',
						'When the limit is hit Anthropic returns 429 with `retry-after`; when capacity is exhausted server-side it returns 529. Both flow through verbatim.'
					],
					seenAt: ['request detail Headers tab (response headers)'],
					source: 'crates/mike/src/proxy/headers.rs'
				},
				{
					slug: 'retry-after',
					term: 'retry-after',
					monoTitle: 'retry-after',
					body: [
						'A response header Anthropic emits alongside 429 (rate-limited) and 529 (overloaded) statuses. Tells the client how many seconds to wait before retrying.',
						"The value is preserved verbatim — Claude Code's retry logic depends on it. Honour it; ignoring `retry-after` is the fastest way to stay rate-limited."
					],
					seenAt: ['request detail Headers tab (response headers, on 429/529 captures)'],
					source: 'crates/mike/src/proxy/headers.rs'
				}
			]
		},
		{
			id: 'group-agents',
			title: 'Agents',
			blurb:
				'How the agent identities that Claude Code spawns and re-spawns across sessions are tracked, including subagent personas.',
			entries: [
				{
					slug: 'agent-registry',
					term: 'Agent registry',
					body: [
						'The catalogue of Claude agents observed in captured sessions. Each row is one canonical agent identity; each agent owns a chain of prompt versions captured over time.',
						'No agent is ever spawned here — the registry is purely observational — every entry was inferred from a captured request whose system prompt matched a known agent definition pattern. File-backed agents reference the on-disk markdown definition; ephemeral / registry-resident agents are surfaced for tracking even without a definition file.'
					],
					seenAt: ['/agents (top-level catalogue)'],
					source: 'crates/mike/src/store/persistence/agents.rs'
				},
				{
					slug: 'subagent',
					term: 'Subagent',
					body: [
						"A Claude Code instance spawned by the parent agent via the Agent tool. The subagent has its own system prompt, its own task scope, and its own captured request stream — but shares the parent's session id.",
						"The subagent persona (e.g. `frontend-developer`, `rust-developer`) is extracted from the subagent's system prompt and surfaced as a chip on every request the subagent emits. Lets operators distinguish subagent traffic from main-agent traffic without reading the system block."
					],
					seenAt: [
						'request list rows (persona chip)',
						'request detail header (subagent persona chip)',
						'/prompts anomalies (UNREGISTERED SUBAGENT sub-pattern)'
					],
					source: 'crates/mike/src/extract/request.rs (subagent persona extractor)'
				},
				{
					slug: 'agent-version',
					term: 'Agent version',
					body: [
						"A captured snapshot of an agent's system prompt at a point in time. Each row in `/agents` shows the count of distinct versions; clicking through to `/agents/{id}` shows the version timeline with diffs between adjacent versions.",
						'A new version is recorded only when the prompt structure changes — volatile-line normalisation prevents trivial timestamp changes from minting a new version. The diff view uses the same `+` / `-` / `=` indicators as the prompt-composition diff.'
					],
					seenAt: ['/agents (per-agent version count chip)', '/agents/{id} (version timeline)'],
					source: 'crates/mike/src/store/persistence/agents.rs'
				},
				{
					slug: 'mode-builtin',
					term: 'builtin',
					body: [
						'An agent identity that ships with the Claude Code harness itself, rather than a persona the operator (or mike) defined — the fork worker, `general-purpose`, `Explore`, `Plan`, and similar built-in tools.',
						'Builtin agents have no on-disk `.md` definition to point at: their identity ships inside Claude Code, so `definition_path` stays empty even after editing.'
					],
					seenAt: ['/agents (source filter chip + row badge)', '/agents/{id} (source badge)'],
					source: 'crates/mike/src/store/persistence/agents.rs (AgentSource::Builtin)'
				},
				{
					slug: 'agent-dispatches',
					term: 'Dispatches',
					body: [
						'Every captured Agent-tool invocation of this agent — one row per time a parent session spawned it via the Agent tool. Each row carries the full dispatch prompt (expand the row to read it in full), the dispatching session, and the request the dispatch was captured on.',
						'Dispatches are paginated, not truncated: `/agents/{id}` loads a page of rows and offers "load more" for the rest, and the prompt text on each row is always the complete captured value.'
					],
					seenAt: ['/agents/{id} (Dispatches section)'],
					source: 'crates/mike/src/store/persistence/agents.rs'
				},
				{
					slug: 'agent-prompt-versions',
					term: 'Prompt versions (section)',
					body: [
						'The distinct system-prompt shas captured for this agent across real traffic — one row per `content_sha256` actually observed, not a theoretical version history.',
						'A new row appears only when the captured prompt structure changes; volatile-line normalisation keeps timestamp-only drift from minting a new version. See Agent version for the underlying concept.'
					],
					seenAt: ['/agents/{id} (Prompt versions section)'],
					source: 'crates/mike/src/store/persistence/agents.rs'
				},
				{
					slug: 'agent-instances',
					term: 'Instances',
					body: [
						'The subagent sessions bound to this agent identity — one row per Claude Code sub-instance that ran as this agent, with the session it ran in, its persona (when captured), and the first request it produced.',
						'Instances are distinct from Dispatches: a dispatch is the invocation (the Agent-tool call that spawned the work); an instance is the resulting sub-session that actually executed it.'
					],
					seenAt: ['/agents/{id} (Instances section)'],
					source: 'crates/mike/src/store/persistence/agents.rs'
				}
			]
		},
		{
			id: 'group-memory',
			title: 'Memory & recall',
			blurb:
				'How lessons are consolidated like human memory, and past turns and lessons recalled by meaning, not just keyword.',
			entries: [
				{
					slug: 'lesson',
					term: 'Lesson',
					body: [
						'A captured unit of learning written by an agent during a session. Lessons flow through a staged consolidation pipeline (see Memory stages): a background worker embeds each raw lesson, deduplicates it by cosine similarity against existing lessons, and promotes it from RAW to DAILY and eventually to LONG_TERM.',
						'Consolidated lessons (DAILY and LONG_TERM) are injected at session start so a new session inherits what earlier sessions in the same project learned. The worker is enabled by default; injection reads a lock-free in-memory snapshot, so it never touches Postgres on the request hot path.'
					],
					seenAt: ['/diagnostics/memory', 'session-start injected context'],
					source: 'crates/mike/src/lesson_worker.rs, crates/mike/src/store/persistence/lessons.rs'
				},
				{
					slug: 'memory-stages',
					term: 'Memory stages (RAW → DAILY → LONG_TERM)',
					body: [
						'Lessons are consolidated like human memory across three stages. A new lesson starts as RAW; a session-consolidation pass promotes idle RAW lessons to DAILY (embedding + dedup); a nightly pass promotes surviving DAILY lessons to LONG_TERM and trims to the per-project cap. Only DAILY and LONG_TERM lessons are injectable.'
					],
					subAnchors: [
						{
							slug: 'stage-raw',
							label: 'RAW',
							description:
								'A freshly written lesson, not yet consolidated. Picked up by the session-consolidation pass once it has been idle long enough.'
						},
						{
							slug: 'stage-daily',
							label: 'DAILY',
							description:
								'A RAW lesson promoted after embedding and cosine-similarity dedup against existing lessons. Injectable at session start.'
						},
						{
							slug: 'stage-long-term',
							label: 'LONG_TERM',
							description:
								'A DAILY lesson that survived the nightly consolidation pass and was kept within the per-project cap. The most durable tier; injectable at session start.'
						}
					],
					seenAt: ['/diagnostics/memory'],
					source: 'crates/mike/src/lesson_worker.rs'
				},
				{
					slug: 'operator-turn',
					term: 'Operator turn',
					body: [
						'One row per Claude Code user prompt, spanning everything the model did from that prompt until the next operator input — the requests, tool calls, tool results, agent dispatches, and tokens of that stretch of work. An operator turn is OPEN while it is the trailing turn of a live session and frozen (CLOSED, immutable) once the next operator prompt arrives.',
						'Operator turns are the unit semantic recall operates over: each closed turn is embedded so past work can be recalled by meaning. Aggregate columns are computed on demand from the normalised tables, never as truncated previews — the full prompt and assistant text ride through verbatim.'
					],
					seenAt: ['/diagnostics/memory', 'semantic-recall results'],
					source:
						'crates/mike/src/store/operator_turns.rs, crates/mike/src/store/persistence/operator_turns.rs'
				},
				{
					slug: 'turn-outcome',
					term: 'Turn outcome',
					body: [
						"How a judge model graded one operator turn — a single label answering \"did this ask get what it wanted?\". A cheap model reads the operator's prompt, the assistant's final response, and the turn's tool-use counts, then assigns exactly one of five labels. It is a read of the transcript after the fact, not a measurement taken while the turn ran, so treat it as a fallible second opinion rather than ground truth.",
						"The counts shown on a session page are these labels tallied across every operator turn in the session that has been graded. Turns the judge has not reached yet are absent from the tally entirely, so the total can be lower than the session's operator-ask count."
					],
					subAnchors: [
						{
							slug: 'outcome-accepted',
							label: 'accepted',
							description:
								'The positive default: real work happened and nothing signals it was rejected. Code written, files changed, a question answered substantively — with no ship action and no sign of failure.'
						},
						{
							slug: 'outcome-shipped',
							label: 'shipped',
							description:
								'Accepted, plus a concrete delivery the assistant confirmed happened: a commit, a push, a PR opened or merged, a deploy, a release.'
						},
						{
							slug: 'outcome-corrected',
							label: 'corrected',
							description:
								'The work was wrong, incomplete, or broken — the operator pushed back ("still broken", "you missed X"), or the assistant caught its own error and retried.'
						},
						{
							slug: 'outcome-abandoned',
							label: 'abandoned',
							description:
								'The turn was cut short or produced no real work: the operator gave up mid-task, or the assistant returned only a refusal or a plan it never acted on.'
						},
						{
							slug: 'outcome-unknown',
							label: 'unknown',
							description:
								'Genuinely insufficient signal to decide. Also what a turn reads as before the judge has graded it.'
						}
					],
					seenAt: ['session detail (Session metrics → Outcomes)', '/diagnostics/memory'],
					source: 'crates/mike/src/analysis/judge.rs, crates/mike/src/judge_worker.rs'
				},
				{
					slug: 'semantic-recall',
					term: 'Semantic recall',
					body: [
						'Retrieval of past operator turns and lessons by meaning rather than exact keyword. Each closed turn and lesson is embedded into a vector, and the most similar prior items are found by cosine ANN — so a new turn can recall relevant earlier work even when the wording differs.',
						'When the embedding service (Infinity) is reachable, recall uses cosine similarity; when it is down, recall degrades gracefully to Postgres full-text search (FTS). The chosen path is logged at startup and visible in `/api/metrics`.'
					],
					seenAt: ['/diagnostics/memory', 'recall_similar_turns API'],
					source: 'crates/mike/src/store/persistence/lessons.rs, crates/mike/src/config/mod.rs'
				},
				{
					slug: 'embeddings',
					term: 'Embeddings / Infinity',
					body: [
						'The vector representations that power semantic recall. Embeddings are computed out-of-process by an Infinity HTTP service running `nomic-embed-text-v1.5` (768-dimensional vectors), keeping the embedding load off this process. A background embed worker embeds each just-closed operator turn and each lesson during consolidation.',
						'The pipeline is enabled by default and settable from /settings (enable flag, Infinity base URL, model name). If Infinity is unreachable, recall falls back to Postgres FTS — embeddings are an accelerator, not a hard dependency.'
					],
					seenAt: ['/settings (embedding pipeline controls)', '/diagnostics/memory'],
					source: 'crates/mike/src/embed_worker.rs, crates/mike/src/settings/mod.rs'
				}
			]
		},
		{
			id: 'group-display-metaphors',
			title: 'Display metaphors',
			blurb: 'Visual representations used throughout to convey captured data at a glance.',
			entries: [
				{
					slug: 'sediment',
					term: 'Sediment layers',
					body: [
						"A visual representation of captured system-prompt compositions stacked like geological strata. Each horizontal layer is one distinct composition; its width encodes the composition's byte size so the largest sits at the bottom.",
						'Layers accumulate as Claude Code varies its system prompt across sessions, agents, and tools. Open a layer to inspect every system-prompt block, its cache-control marker, and which requests carried that composition.'
					],
					seenAt: ['/prompts (composition list)'],
					source: 'frontend/src/routes/prompts/+page.svelte'
				},
				{
					slug: 'fleet',
					term: 'Fleet',
					body: [
						'The collection of configured steerers available to watch live sessions. Each steerer in the fleet has a name, an authority level, and a drive mode. The fleet view shows which steerers are active and lets you assign them to sessions.',
						'The /fleet page shows a live status board and relationship graph of the session fleet; /steering?tab=fleet lists steerer enrolments and lets you edit or assign them.'
					],
					seenAt: ['/fleet (fleet board)', '/steering?tab=fleet'],
					source: 'frontend/src/routes/fleet/+page.svelte'
				}
			]
		}
	];

	const tocEntries = $derived(
		groups.flatMap((g) =>
			g.entries.map((e) => ({
				slug: e.slug,
				term: e.term,
				monoTitle: e.monoTitle,
				groupId: g.id,
				groupTitle: g.title
			}))
		)
	);

	const totalEntries = $derived(tocEntries.length);

	let steeringVocab = $state<SteeringVocabulary | null>(null);
	let steeringVocabError = $state(false);

	let searchQuery = $state('');
	const trimmedSearch = $derived(searchQuery.trim());
	const normalizedSearch = $derived(trimmedSearch.toLowerCase());
	const isSearching = $derived(trimmedSearch.length > 0);

	function clearSearch(): void {
		searchQuery = '';
	}

	function entryMatches(entry: Entry, q: string): boolean {
		if (entry.term.toLowerCase().includes(q)) return true;
		if (entry.monoTitle?.toLowerCase().includes(q)) return true;
		if (entry.body.some((paragraph) => paragraph.toLowerCase().includes(q))) return true;
		if (
			entry.subAnchors?.some(
				(sub) => sub.label.toLowerCase().includes(q) || sub.description.toLowerCase().includes(q)
			)
		)
			return true;
		return false;
	}

	function vocabItemMatches(item: SteeringVocabularyItem, q: string): boolean {
		return item.code.toLowerCase().includes(q) || item.description.toLowerCase().includes(q);
	}

	const filteredGroups = $derived.by(() => {
		if (!isSearching) return groups;
		const q = normalizedSearch;
		return groups
			.map((group) => ({
				...group,
				entries: group.entries.filter((entry) => entryMatches(entry, q))
			}))
			.filter((group) => group.entries.length > 0);
	});

	const visibleAuthorities = $derived.by(() => {
		if (!steeringVocab) return [];
		if (!isSearching) return steeringVocab.authorities;
		return steeringVocab.authorities.filter((item) => vocabItemMatches(item, normalizedSearch));
	});
	const visibleModes = $derived.by(() => {
		if (!steeringVocab) return [];
		if (!isSearching) return steeringVocab.modes;
		return steeringVocab.modes.filter((item) => vocabItemMatches(item, normalizedSearch));
	});
	const visibleVerdicts = $derived.by(() => {
		if (!steeringVocab) return [];
		if (!isSearching) return steeringVocab.verdicts;
		return steeringVocab.verdicts.filter((item) => vocabItemMatches(item, normalizedSearch));
	});
	const vocabMatchCount = $derived(
		visibleAuthorities.length + visibleModes.length + visibleVerdicts.length
	);
	const vocabDataCount = $derived(
		steeringVocab
			? steeringVocab.authorities.length +
					steeringVocab.modes.length +
					steeringVocab.verdicts.length
			: 0
	);

	const staticMatchCount = $derived(
		filteredGroups.reduce((sum, group) => sum + group.entries.length, 0)
	);
	const totalMatchCount = $derived(staticMatchCount + vocabMatchCount);
	const noMatchesAnywhere = $derived(isSearching && totalMatchCount === 0);

	onMount(() => {
		getSteeringVocabulary()
			.then((v) => {
				steeringVocab = v;
			})
			.catch(() => {
				steeringVocabError = true;
			});

		const hash = page.url.hash.replace(/^#/, '');
		if (!hash) return;
		const target = document.getElementById(hash);
		if (target) {
			requestAnimationFrame(() => {
				target.scrollIntoView({ behavior: 'instant', block: 'start' });
			});
		}
	});
</script>

<svelte:head>
	<title>Glossary — mike</title>
</svelte:head>

<section class="glossary-page" data-slot="glossary-page">
	<header class="page-head">
		<div class="head-inner">
			<h1 class="page-title">Glossary</h1>
			<p class="page-sub">
				The single source of truth for vocabulary in mike. Reference these entries in PR
				descriptions instead of redefining terms inline.
			</p>
			<div class="search-wrap" data-augmented-ui="tl-clip border">
				<Search class="search-icon" size={13} aria-hidden="true" />
				<input
					bind:value={searchQuery}
					type="search"
					class="search-input"
					placeholder="Search terms and definitions"
					autocomplete="off"
					autocapitalize="off"
					spellcheck="false"
					aria-label="Search terms and definitions"
				/>
				{#if isSearching}
					<button
						type="button"
						class="search-clear"
						aria-label="Clear search"
						onclick={clearSearch}
					>
						<X size={12} aria-hidden="true" />
					</button>
				{/if}
			</div>
			<p class="page-sub-meta">
				{#if isSearching}
					{totalMatchCount} of {totalEntries + (steeringVocab ? vocabDataCount : 0)} entries match "{trimmedSearch}"
				{:else}
					{totalEntries} entries across {groups.length} domains, plus the live steering vocabulary below.
				{/if}
			</p>
		</div>
	</header>

	<div class="glossary-body">
		<aside class="toc" aria-label="Glossary table of contents">
			<h2 class="toc-heading">Quick jump</h2>
			{#each filteredGroups as group (group.id)}
				<div class="toc-group">
					<a class="toc-group-link" href="#{group.id}">{group.title}</a>
					<ul class="toc-list">
						{#each group.entries as entry (entry.slug)}
							<li>
								<a class="toc-link" href="#{entry.slug}">
									<ChevronRight class="size-3 shrink-0 text-[var(--text-3)]" aria-hidden="true" />
									<span class="toc-link-text">
										{#if entry.monoTitle}
											<code class="font-mono">{entry.monoTitle}</code>
										{:else}
											{entry.term}
										{/if}
									</span>
								</a>
							</li>
						{/each}
					</ul>
				</div>
			{/each}
			{#if !isSearching || vocabMatchCount > 0}
				<div class="toc-group">
					<a class="toc-group-link" href="#group-steering-vocabulary">Steering vocabulary</a>
					<ul class="toc-list">
						{#if visibleAuthorities.length > 0}<li>
								<a class="toc-link" href="#vocab-authorities"
									><ChevronRight
										class="size-3 shrink-0 text-[var(--text-3)]"
										aria-hidden="true"
									/><span class="toc-link-text">Authorities</span></a
								>
							</li>{/if}
						{#if visibleModes.length > 0}<li>
								<a class="toc-link" href="#vocab-modes"
									><ChevronRight
										class="size-3 shrink-0 text-[var(--text-3)]"
										aria-hidden="true"
									/><span class="toc-link-text">Modes</span></a
								>
							</li>{/if}
						{#if visibleVerdicts.length > 0}<li>
								<a class="toc-link" href="#vocab-verdicts"
									><ChevronRight
										class="size-3 shrink-0 text-[var(--text-3)]"
										aria-hidden="true"
									/><span class="toc-link-text">Verdicts</span></a
								>
							</li>{/if}
					</ul>
				</div>
			{/if}
		</aside>

		<section class="entries" aria-label="Glossary entries">
			{#if noMatchesAnywhere}
				<FilteredEmptyState
					icon={SearchX}
					filtered={true}
					emptyText="No glossary terms yet"
					filteredText={`No terms match "${trimmedSearch}"`}
					filteredSubtext="Try a shorter word, or check the spelling."
					onClearFilters={clearSearch}
					clearLabel="Clear search"
				/>
			{/if}
			{#each filteredGroups as group (group.id)}
				<section
					class="group"
					id={group.id}
					aria-labelledby="{group.id}-title"
					data-slot="glossary-group"
				>
					<header class="group-head">
						<h2 id="{group.id}-title" class="group-title">{group.title}</h2>
						<p class="group-blurb">{group.blurb}</p>
					</header>
					<div class="group-entries">
						{#each group.entries as entry (entry.slug)}
							<article
								class="entry"
								data-slot="glossary-entry"
								data-augmented-ui="tl-clip br-clip border"
							>
								<h3 id={entry.slug} class="entry-title">
									{#if entry.monoTitle}
										<code class="font-mono entry-mono">{entry.monoTitle}</code>
										<span class="entry-alias">{entry.term}</span>
									{:else}
										<span>{entry.term}</span>
									{/if}
									<a
										class="anchor-link"
										href="#{entry.slug}"
										aria-label="Permalink to {entry.term}"
									>
										#
									</a>
								</h3>
								<div class="entry-body">
									{#each entry.body as paragraph, i (i)}
										<p>{paragraph}</p>
									{/each}
								</div>
								{#if entry.subAnchors && entry.subAnchors.length > 0}
									<dl class="sub-anchors" data-augmented-ui="tl-clip br-clip border">
										{#each entry.subAnchors as sub (sub.slug)}
											<div class="sub-anchor" id={sub.slug}>
												<dt class="sub-anchor-label">
													<code class="font-mono">{sub.label}</code>
													<a
														class="anchor-link sub-anchor-permalink"
														href="#{sub.slug}"
														aria-label="Permalink to {sub.label}"
													>
														#
													</a>
												</dt>
												<dd class="sub-anchor-desc">{sub.description}</dd>
											</div>
										{/each}
									</dl>
								{/if}
								<dl class="entry-meta">
									<div class="meta-row">
										<dt>Where you'll see it</dt>
										<dd>
											<ul class="seen-list">
												{#each entry.seenAt as where, i (i)}
													<li>
														<span class="seen-chip-backing">
															<span
																class="seen-chip font-mono"
																data-augmented-ui="tl-clip br-clip border">{where}</span
															>
														</span>
													</li>
												{/each}
											</ul>
										</dd>
									</div>
									{#if entry.source}
										<div class="meta-row">
											<dt>Source</dt>
											<dd><code class="font-mono">{entry.source}</code></dd>
										</div>
									{/if}
								</dl>
							</article>
						{/each}
					</div>
				</section>
			{/each}

			{#if !isSearching || steeringVocabError || steeringVocab === null || vocabMatchCount > 0}
				<section
					class="group"
					id="group-steering-vocabulary"
					aria-labelledby="group-steering-vocabulary-title"
					data-slot="glossary-group"
				>
					<header class="group-head">
						<h2 id="group-steering-vocabulary-title" class="group-title">
							Steering vocabulary
							<Tooltip
								text="Fetched from the running proxy on load, not a static definition like every entry above."
							>
								<span class="badge-nx badge-analyzed">live</span>
							</Tooltip>
						</h2>
						<p class="group-blurb">
							The live code definitions for authority, mode, and verdict — fetched live, so this
							page always matches the running binary.
						</p>
					</header>
					{#if steeringVocabError}
						<p class="vocab-error">
							Could not load steering vocabulary from the inspect API. Start the capture stack and
							reload this page to populate this section.
						</p>
					{:else if steeringVocab === null}
						<p class="vocab-loading" aria-live="polite">Loading vocabulary</p>
					{:else}
						<div class="group-entries">
							<article class="entry" data-slot="glossary-entry">
								<h3 id="vocab-authorities" class="entry-title">
									<span>Authority</span>
									<a
										class="anchor-link"
										href="#vocab-authorities"
										aria-label="Permalink to Authority"
									>
										#
									</a>
								</h3>
								<div class="entry-body">
									<p>
										How forceful a steering intervention is. Controls the trust level the session's
										Claude assigns to the steered message.
									</p>
								</div>
								<dl class="sub-anchors">
									{#each visibleAuthorities as item (item.code)}
										<div class="sub-anchor" id="vocab-authority-{item.code}">
											<dt class="sub-anchor-label">
												<code class="font-mono">{item.code}</code>
												<a
													class="anchor-link sub-anchor-permalink"
													href="#vocab-authority-{item.code}"
													aria-label="Permalink to {item.code}"
												>
													#
												</a>
											</dt>
											<dd class="sub-anchor-desc">{item.description}</dd>
										</div>
									{/each}
								</dl>
							</article>

							<article class="entry" data-slot="glossary-entry">
								<h3 id="vocab-modes" class="entry-title">
									<span>Mode</span>
									<a class="anchor-link" href="#vocab-modes" aria-label="Permalink to Mode"> # </a>
								</h3>
								<div class="entry-body">
									<p>
										How much the steerer takes over the session when it fires. Controls how the
										steered message is delivered.
									</p>
								</div>
								<dl class="sub-anchors">
									{#each visibleModes as item (item.code)}
										<div class="sub-anchor" id="vocab-mode-{item.code}">
											<dt class="sub-anchor-label">
												<code class="font-mono">{item.code}</code>
												<a
													class="anchor-link sub-anchor-permalink"
													href="#vocab-mode-{item.code}"
													aria-label="Permalink to {item.code}"
												>
													#
												</a>
											</dt>
											<dd class="sub-anchor-desc">{item.description}</dd>
										</div>
									{/each}
								</dl>
							</article>

							<article class="entry" data-slot="glossary-entry">
								<h3 id="vocab-verdicts" class="entry-title">
									<span>Verdict</span>
									<a class="anchor-link" href="#vocab-verdicts" aria-label="Permalink to Verdict">
										#
									</a>
								</h3>
								<div class="entry-body">
									<p>
										The outcome of a single steerer evaluation cycle. Every turn the steerer runs
										produces one verdict.
									</p>
								</div>
								<dl class="sub-anchors">
									{#each visibleVerdicts as item (item.code)}
										<div class="sub-anchor" id="vocab-verdict-{item.code}">
											<dt class="sub-anchor-label">
												<code class="font-mono">{item.code}</code>
												<a
													class="anchor-link sub-anchor-permalink"
													href="#vocab-verdict-{item.code}"
													aria-label="Permalink to {item.code}"
												>
													#
												</a>
											</dt>
											<dd class="sub-anchor-desc">{item.description}</dd>
										</div>
									{/each}
								</dl>
							</article>
						</div>
					{/if}
				</section>
			{/if}
		</section>
	</div>
</section>

<style>
	.glossary-page {
		display: flex;
		flex-direction: column;
		min-height: 0;
		overflow-y: auto;
		padding: 24px 32px 48px;
	}
	.page-head {
		border-bottom: 1px solid var(--border-subtle);
		padding-bottom: 18px;
		margin-bottom: 24px;
	}
	.head-inner {
		display: flex;
		flex-direction: column;
		gap: 6px;
		max-width: 760px;
	}
	.page-title {
		margin: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-size: 1.5rem;
		font-weight: 700;
		color: var(--text-strong);
		letter-spacing: -0.02em;
	}
	.page-sub {
		margin: 0;
		font-size: 0.875rem;
		color: var(--text-1);
		line-height: 1.5;
	}
	.page-sub-meta {
		margin: 0;
		font-size: 0.75rem;
		color: var(--text-3);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}

	.glossary-body {
		display: grid;
		grid-template-columns: 220px 1fr;
		gap: 32px;
		align-items: start;
	}

	.toc {
		position: sticky;
		top: 0;
		display: flex;
		flex-direction: column;
		gap: 14px;
		padding-right: 8px;
		max-height: calc(100vh - 120px);
		overflow-y: auto;
		font-size: 0.75rem;
	}
	.toc-heading {
		margin: 0;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 0.6875rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}
	.toc-group {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.toc-group-link {
		font-size: 0.75rem;
		font-weight: 600;
		color: var(--text-strong);
		text-decoration: none;
		padding: 4px 0;
		border-bottom: 1px solid var(--border-subtle);
	}
	.toc-group-link:hover {
		color: var(--interactive-active-fg);
	}
	.toc-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 1px;
	}
	.toc-link {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 3px 4px;
		color: var(--text-2);
		text-decoration: none;
		border-radius: 2px;
		font-size: 0.75rem;
		line-height: 1.35;
	}
	.toc-link:hover {
		background: var(--surface-2);
		color: var(--interactive-active-fg);
	}
	.toc-link:focus-visible {
		outline: none;
		background: var(--surface-2);
		box-shadow: 0 0 0 1px var(--interactive-focus-ring);
	}
	.toc-link-text code {
		font-size: 0.6875rem;
	}

	.entries {
		display: flex;
		flex-direction: column;
		gap: 36px;
		min-width: 0;
		max-width: 800px;
	}
	.group {
		display: flex;
		flex-direction: column;
		gap: 18px;
	}
	.group-head {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding-bottom: 10px;
		border-bottom: 1px solid var(--border-default);
	}
	.group-title {
		margin: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-size: 1.125rem;
		font-weight: 700;
		color: var(--text-strong);
		letter-spacing: -0.01em;
	}
	.group-blurb {
		margin: 0;
		font-size: 0.8125rem;
		color: var(--text-2);
		line-height: 1.5;
	}
	.group-entries {
		display: flex;
		flex-direction: column;
		gap: 24px;
	}
	.entry {
		--edge: 8px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--status-info-fg) 0%,
			var(--status-info-border) 45%,
			var(--border-default) 80%
		);
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: calc(var(--edge) + 8px) calc(var(--edge) + 10px);
		background: var(--surface-card);
		scroll-margin-top: 16px;
	}
	.entry-title {
		display: flex;
		align-items: baseline;
		gap: 8px;
		margin: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-size: 1rem;
		font-weight: 700;
		color: var(--text-strong);
		letter-spacing: -0.01em;
		flex-wrap: wrap;
	}
	.entry-mono {
		font-size: 0.9375rem;
		color: var(--interactive-active-fg);
	}
	.entry-alias {
		font-weight: 400;
		font-size: 0.8125rem;
		color: var(--text-3);
	}
	.anchor-link {
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 0.75rem;
		color: var(--text-3);
		text-decoration: none;
		opacity: 0;
		transition:
			opacity 0.14s,
			color 0.14s;
	}
	.entry:hover .anchor-link,
	.anchor-link:focus-visible {
		opacity: 1;
		color: var(--interactive-active-fg);
	}
	.anchor-link:focus-visible {
		outline: none;
	}
	.entry-body {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.entry-body p {
		margin: 0;
		font-size: 0.875rem;
		color: var(--text-1);
		line-height: 1.55;
	}
	.entry-body p :global(code) {
		padding: 1px 4px;
		background: var(--surface-2);
		font-size: 0.8125rem;
		color: var(--text-strong);
	}

	.sub-anchors {
		--edge: 6px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		margin: 4px 0 0;
		padding: calc(var(--edge) + 6px) calc(var(--edge) + 8px);
		display: flex;
		flex-direction: column;
		gap: 10px;
		background: var(--surface-2);
	}
	.sub-anchor {
		display: flex;
		flex-direction: column;
		gap: 3px;
		scroll-margin-top: 16px;
	}
	.sub-anchor-label {
		display: flex;
		align-items: center;
		gap: 6px;
		margin: 0;
		font-weight: 600;
		font-size: 0.8125rem;
		color: var(--text-strong);
	}
	.sub-anchor-label code {
		color: var(--interactive-active-fg);
	}
	.sub-anchor-permalink {
		font-size: 0.6875rem;
	}
	.sub-anchor-desc {
		margin: 0;
		font-size: 0.8125rem;
		color: var(--text-1);
		line-height: 1.5;
	}

	.entry-meta {
		display: flex;
		flex-direction: column;
		gap: 6px;
		margin: 0;
		padding-top: 10px;
		border-top: 1px solid var(--border-subtle);
	}
	.meta-row {
		display: grid;
		grid-template-columns: 140px 1fr;
		gap: 8px;
		align-items: baseline;
	}
	.meta-row dt {
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--text-3);
	}
	.meta-row dd {
		margin: 0;
		font-size: 0.75rem;
		color: var(--text-2);
	}
	.meta-row dd code {
		font-size: 0.75rem;
		color: var(--text-1);
	}
	.seen-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.seen-chip-backing {
		display: inline-flex;
		background: var(--surface-card);
	}
	.seen-chip {
		--aug-tl: 4px;
		--aug-br: 4px;
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		display: inline-block;
		padding: 1px 6px;
		font-size: 0.75rem;
		color: var(--text-1);
		background: linear-gradient(0deg, var(--surface-2), var(--surface-2)), var(--surface-card);
	}

	.vocab-loading,
	.vocab-error {
		margin: 8px 0;
		font-family: var(--font-mono, monospace);
		font-size: 0.8125rem;
		line-height: 1.5;
	}
	.vocab-loading {
		color: var(--text-3);
	}
	.vocab-error {
		color: var(--status-warning-fg);
	}

	@media (max-width: 960px) {
		.glossary-body {
			grid-template-columns: 1fr;
			gap: 18px;
		}
		.toc {
			position: static;
			max-height: none;
		}
		.meta-row {
			grid-template-columns: 1fr;
			gap: 2px;
		}
	}

	.search-wrap {
		position: relative;
		display: flex;
		align-items: center;
		height: var(--control-h-lg, 36px);
		max-width: 22rem;
		background: var(--surface-2);
		--aug-tl: var(--edge-sm);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		transition: filter var(--dur-1, 120ms) var(--ease-io, ease);
		filter: drop-shadow(0 0 0 transparent) drop-shadow(0 0 0 transparent);
	}
	.search-wrap:hover {
		--aug-border-bg: var(--border-strong);
	}
	.search-wrap:focus-within {
		--aug-border-bg: var(--interactive-focus-ring);
		filter: drop-shadow(0 0 2px var(--interactive-focus-ring))
			drop-shadow(0 0 12px var(--interactive-focus-ring));
	}
	.search-wrap :global(.search-icon) {
		position: absolute;
		left: 9px;
		color: var(--text-faint);
		pointer-events: none;
	}
	.search-input {
		width: 100%;
		min-width: 0;
		height: 100%;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 0.8125rem;
		color: var(--text-1);
		background: transparent;
		border: none;
		border-radius: 0;
		padding: 0 28px 0 28px;
		outline: none;
	}
	.search-input::placeholder {
		color: var(--text-faint);
	}
	.search-input::-webkit-search-cancel-button,
	.search-input::-webkit-search-decoration {
		-webkit-appearance: none;
	}
	.search-clear {
		position: absolute;
		right: 6px;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 18px;
		height: 18px;
		padding: 0;
		background: transparent;
		border: none;
		border-radius: 2px;
		color: var(--text-faint);
		cursor: pointer;
		transition:
			color var(--dur-1, 120ms) var(--ease-snap, ease),
			background var(--dur-1, 120ms) var(--ease-snap, ease);
	}
	.search-clear:hover {
		color: var(--text-strong);
		background: var(--surface-3);
	}
	.search-clear:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 1px;
	}
</style>
