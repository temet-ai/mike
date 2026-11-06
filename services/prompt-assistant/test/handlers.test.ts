import type { query as QueryFn } from '@anthropic-ai/claude-agent-sdk';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Config } from '../src/config.js';
import {
	handleCreateConversation,
	handleGetConversation,
	handleHealth,
	handlePostMessage,
	type AppDeps
} from '../src/http/handlers.js';
import { InspectClient } from '../src/inspectClient.js';
import { addSpend } from '../src/store/budget.js';
import { getConversation } from '../src/store/conversations.js';
import { anthropicCredentialsRoute, asResponse, FakeResponse, fakeRequest, stubFetch } from './fakes.js';

let stateDir: string;
let now: Date;

beforeEach(async () => {
	stateDir = await fs.mkdtemp(path.join(os.tmpdir(), 'prompt-assistant-handlers-'));
	now = new Date('2026-09-22T12:00:00Z');
});
afterEach(async () => {
	await fs.rm(stateDir, { recursive: true, force: true });
});

function config(): Config {
	return {
		port: 9985,
		host: '127.0.0.1',
		inspectUrl: 'http://inspect.local',
		captureBaseUrl: 'http://localhost:9988',
		stateDir,
		sweepIntervalMs: 3_600_000,
		claudeHome: '/tmp/prompt-assistant-tests-unused-claude-home'
	};
}

function successfulFakeQuery(deltas: string[], costUsd: number): typeof QueryFn {
	return ((_params: unknown) => {
		async function* gen() {
			for (const delta of deltas) {
				yield {
					type: 'stream_event',
					event: { type: 'content_block_delta', delta: { type: 'text_delta', text: delta } },
					parent_tool_use_id: null,
					uuid: 'u-stream',
					session_id: 's'
				};
			}
			yield {
				type: 'result',
				subtype: 'success',
				duration_ms: 1,
				duration_api_ms: 1,
				is_error: false,
				num_turns: 1,
				result: deltas.join(''),
				stop_reason: null,
				total_cost_usd: costUsd,
				usage: {},
				modelUsage: {},
				permission_denials: [],
				uuid: 'u-result',
				session_id: 's'
			};
		}
		return gen();
	}) as unknown as typeof QueryFn;
}

function deps(overrides: Partial<AppDeps> = {}): AppDeps {
	return {
		config: config(),
		inspectClient: new InspectClient(
			'http://inspect.local',
			stubFetch({
				'GET /api/settings': { settings: [] },
				'GET /api/settings/features': { features: [] }
			})
		),
		queryFn: successfulFakeQuery(['Hello', ' there'], 0.004),
		now: () => now,
		...overrides
	};
}

describe('handleHealth', () => {
	it('reports defaults, today’s spend, and the daily budget', async () => {
		await addSpend(stateDir, 0.25, now);
		const health = await handleHealth(deps());
		expect(health).toEqual({
			ok: true,
			enabled: true,
			model: 'claude-sonnet-5',
			effort: 'medium',
			may_apply: 'suggest',
			spent_today_usd: 0.25,
			daily_budget_usd: 1.0,
			auth_mode: 'claude_code_login',
			credentials_available: true,
			provider: 'anthropic'
		});
	});

	it('reports credentials_available: true for claude_code_login mode even when the credentials route 404s', async () => {
		const inspectClient = new InspectClient(
			'http://inspect.local',
			stubFetch({
				'GET /api/settings': { settings: [] },
				'GET /api/settings/features': { features: [] }
			})
		);
		const health = await handleHealth(deps({ inspectClient }));
		expect(health.auth_mode).toBe('claude_code_login');
		expect(health.credentials_available).toBe(true);
	});

	it('reports credentials_available: true for api_key mode when a key is saved', async () => {
		const inspectClient = new InspectClient(
			'http://inspect.local',
			stubFetch({
				'GET /api/settings': { settings: [] },
				'GET /api/settings/features': { features: [] },
				...anthropicCredentialsRoute('api_key', 'sk-saved')
			})
		);
		const health = await handleHealth(deps({ inspectClient }));
		expect(health.auth_mode).toBe('api_key');
		expect(health.credentials_available).toBe(true);
	});

	it('reports credentials_available: false for api_key mode when no key is saved', async () => {
		const inspectClient = new InspectClient(
			'http://inspect.local',
			stubFetch({
				'GET /api/settings': { settings: [] },
				'GET /api/settings/features': { features: [] },
				...anthropicCredentialsRoute('api_key', null)
			})
		);
		const health = await handleHealth(deps({ inspectClient }));
		expect(health.auth_mode).toBe('api_key');
		expect(health.credentials_available).toBe(false);
	});
});

describe('conversations', () => {
	it('creates then fetches a conversation', async () => {
		const created = await handleCreateConversation(deps(), { request_id: 421514 });
		expect(created.status).toBe(200);
		const id = (created.body as { id: string }).id;

		const fetched = await handleGetConversation(deps(), id);
		expect(fetched.status).toBe(200);
		expect((fetched.body as { request_id: number }).request_id).toBe(421514);
		expect((fetched.body as { messages: unknown[] }).messages).toEqual([]);
	});

	it('rejects a non-integer request_id', async () => {
		const result = await handleCreateConversation(deps(), { request_id: 'nope' });
		expect(result.status).toBe(400);
	});

	it('404s for an unknown conversation id', async () => {
		const result = await handleGetConversation(deps(), 'does-not-exist');
		expect(result.status).toBe(404);
	});
});

describe('handlePostMessage', () => {
	it('streams text deltas then done, and persists both turns with cost', async () => {
		const d = deps();
		const created = await handleCreateConversation(d, { request_id: 421514 });
		const id = (created.body as { id: string }).id;
		const res = new FakeResponse();

		await handlePostMessage(d, id, { text: 'What does the Harness section say?' }, asResponse(res));

		expect(res.headers['content-type']).toBe('text/event-stream');
		expect(res.body).toContain('event: text\ndata: {"delta":"Hello"}');
		expect(res.body).toContain('event: text\ndata: {"delta":" there"}');
		expect(res.body).toContain('event: done\ndata: {"cost_usd":0.004}');
		expect(res.ended).toBe(true);

		const conversation = await getConversation(stateDir, id);
		expect(conversation?.messages).toHaveLength(2);
		expect(conversation?.messages[0]).toMatchObject({ role: 'user', text: 'What does the Harness section say?' });
		expect(conversation?.messages[1]).toMatchObject({ role: 'assistant', text: 'Hello there', cost_usd: 0.004 });
	});

	it('records the quote on the user message', async () => {
		const d = deps();
		const created = await handleCreateConversation(d, { request_id: 421514 });
		const id = (created.body as { id: string }).id;
		await handlePostMessage(d, id, { text: 'reword this', quote: 'the quoted paragraph' }, asResponse(new FakeResponse()));
		const conversation = await getConversation(stateDir, id);
		expect(conversation?.messages[0]?.quote).toBe('the quoted paragraph');
	});

	it('refuses with an error event and never calls the SDK once today’s budget is spent', async () => {
		const queryFn = vi.fn(successfulFakeQuery(['unused'], 0.01));
		const d = deps({ queryFn });
		await addSpend(stateDir, 1.0, now);
		const created = await handleCreateConversation(d, { request_id: 421514 });
		const id = (created.body as { id: string }).id;
		const res = new FakeResponse();

		await handlePostMessage(d, id, { text: 'hello' }, asResponse(res));

		expect(queryFn).not.toHaveBeenCalled();
		expect(res.body).toContain('event: error');
		expect(res.body).toContain('budget');
		expect(res.body).not.toContain('event: done');
	});

	it('refuses with an error event when the prompt_assistant feature toggle is off', async () => {
		const queryFn = vi.fn(successfulFakeQuery(['unused'], 0.01));
		const inspectClient = new InspectClient(
			'http://inspect.local',
			stubFetch({
				'GET /api/settings': { settings: [] },
				'GET /api/settings/features': {
					features: [{ key: 'prompt_assistant', enabled: false, scope: 'global', applies: 'immediate' }]
				}
			})
		);
		const d = deps({ queryFn, inspectClient });
		const created = await handleCreateConversation(d, { request_id: 421514 });
		const id = (created.body as { id: string }).id;
		const res = new FakeResponse();

		await handlePostMessage(d, id, { text: 'hello' }, asResponse(res));

		expect(queryFn).not.toHaveBeenCalled();
		expect(res.body).toContain('event: error');
	});

	it('refuses with a clear error event and never calls the SDK when auth mode is api_key with no key saved', async () => {
		const queryFn = vi.fn(successfulFakeQuery(['unused'], 0.01));
		const inspectClient = new InspectClient(
			'http://inspect.local',
			stubFetch({
				'GET /api/settings': { settings: [] },
				'GET /api/settings/features': { features: [] },
				...anthropicCredentialsRoute('api_key', null)
			})
		);
		const d = deps({ queryFn, inspectClient });
		const created = await handleCreateConversation(d, { request_id: 421514 });
		const id = (created.body as { id: string }).id;
		const res = new FakeResponse();

		await handlePostMessage(d, id, { text: 'hello' }, asResponse(res));

		expect(queryFn).not.toHaveBeenCalled();
		expect(res.body).toContain('event: error');
		expect(res.body).toContain('Anthropic API key');
		expect(res.body).not.toContain('event: done');
	});

	it('runs the turn normally when auth mode is api_key and a key is saved', async () => {
		const inspectClient = new InspectClient(
			'http://inspect.local',
			stubFetch({
				'GET /api/settings': { settings: [] },
				'GET /api/settings/features': { features: [] },
				...anthropicCredentialsRoute('api_key', 'sk-saved')
			})
		);
		const d = deps({ inspectClient });
		const created = await handleCreateConversation(d, { request_id: 421514 });
		const id = (created.body as { id: string }).id;
		const res = new FakeResponse();

		await handlePostMessage(d, id, { text: 'hello' }, asResponse(res));

		expect(res.body).toContain('event: done');
	});

	it('404s (plain JSON, no SSE) for an unknown conversation id', async () => {
		const res = new FakeResponse();
		await handlePostMessage(deps(), 'missing', { text: 'hi' }, asResponse(res));
		expect(res.statusCode).toBe(404);
	});

	it('400s for an empty message body', async () => {
		const d = deps();
		const created = await handleCreateConversation(d, { request_id: 421514 });
		const id = (created.body as { id: string }).id;
		const res = new FakeResponse();
		await handlePostMessage(d, id, { text: '   ' }, asResponse(res));
		expect(res.statusCode).toBe(400);
	});

	it('400s for an empty project_path', async () => {
		const d = deps();
		const created = await handleCreateConversation(d, { request_id: 421514 });
		const id = (created.body as { id: string }).id;
		const res = new FakeResponse();
		await handlePostMessage(d, id, { text: 'hi', project_path: '' }, asResponse(res));
		expect(res.statusCode).toBe(400);
	});

	it('aborts the SDK query when the client disconnects mid-stream (G13)', async () => {
		const res = new FakeResponse();
		let capturedAbort: AbortController | undefined;
		const queryFn = ((params: { options?: { abortController?: AbortController } }) => {
			capturedAbort = params.options?.abortController;
			async function* gen() {
				yield {
					type: 'stream_event',
					event: { type: 'content_block_delta', delta: { type: 'text_delta', text: 'x' } },
					parent_tool_use_id: null,
					uuid: 'u',
					session_id: 's'
				};
				res.simulateClose();
				yield {
					type: 'result',
					subtype: 'success',
					duration_ms: 1,
					duration_api_ms: 1,
					is_error: false,
					num_turns: 1,
					result: 'x',
					stop_reason: null,
					total_cost_usd: 0.001,
					usage: {},
					modelUsage: {},
					permission_denials: [],
					uuid: 'u2',
					session_id: 's'
				};
			}
			return gen();
		}) as unknown as typeof QueryFn;
		const d = deps({ queryFn });
		const created = await handleCreateConversation(d, { request_id: 421514 });
		const id = (created.body as { id: string }).id;

		await handlePostMessage(d, id, { text: 'hi' }, asResponse(res));

		expect(capturedAbort).toBeDefined();
		expect(capturedAbort?.signal.aborted).toBe(true);
	});
});
