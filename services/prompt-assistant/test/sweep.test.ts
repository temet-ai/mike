import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { appendMessage, createConversation, getConversation, newMessage } from '../src/store/conversations.js';
import { sweepOldConversations } from '../src/store/sweep.js';

let stateDir: string;

beforeEach(async () => {
	stateDir = await fs.mkdtemp(path.join(os.tmpdir(), 'prompt-assistant-sweep-'));
});
afterEach(async () => {
	await fs.rm(stateDir, { recursive: true, force: true });
});

describe('sweepOldConversations', () => {
	it('deletes a conversation whose last message is older than keep_days, keeps a recent one', async () => {
		const stale = await createConversation(stateDir, 1);
		await appendMessage(stateDir, stale.id, {
			...newMessage('user', 'old message'),
			created_at: '2026-08-01T00:00:00Z'
		});

		const fresh = await createConversation(stateDir, 2);
		await appendMessage(stateDir, fresh.id, {
			...newMessage('user', 'recent message'),
			created_at: '2026-09-21T00:00:00Z'
		});

		const deleted = await sweepOldConversations(stateDir, 30, new Date('2026-09-22T00:00:00Z'));

		expect(deleted).toEqual([stale.id]);
		expect(await getConversation(stateDir, stale.id)).toBeNull();
		expect(await getConversation(stateDir, fresh.id)).not.toBeNull();
	});

	it('uses created_at as the cutoff for a conversation with no messages yet', async () => {
		const empty = await createConversation(stateDir, 3);
		// Directly touch the file's created_at into the past via the store API surface: append then remove is
		// unnecessary — createConversation stamps created_at = now, so simulate age with a very tight keep_days.
		const deleted = await sweepOldConversations(stateDir, 0, new Date(Date.now() + 1000));
		expect(deleted).toEqual([empty.id]);
	});

	it('sweeps nothing when every conversation is within keep_days', async () => {
		const c = await createConversation(stateDir, 4);
		await appendMessage(stateDir, c.id, newMessage('user', 'hi'));
		const deleted = await sweepOldConversations(stateDir, 30, new Date());
		expect(deleted).toEqual([]);
	});
});
