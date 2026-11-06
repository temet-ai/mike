import fs from 'node:fs/promises';
import path from 'node:path';

interface Ledger {
	[localDate: string]: number;
}

function ledgerFile(stateDir: string): string {
	return path.join(stateDir, 'budget-ledger.json');
}

/**
 * The ledger is keyed by the machine's LOCAL calendar day (G13), not UTC —
 * the operator's "today" is local, and a UTC-keyed ledger rolled over at a
 * time that did not match their day. Uses the JS `Date` object's own
 * local-timezone accessors, so this follows whatever timezone the service's
 * process runs in (its host's `TZ`), with no separate configuration.
 */
function dateKey(now: Date): string {
	const year = now.getFullYear();
	const month = String(now.getMonth() + 1).padStart(2, '0');
	const day = String(now.getDate()).padStart(2, '0');
	return `${year}-${month}-${day}`;
}

async function readLedger(stateDir: string): Promise<Ledger> {
	try {
		const raw = await fs.readFile(ledgerFile(stateDir), 'utf8');
		return JSON.parse(raw) as Ledger;
	} catch (err) {
		if ((err as NodeJS.ErrnoException).code === 'ENOENT') return {};
		throw err;
	}
}

async function writeLedger(stateDir: string, ledger: Ledger): Promise<void> {
	await fs.mkdir(stateDir, { recursive: true });
	await fs.writeFile(ledgerFile(stateDir), JSON.stringify(ledger, null, 2));
}

/** Cumulative USD spend recorded for `now`'s UTC calendar day. */
export async function spentToday(stateDir: string, now: Date = new Date()): Promise<number> {
	const ledger = await readLedger(stateDir);
	return ledger[dateKey(now)] ?? 0;
}

/**
 * True when today's recorded spend is still under `dailyBudgetUsd`. Checked
 * BEFORE a turn starts — the turn itself is refused, not truncated mid-way,
 * once the budget for the day is used up.
 */
export async function canSpend(
	stateDir: string,
	dailyBudgetUsd: number,
	now: Date = new Date()
): Promise<boolean> {
	const spent = await spentToday(stateDir, now);
	return spent < dailyBudgetUsd;
}

/** Adds `amountUsd` to today's ledger entry and persists it. */
export async function addSpend(
	stateDir: string,
	amountUsd: number,
	now: Date = new Date()
): Promise<number> {
	const ledger = await readLedger(stateDir);
	const key = dateKey(now);
	const updated = (ledger[key] ?? 0) + Math.max(0, amountUsd);
	ledger[key] = updated;
	await writeLedger(stateDir, ledger);
	return updated;
}
