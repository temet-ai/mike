import { ApiError } from '$lib/api';

/**
 * Client for the prompt-assistant service (spec section 3), reached at `/api/assistant/*`
 * through the same vite proxy as the rest of the inspect API. The message endpoint is a
 * POST that streams `text/event-stream`, so this reads it as a `fetch` + `ReadableStream`
 * rather than `EventSource` (which cannot POST).
 */

export interface AssistantHealth {
	ok: boolean;
	enabled: boolean;
	model: string;
	effort: 'low' | 'medium' | 'high';
	may_apply: 'suggest' | 'apply';
	spent_today_usd: number;
	daily_budget_usd: number;
	auth_mode: 'claude_code_login' | 'api_key';
	credentials_available: boolean;
	provider: 'anthropic' | 'codex';
}

export type AssistantProposalAction = 'replace' | 'append' | 'drop';

/**
 * `edit_id` is the filter rule's own `id` (a stringified bigint, same string
 * `FilterRuleDto.id` already carries) — a string, not a number. Verified against
 * `services/prompt-assistant/src/types.ts`'s `Proposal.edit_id: string | null` and
 * `mcp/tools.ts`'s `propose_edit` zod schema (`edit_id: z.string().nullable().optional()`),
 * which passes it straight to `updateFilterRule(args.edit_id, ...)` with no conversion.
 */
export interface AssistantProposal {
	id: string;
	title: string;
	edit_id: string | null;
	action: AssistantProposalAction;
	match: string;
	before: string;
	after: string;
	reach: string;
	/**
	 * The raw rule text to SAVE — added alongside `match`/`action` so the client never has to
	 * reconstruct it from `after` (which, for `append`, is `match + text` and duplicates the
	 * anchor when sent straight to `updateFilterRule`/`createFilterRule` as `text`). `after`
	 * stays display-only. Optional/nullable for graceful degradation against an older service
	 * that hasn't shipped the field yet, and legitimately `null` for a `drop` proposal (nothing
	 * to save as body text).
	 */
	rule_text?: string | null;
	/** When set, the proposal is scoped to this project's canonical path rather than the shared global prompt. */
	project_path?: string | null;
}

/** What the assistant checks a message against — sent as `scope` on every `sendAssistantMessage` call. */
export type AssistantScope = 'project' | 'all';

/**
 * Mirrors `services/prompt-assistant/src/types.ts`'s `ConversationMessage`. One message can
 * carry MULTIPLE proposals (the model may call `propose_edit` more than once in a turn) —
 * `proposals`, plural, never a single `proposal`.
 */
export interface AssistantMessageDto {
	id: string;
	role: 'user' | 'assistant';
	text: string;
	quote?: string;
	proposals?: AssistantProposal[];
	cost_usd?: number;
	created_at: string;
}

export interface AssistantConversation {
	id: string;
	request_id: number;
	created_at: string;
	updated_at: string;
	messages: AssistantMessageDto[];
}

export type AssistantStreamEvent =
	| { type: 'text'; delta: string }
	| { type: 'proposal'; proposal: AssistantProposal }
	| { type: 'status'; phase: string }
	| { type: 'error'; message: string }
	| { type: 'done'; cost_usd: number }
	| { type: 'applied'; proposal_id: string; rule_id: string };

async function getJson<T>(path: string): Promise<T> {
	const res = await fetch(path, { headers: { accept: 'application/json' } });
	if (!res.ok) {
		throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	return (await res.json()) as T;
}

async function postJson<Req, Res>(path: string, body: Req): Promise<Res> {
	const res = await fetch(path, {
		method: 'POST',
		headers: { accept: 'application/json', 'content-type': 'application/json' },
		body: JSON.stringify(body)
	});
	if (!res.ok) {
		let message = `POST ${path} failed: ${res.status} ${res.statusText}`;
		try {
			const errBody = (await res.json()) as { error?: string };
			if (errBody && typeof errBody.error === 'string' && errBody.error.length > 0) {
				message = errBody.error;
			}
		} catch {}
		throw new ApiError(message, res.status, path);
	}
	return (await res.json()) as Res;
}

export function getAssistantHealth(): Promise<AssistantHealth> {
	return getJson<AssistantHealth>('/api/assistant/health');
}

export function createAssistantConversation(requestId: number): Promise<{ id: string }> {
	return postJson<{ request_id: number }, { id: string }>('/api/assistant/conversations', {
		request_id: requestId
	});
}

export function getAssistantConversation(id: string): Promise<AssistantConversation> {
	return getJson<AssistantConversation>(`/api/assistant/conversations/${encodeURIComponent(id)}`);
}

export interface SseFrame {
	event: string;
	data: string;
}

/**
 * Incremental `text/event-stream` frame reader. Fed raw text chunks as they arrive over
 * the wire; emits complete `{event, data}` frames as soon as a blank-line frame boundary
 * closes them. Kept separate from `fetch` so it can be unit-tested with plain strings.
 */
export class SseFrameParser {
	private buffer = '';

	push(chunk: string): SseFrame[] {
		this.buffer += chunk;
		const frames: SseFrame[] = [];
		let boundary = this.buffer.indexOf('\n\n');
		while (boundary !== -1) {
			const raw = this.buffer.slice(0, boundary);
			this.buffer = this.buffer.slice(boundary + 2);
			const frame = parseSseFrame(raw);
			if (frame !== null) frames.push(frame);
			boundary = this.buffer.indexOf('\n\n');
		}
		return frames;
	}
}

function parseSseFrame(raw: string): SseFrame | null {
	let event = 'message';
	const dataLines: string[] = [];
	for (const line of raw.split('\n')) {
		if (line.startsWith('event:')) {
			event = line.slice('event:'.length).trim();
		} else if (line.startsWith('data:')) {
			dataLines.push(line.slice('data:'.length).replace(/^ /, ''));
		}
	}
	if (dataLines.length === 0) return null;
	return { event, data: dataLines.join('\n') };
}

/** Maps one parsed SSE frame to the typed assistant event union, or null if unrecognised. */
export function parseAssistantStreamFrame(frame: SseFrame): AssistantStreamEvent | null {
	let payload: unknown;
	try {
		payload = JSON.parse(frame.data);
	} catch {
		return null;
	}
	if (typeof payload !== 'object' || payload === null) return null;
	const p = payload as Record<string, unknown>;

	switch (frame.event) {
		case 'text':
			return typeof p.delta === 'string' ? { type: 'text', delta: p.delta } : null;
		case 'proposal':
			return isAssistantProposal(p) ? { type: 'proposal', proposal: toAssistantProposal(p) } : null;
		case 'status':
			return typeof p.phase === 'string' ? { type: 'status', phase: p.phase } : null;
		case 'error':
			return typeof p.message === 'string' ? { type: 'error', message: p.message } : null;
		case 'done':
			return typeof p.cost_usd === 'number' ? { type: 'done', cost_usd: p.cost_usd } : null;
		case 'applied':
			return typeof p.proposal_id === 'string' && typeof p.rule_id === 'string'
				? { type: 'applied', proposal_id: p.proposal_id, rule_id: p.rule_id }
				: null;
		default:
			return null;
	}
}

function isAssistantProposal(p: Record<string, unknown>): boolean {
	return (
		typeof p.id === 'string' &&
		typeof p.title === 'string' &&
		(p.edit_id === null || typeof p.edit_id === 'string') &&
		(p.action === 'replace' || p.action === 'append' || p.action === 'drop') &&
		typeof p.match === 'string' &&
		typeof p.before === 'string' &&
		typeof p.after === 'string' &&
		typeof p.reach === 'string' &&
		(p.rule_text === undefined || p.rule_text === null || typeof p.rule_text === 'string') &&
		(p.project_path === undefined || p.project_path === null || typeof p.project_path === 'string')
	);
}

/** Only called after `isAssistantProposal` has confirmed every field's shape. */
function toAssistantProposal(p: Record<string, unknown>): AssistantProposal {
	return {
		id: p.id as string,
		title: p.title as string,
		edit_id: p.edit_id as string | null,
		action: p.action as AssistantProposalAction,
		match: p.match as string,
		before: p.before as string,
		after: p.after as string,
		reach: p.reach as string,
		rule_text: (p.rule_text as string | null | undefined) ?? undefined,
		project_path: (p.project_path as string | null | undefined) ?? undefined
	};
}

export interface SendAssistantMessageOptions {
	signal?: AbortSignal;
}

/**
 * Sends one message to a conversation and streams the response, invoking `onEvent` for
 * each recognised SSE frame as it arrives. Resolves once the stream ends (on `done`,
 * `error`, or the connection closing) — it does not throw on an `error` event, since
 * that is a normal in-band assistant response, not a transport failure.
 */
export async function sendAssistantMessage(
	conversationId: string,
	body: {
		text: string;
		quote?: string;
		edit_id?: string;
		scope?: AssistantScope;
		project_path?: string;
	},
	onEvent: (event: AssistantStreamEvent) => void,
	options: SendAssistantMessageOptions = {}
): Promise<void> {
	const path = `/api/assistant/conversations/${encodeURIComponent(conversationId)}/messages`;
	const res = await fetch(path, {
		method: 'POST',
		headers: { accept: 'text/event-stream', 'content-type': 'application/json' },
		body: JSON.stringify(body),
		signal: options.signal
	});
	if (!res.ok || res.body === null) {
		let message = `POST ${path} failed: ${res.status} ${res.statusText}`;
		try {
			const errBody = (await res.json()) as { error?: string };
			if (errBody && typeof errBody.error === 'string' && errBody.error.length > 0) {
				message = errBody.error;
			}
		} catch {}
		throw new ApiError(message, res.status, path);
	}

	const reader = res.body.getReader();
	const decoder = new TextDecoder();
	const parser = new SseFrameParser();
	for (;;) {
		const { done, value } = await reader.read();
		if (done) break;
		const chunk = decoder.decode(value, { stream: true });
		for (const frame of parser.push(chunk)) {
			const event = parseAssistantStreamFrame(frame);
			if (event !== null) onEvent(event);
		}
	}
}
