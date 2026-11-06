import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { addSpend, canSpend, spentToday } from '../src/store/budget.js';

let stateDir: string;

beforeEach(async () => {
	stateDir = await fs.mkdtemp(path.join(os.tmpdir(), 'prompt-assistant-budget-'));
});
afterEach(async () => {
	await fs.rm(stateDir, { recursive: true, force: true });
});

// Local-calendar-day boundaries (G13: the ledger is keyed by the machine's
// local day, not UTC), so every date below is built with the local-time
// `Date(year, monthIndex, day, ...)` constructor rather than an ISO 'Z'
// string — the test then holds regardless of which timezone it runs in,
// exactly like the ledger it exercises.
describe('budget ledger', () => {
	it('starts at zero spend for a day with no ledger file yet', async () => {
		expect(await spentToday(stateDir, new Date(2026, 8, 22, 12, 0, 0))).toBe(0);
	});

	it('accumulates spend within the same local day', async () => {
		const now = new Date(2026, 8, 22, 9, 0, 0);
		await addSpend(stateDir, 0.3, now);
		await addSpend(stateDir, 0.25, now);
		expect(await spentToday(stateDir, now)).toBeCloseTo(0.55);
	});

	it('keeps separate totals per local calendar day', async () => {
		await addSpend(stateDir, 0.9, new Date(2026, 8, 21, 23, 59, 0));
		await addSpend(stateDir, 0.1, new Date(2026, 8, 22, 0, 1, 0));
		expect(await spentToday(stateDir, new Date(2026, 8, 21, 12, 0, 0))).toBeCloseTo(0.9);
		expect(await spentToday(stateDir, new Date(2026, 8, 22, 12, 0, 0))).toBeCloseTo(0.1);
	});

	it('canSpend refuses once today’s spend reaches the daily budget', async () => {
		const now = new Date(2026, 8, 22, 12, 0, 0);
		await addSpend(stateDir, 1.0, now);
		expect(await canSpend(stateDir, 1.0, now)).toBe(false);
		expect(await canSpend(stateDir, 1.01, now)).toBe(true);
	});

	it('canSpend allows a fresh day even after yesterday exceeded budget', async () => {
		await addSpend(stateDir, 5, new Date(2026, 8, 21, 12, 0, 0));
		expect(await canSpend(stateDir, 1.0, new Date(2026, 8, 22, 0, 0, 1))).toBe(true);
	});
});
