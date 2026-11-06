import type { query as QueryFn } from '@anthropic-ai/claude-agent-sdk';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import type { Config } from '../src/config.js';
import { createRequestListener } from '../src/http/app.js';
import type { AppDeps } from '../src/http/handlers.js';
import { InspectClient } from '../src/inspectClient.js';
import { FakeResponse, asResponse, fakeRequest, stubFetch } from './fakes.js';

let stateDir: string;

beforeEach(async () => {
	stateDir = await fs.mkdtemp(path.join(os.tmpdir(), 'prompt-assistant-app-'));
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

function deps(): AppDeps {
	return {
		config: config(),
		inspectClient: new InspectClient(
			'http://inspect.local',
			stubFetch({ 'GET /api/settings': { settings: [] }, 'GET /api/settings/features': { features: [] } })
		),
		queryFn: (() => {
			async function* gen() {
				/* no messages — these routing tests never reach the SDK */
			}
			return gen();
		}) as unknown as typeof QueryFn,
		now: () => new Date('2026-09-22T12:00:00Z')
	};
}

/** G15: a conversation id must be exactly a UUID before it ever reaches a filesystem path. */
describe('conversation id validation (G15)', () => {
	it('rejects a path-traversal id with 400, for GET /conversations/:id', async () => {
		const listener = createRequestListener(deps());
		const res = new FakeResponse();
		// `..%2Fbudget-ledger` has no literal '/' so it still matches the route's
		// `[^/]+` segment, but decodes to `../budget-ledger` — exactly the shape
		// that used to escape the conversations/ directory.
		await listener(
			fakeRequest('GET', '/api/assistant/conversations/..%2Fbudget-ledger'),
			asResponse(res)
		);
		expect(res.statusCode).toBe(400);
		expect(JSON.parse(res.body)).toMatchObject({ error: expect.stringContaining('UUID') });
	});

	it('rejects a path-traversal id with 400, for POST /conversations/:id/messages', async () => {
		const listener = createRequestListener(deps());
		const res = new FakeResponse();
		await listener(
			fakeRequest('POST', '/api/assistant/conversations/..%2Fbudget-ledger/messages', { text: 'hi' }),
			asResponse(res)
		);
		expect(res.statusCode).toBe(400);
	});

	it('rejects a non-UUID id that contains no traversal characters too', async () => {
		const listener = createRequestListener(deps());
		const res = new FakeResponse();
		await listener(fakeRequest('GET', '/api/assistant/conversations/not-a-uuid'), asResponse(res));
		expect(res.statusCode).toBe(400);
	});

	it('404s a well-formed but unknown UUID (distinct from the 400 for a malformed id)', async () => {
		const listener = createRequestListener(deps());
		const res = new FakeResponse();
		await listener(
			fakeRequest('GET', '/api/assistant/conversations/6127aa49-52e8-415c-9167-4c5e6939cda2'),
			asResponse(res)
		);
		expect(res.statusCode).toBe(404);
	});

	it('200s and round-trips a real, freshly-created conversation id', async () => {
		const d = deps();
		const listener = createRequestListener(d);
		const createRes = new FakeResponse();
		await listener(
			fakeRequest('POST', '/api/assistant/conversations', { request_id: 421514 }),
			asResponse(createRes)
		);
		expect(createRes.statusCode).toBe(200);
		const { id } = JSON.parse(createRes.body) as { id: string };

		const getRes = new FakeResponse();
		await listener(fakeRequest('GET', `/api/assistant/conversations/${id}`), asResponse(getRes));
		expect(getRes.statusCode).toBe(200);
		expect(JSON.parse(getRes.body)).toMatchObject({ id, request_id: 421514 });
	});
});
