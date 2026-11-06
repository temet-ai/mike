import type {
	BodyPayload,
	DeltaRowDto,
	RequestDeltasPayload,
	RequestSummary,
	ToolUseDto,
	ToolUsesResponse,
	OperatorTurnDto,
	SessionDetail,
	SessionSummary
} from '$lib/types';
import { buildRequestDetail } from './request';

function turnPair(turn: number): unknown[] {
	const toolId = `toolu_story_${turn.toString().padStart(3, '0')}`;
	return [
		{
			role: 'user',
			content: [
				{
					type: 'text',
					text:
						`Turn ${turn}: walk me through how the inspect bind reconstructs the ` +
						`accumulated SSE text for request lineage so I can verify the band ` +
						`timeline renders the full conversation without dropping any turn.`
				}
			]
		},
		{
			role: 'assistant',
			content: [
				{
					type: 'thinking',
					thinking:
						`The operator is asking about turn ${turn}. The accumulated text is ` +
						`rebuilt from the persisted content_block_delta rows in seq_ord ` +
						`order; I should read the relevant store module before answering so ` +
						`the explanation matches the code rather than my prior assumptions.`
				},
				{
					type: 'tool_use',
					id: toolId,
					name: 'Read',
					input: {
						file_path: 'crates/mike/src/store/models.rs',
						offset: turn * 8,
						limit: 60
					}
				},
				{
					type: 'text',
					text:
						`For turn ${turn}: the accumulated text lives in the SSE accumulator ` +
						`fields on the request row, fed by the channel-based tee. Each ` +
						`content_block_delta is appended in arrival order, so the band ` +
						`timeline can replay the exact streamed sequence — message_start ` +
						`through message_stop — with byte-identity. Nothing is summarised at ` +
						`capture time; the projection clones the full text rather than ` +
						`slicing it, which is why this band shows the complete answer.`
				}
			]
		},
		{
			role: 'user',
			content: [
				{
					type: 'tool_result',
					tool_use_id: toolId,
					content: [
						{
							type: 'text',
							text:
								`pub struct StreamBlockDto {\n    pub seq_ord: i32,\n    pub ` +
								`content: String,\n    pub byte_size: i32,\n    pub ` +
								`content_sha256: String,\n    pub block_type: String, ` +
								`// 'text' | 'thinking' | 'tool_use'\n    pub signature: ` +
								`Option<String>,\n    pub wire_tool_use_id: Option<String>,\n}` +
								`\n// (turn ${turn} excerpt — full row shape, no truncation)`
						}
					]
				}
			]
		}
	];
}

function buildRequestBodyJson(turns = 256): string {
	const messages: unknown[] = [];
	for (let t = 1; t <= turns; t++) messages.push(...turnPair(t));
	return JSON.stringify({
		model: 'claude-opus-4-7',
		max_tokens: 8096,
		messages
	});
}

function buildResponseBodyJson(): string {
	return JSON.stringify({
		id: 'msg_story_band_01',
		type: 'message',
		role: 'assistant',
		model: 'claude-opus-4-7',
		content: [
			{
				type: 'thinking',
				thinking:
					'Reconstructing the response from the buffered non-SSE body. The ' +
					'band renders this assistant turn from response_body.content[] once ' +
					'detail is hydrated; the full block text passes through verbatim.'
			},
			{
				type: 'tool_use',
				id: 'toolu_story_resp_01',
				name: 'Grep',
				input: { pattern: 'accumulated_text', path: 'crates/mike/src' }
			},
			{
				type: 'text',
				text:
					'The response body is buffered for non-SSE replies and walked into a ' +
					'single assistant Message by walkResponseContent. This fixture is the ' +
					'full captured reply — every block is rendered in its entirety so the ' +
					'band timeline reflects exactly what the upstream returned.'
			}
		],
		stop_reason: 'end_turn',
		usage: { input_tokens: 1500, output_tokens: 480 }
	});
}

const REQUEST_BODY_JSON = buildRequestBodyJson();
const SENT_BODY_JSON = buildRequestBodyJson(8);
const RESPONSE_BODY_JSON = buildResponseBodyJson();

function payload(text: string): BodyPayload {
	return {
		text,
		byte_size: new TextEncoder().encode(text).length,
		has_body: true
	};
}

const REQUEST_PAYLOAD = payload(REQUEST_BODY_JSON);
const SENT_PAYLOAD = payload(SENT_BODY_JSON);
const RESPONSE_PAYLOAD = payload(RESPONSE_BODY_JSON);

function buildDeltasPayload(requestId: number): RequestDeltasPayload {
	const lastTurn = 256;
	const lastTurnMessages = turnPair(lastTurn);
	const baseOrd = (lastTurn - 1) * 3;

	const deltas: DeltaRowDto[] = lastTurnMessages.map((message, i) => {
		const body = JSON.stringify(message);
		const role = (message as { role: 'user' | 'assistant' }).role;
		return {
			change_kind: 'added',
			ord: baseOrd + i,
			message_id: 900_000 + baseOrd + i,
			message_sha256: `story-delta-${requestId}-${baseOrd + i}`,
			role,
			byte_size: new TextEncoder().encode(body).length,
			body: message
		};
	});

	return {
		request_id: requestId,
		predecessor_request_id: requestId - 1,
		deltas
	};
}

export function stubBandBodyFetch(): void {
	globalThis.fetch = async (input: RequestInfo | URL) => {
		const url = typeof input === 'string' ? input : input.toString();
		const deltasMatch = url.match(/\/api\/sessions\/[^/]+\/requests\/(\d+)\/deltas(?:[?#]|$)/);
		if (deltasMatch) {
			const id = Number(deltasMatch[1]);
			return new Response(JSON.stringify(buildDeltasPayload(id)), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			});
		}
		const m = url.match(/\/api\/requests\/\d+\/body\/(request_sent|request|response)/);
		if (m) {
			const kind = m[1];
			const body =
				kind === 'request'
					? REQUEST_PAYLOAD
					: kind === 'request_sent'
						? SENT_PAYLOAD
						: RESPONSE_PAYLOAD;
			return new Response(JSON.stringify(body), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			});
		}
		const meta = url.match(/\/api\/requests\/(\d+)(?:[?#]|$)/);
		if (meta) {
			const id = Number(meta[1]);
			const detail = buildRequestDetail({ id, session_id: 'storybook-session-1' });
			return new Response(JSON.stringify(detail), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			});
		}
		return new Response('Not Found', { status: 404, statusText: 'Not Found' });
	};
}

function buildKickoffBodyJson(userPrompt: string): string {
	return JSON.stringify({
		model: 'claude-sonnet-4-6',
		max_tokens: 32000,
		system: [
			{
				type: 'text',
				text:
					'<claude_info>\nThe assistant is Claude, made by Anthropic. Claude Code is ' +
					"Claude's official agentic coding tool for software engineering tasks.\n</claude_info>"
			},
			{
				type: 'text',
				text:
					'# Claude Proxy — Project Instructions\n\n## Project Overview\n\n' +
					'A Rust reverse proxy for Claude API traffic. Three ports, two Rust binaries, ' +
					'one SPA process:\n\n- **Proxy bind** `:9988` — `/v1/*` only.\n' +
					'- **Inspect bind** `:9989` — `/api/*` REST + `/api/ws` WebSocket.\n' +
					'- **SPA host** `:9990` — independent `vite dev` process.\n\n' +
					'Build: `cargo build --all` | Test: `cargo nextest run --all`\n' +
					'Run: `./scripts/start-proxy.sh`\nWeb UI: http://localhost:9990/'
			},
			{
				type: 'text',
				text:
					"# Output discipline\n\nAnswer once. Do not restate a heading's point in " +
					'the prose under it, do not recap at the end what you already said above. ' +
					'State each point a single time and stop.\n\n# Tone and style\n\n' +
					'Only use emojis if the user explicitly requests it. Your responses should ' +
					'be short and concise.'
			}
		],
		messages: [
			{
				role: 'user',
				content: [
					{
						type: 'text',
						text: userPrompt
					}
				]
			}
		]
	});
}

function buildToolUsesResponse(sessionId: string): ToolUsesResponse {
	const rows: ToolUseDto[] = [
		{
			request_id: 43028,
			session_id: sessionId,
			created_at: '2026-06-19T13:23:21Z',
			tool_id: 'toolu_01BashExplore',
			tool_name: 'Bash',
			input: {
				command: 'find frontend/src -name "*.stories.svelte" | head -20',
				description: 'List story files'
			},
			file_path: null,
			ord: 0
		},
		{
			request_id: 43030,
			session_id: sessionId,
			created_at: '2026-06-19T13:23:33Z',
			tool_id: 'toolu_01ReadMain',
			tool_name: 'Read',
			input: { file_path: 'frontend/.storybook/main.ts' },
			file_path: 'frontend/.storybook/main.ts',
			ord: 0
		},
		{
			request_id: 43031,
			session_id: sessionId,
			created_at: '2026-06-19T13:23:43Z',
			tool_id: 'toolu_01EditMain',
			tool_name: 'Edit',
			input: {
				file_path: 'frontend/.storybook/main.ts',
				old_string: '@storybook/addon-console',
				new_string: ''
			},
			file_path: 'frontend/.storybook/main.ts',
			ord: 0
		},
		{
			request_id: 43032,
			session_id: sessionId,
			created_at: '2026-06-19T13:23:50Z',
			tool_id: 'toolu_01AgentFrontend',
			tool_name: 'Agent',
			input: {
				subagent_type: 'frontend-developer',
				description: 'Update Storybook stories with realistic session fixture data'
			},
			file_path: null,
			ord: 0
		},
		{
			request_id: 43033,
			session_id: sessionId,
			created_at: '2026-06-19T13:23:56Z',
			tool_id: 'toolu_01EditStories',
			tool_name: 'Edit',
			input: {
				file_path: 'frontend/src/routes/sessions/[sessionId]/page.stories.svelte',
				old_string: 'request_count: 208',
				new_string: 'request_count: 332'
			},
			file_path: 'frontend/src/routes/sessions/[sessionId]/page.stories.svelte',
			ord: 0
		},
		{
			request_id: 43035,
			session_id: sessionId,
			created_at: '2026-06-19T13:24:08Z',
			tool_id: 'toolu_01WriteBandBodies',
			tool_name: 'Edit',
			input: {
				file_path: 'frontend/src/lib/components/__fixtures__/bandBodies.ts',
				old_string: 'import type { BodyPayload',
				new_string: 'import type { BodyPayload, ToolUsesResponse'
			},
			file_path: 'frontend/src/lib/components/__fixtures__/bandBodies.ts',
			ord: 0
		},
		{
			request_id: 43040,
			session_id: sessionId,
			created_at: '2026-06-19T13:24:45Z',
			tool_id: 'toolu_01AgentUIDesigner',
			tool_name: 'Agent',
			input: {
				subagent_type: 'ui-designer',
				description: 'Review SessionOverview story visual accuracy against live page'
			},
			file_path: null,
			ord: 0
		}
	];
	return { session_id: sessionId, total_count: rows.length, offset: 0, limit: 1000, rows };
}

export function buildOperatorTurnsResponse(sessionId: string): OperatorTurnDto[] {
	return [
		{
			turn_id: 1,
			session_id: sessionId,
			project_id: 1,
			start_request_id: 47362,
			end_request_id: 47404,
			start_request_number: 4,
			end_request_number: 15,
			user_prompt:
				'The storybook that we have running. the tree structure on the side. ' +
				'its still not good enough. it seems when we cleaned it up, there were ' +
				'a lot of old components that are still in the wrong location',
			final_assistant_text:
				"I've investigated the Storybook tree structure and updated the main.ts " +
				'addons list, removing the incompatible @storybook/addon-console. The ' +
				'TypicalSession story now includes all request types from the real session ' +
				'including quota_probe and title_sidecar housekeeping bands.',
			tool_call_count: 12,
			tool_error_count: 0,
			files_touched: [
				'frontend/.storybook/main.ts',
				'frontend/src/routes/sessions/[sessionId]/page.stories.svelte',
				'frontend/src/lib/components/__fixtures__/bandBodies.ts'
			],
			tools_used: ['Bash', 'Read', 'Edit', 'Agent'],
			agents_dispatched: [
				{ subagent_type: 'frontend-developer', count: 1 },
				{ subagent_type: 'ui-designer', count: 1 }
			],
			weasel_findings_count: 0,
			total_input_tokens: 48200,
			total_output_tokens: 4150,
			duration_ms: 148000,
			outcome: 'accepted',
			outcome_evidence: null,
			classified_at: '2026-06-19T13:25:49Z',
			created_at: '2026-06-19T13:23:21Z',
			updated_at: '2026-06-19T13:25:49Z'
		}
	];
}

export function stubSessionOverviewFetch(opts: {
	sessionId: string;
	requests: readonly RequestSummary[];
	kickoffRequestId: number;
	kickoffUserPrompt: string;
	summary: SessionSummary;
	toolUses?: ToolUsesResponse;
	operatorTurns?: OperatorTurnDto[];
}): void {
	const kickoffBody = payload(buildKickoffBodyJson(opts.kickoffUserPrompt));
	const toolUses = opts.toolUses ?? buildToolUsesResponse(opts.sessionId);
	const operatorTurns = opts.operatorTurns ?? buildOperatorTurnsResponse(opts.sessionId);

	const sessionDetail: SessionDetail = {
		summary: opts.summary,
		requests: opts.requests as RequestSummary[],
		total_requests: opts.summary.request_count,
		offset: 0,
		limit: 20
	};

	globalThis.fetch = async (input: RequestInfo | URL) => {
		const url = typeof input === 'string' ? input : input.toString();

		const deltasMatch = url.match(/\/api\/sessions\/[^/]+\/requests\/(\d+)\/deltas(?:[?#]|$)/);
		if (deltasMatch) {
			const id = Number(deltasMatch[1]);
			return new Response(JSON.stringify(buildDeltasPayload(id)), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			});
		}

		if (url.includes(`/api/sessions/${opts.sessionId}`)) {
			if (url.includes('/tool_uses') || url.includes('/tool-uses')) {
				return new Response(JSON.stringify(toolUses), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.includes('/operator-turns')) {
				return new Response(JSON.stringify(operatorTurns), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.includes('/steering')) {
				return new Response(JSON.stringify({ rows: [], total_count: 0, offset: 0, limit: 200 }), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.includes('/enrolments')) {
				return new Response(JSON.stringify([]), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.includes('/filter-rules') || url.includes('/analyzers')) {
				return new Response(JSON.stringify([]), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response(JSON.stringify(sessionDetail), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			});
		}

		const bodyMatch = url.match(/\/api\/requests\/(\d+)\/body\/(request_sent|request|response)/);
		if (bodyMatch) {
			const id = Number(bodyMatch[1]);
			const kind = bodyMatch[2];
			if (kind !== 'response' && id === opts.kickoffRequestId) {
				return new Response(JSON.stringify(kickoffBody), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			const body =
				kind === 'request'
					? REQUEST_PAYLOAD
					: kind === 'request_sent'
						? SENT_PAYLOAD
						: RESPONSE_PAYLOAD;
			return new Response(JSON.stringify(body), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			});
		}
		const meta = url.match(/\/api\/requests\/(\d+)(?:[?#]|$)/);
		if (meta) {
			const id = Number(meta[1]);
			const detail = buildRequestDetail({ id, session_id: opts.sessionId });
			return new Response(JSON.stringify(detail), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			});
		}

		return new Response('Not Found', { status: 404, statusText: 'Not Found' });
	};
}
