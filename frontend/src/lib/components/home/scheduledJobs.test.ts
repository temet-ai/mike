import { describe, expect, it } from 'vitest';
import type { ScheduledJob } from '$lib/api/scheduledJobs';
import {
	isUnavailable,
	lastRanText,
	nextText,
	relativeTime,
	runsOnAnotherMachine,
	sortJobs,
	stateView
} from './scheduledJobs';

const NOW = Date.parse('2026-10-14T12:00:00Z');
const at = (offsetMs: number): string => new Date(NOW + offsetMs).toISOString();

function job(overrides: Partial<ScheduledJob> = {}): ScheduledJob {
	return {
		id: 'backup',
		label: 'Backup',
		description: 'Copies the capture database.',
		schedule: 'every 6h',
		scope: 'this_machine',
		owner_machine_uuid: null,
		enabled: true,
		state: 'ok',
		last_started_at: at(-300_000),
		last_finished_at: at(-290_000),
		last_exit_code: 0,
		last_output_tail: null,
		next_run_at: at(3_600_000),
		running_since: null,
		run_requested_at: null,
		last_trigger: 'schedule',
		consecutive_failures: 0,
		detail: '',
		fix: '',
		...overrides
	};
}

describe('relativeTime', () => {
	it('words a past time as ago', () => {
		expect(relativeTime(at(-30_000), NOW)).toBe('30s ago');
		expect(relativeTime(at(-5 * 60_000), NOW)).toBe('5m ago');
		expect(relativeTime(at(-3 * 3_600_000), NOW)).toBe('3h ago');
		expect(relativeTime(at(-2 * 86_400_000), NOW)).toBe('2d ago');
	});

	it('words a future time as in', () => {
		expect(relativeTime(at(45_000), NOW)).toBe('in 45s');
		expect(relativeTime(at(10 * 60_000), NOW)).toBe('in 10m');
		expect(relativeTime(at(6 * 3_600_000), NOW)).toBe('in 6h');
		expect(relativeTime(at(3 * 86_400_000), NOW)).toBe('in 3d');
	});

	it('says just now within a second either way', () => {
		expect(relativeTime(at(0), NOW)).toBe('just now');
		expect(relativeTime(at(500), NOW)).toBe('just now');
	});

	it('gives an em dash for a missing or unparseable time', () => {
		expect(relativeTime(null, NOW)).toBe('—');
		expect(relativeTime('not a date', NOW)).toBe('—');
	});
});

describe('lastRanText and nextText', () => {
	it('reads last ran and next for a healthy job', () => {
		const j = job();
		expect(lastRanText(j, NOW)).toBe('last ran 5m ago');
		expect(nextText(j, NOW)).toBe('next in 1h');
	});

	it('says never ran when there is no start time', () => {
		expect(lastRanText(job({ state: 'never_run', last_started_at: null }), NOW)).toBe('never ran');
	});

	it('says started for a running job', () => {
		expect(lastRanText(job({ state: 'running', last_started_at: at(-12_000) }), NOW)).toBe(
			'started 12s ago'
		);
	});

	it('adds by hand only for a manual run', () => {
		expect(lastRanText(job({ last_trigger: 'manual' }), NOW)).toBe('last ran 5m ago, by hand');
		expect(
			lastRanText(job({ state: 'running', last_trigger: 'manual', last_started_at: at(-5_000) }), NOW)
		).toBe('started 5s ago, by hand');
		expect(lastRanText(job({ last_trigger: null }), NOW)).toBe('last ran 5m ago');
	});

	it('says next due now when the next run is not in the future', () => {
		expect(nextText(job({ next_run_at: at(-60_000) }), NOW)).toBe('next due now');
	});

	it('says not scheduled for a job that is off or disabled', () => {
		expect(nextText(job({ state: 'off' }), NOW)).toBe('not scheduled');
		expect(nextText(job({ enabled: false }), NOW)).toBe('not scheduled');
	});

	it('shows a dash when an enabled job has no next time', () => {
		expect(nextText(job({ next_run_at: null }), NOW)).toBe('next —');
	});
});

describe('stateView', () => {
	it('maps every state to a label and badge variant', () => {
		expect(stateView('ok')).toMatchObject({ label: 'ok', variant: 'chip-active' });
		expect(stateView('failing')).toMatchObject({ label: 'failing', variant: 'chip-error' });
		expect(stateView('overdue')).toMatchObject({ label: 'overdue', variant: 'chip-warning' });
		expect(stateView('never_run')).toMatchObject({ label: 'never run', variant: 'chip-warning' });
		expect(stateView('off')).toMatchObject({ label: 'off', variant: 'chip-muted' });
		expect(stateView('running')).toMatchObject({ label: 'running', variant: 'chip-analyzed' });
		expect(stateView('unavailable')).toMatchObject({
			label: 'not available',
			variant: 'chip-muted'
		});
	});

	it('asks for attention only on failing, overdue and never_run', () => {
		const attention = (
			['ok', 'failing', 'overdue', 'never_run', 'off', 'running', 'unavailable'] as const
		).filter((s) => stateView(s).needsAttention);
		expect(attention).toEqual(['failing', 'overdue', 'never_run']);
	});

	it('tells a job this mike cannot run from one that is switched off', () => {
		expect(isUnavailable(job({ state: 'unavailable' }))).toBe(true);
		expect(isUnavailable(job({ state: 'off', enabled: false }))).toBe(false);
	});

	it('falls back to the off view for a state it does not know', () => {
		expect(stateView('surprising' as never).label).toBe('off');
	});
});

describe('runsOnAnotherMachine', () => {
	it('is true for a one-machine job owned by a different machine', () => {
		const j = job({ scope: 'one_machine', owner_machine_uuid: 'mac-uuid' });
		expect(runsOnAnotherMachine(j, 'omit-uuid')).toBe(true);
	});

	it('is false when this machine owns the job', () => {
		const j = job({ scope: 'one_machine', owner_machine_uuid: 'omit-uuid' });
		expect(runsOnAnotherMachine(j, 'omit-uuid')).toBe(false);
	});

	it('is false for a this-machine job, or when either side is unknown', () => {
		expect(runsOnAnotherMachine(job({ owner_machine_uuid: 'mac-uuid' }), 'omit-uuid')).toBe(false);
		const owned = job({ scope: 'one_machine', owner_machine_uuid: null });
		expect(runsOnAnotherMachine(owned, 'omit-uuid')).toBe(false);
		const unknownSelf = job({ scope: 'one_machine', owner_machine_uuid: 'mac-uuid' });
		expect(runsOnAnotherMachine(unknownSelf, null)).toBe(false);
	});
});

describe('sortJobs', () => {
	it('orders by label then id, ignoring when a job last ran', () => {
		const a = job({ id: 'z-1', label: 'Alpha', last_started_at: at(-1_000) });
		const b = job({ id: 'b-1', label: 'Beta', last_started_at: at(-900_000) });
		const c = job({ id: 'a-2', label: 'Beta', last_started_at: at(-10) });
		expect(sortJobs([c, b, a]).map((j) => j.id)).toEqual(['z-1', 'a-2', 'b-1']);
	});

	it('does not change the array it is given', () => {
		const input = [job({ id: 'b', label: 'B' }), job({ id: 'a', label: 'A' })];
		sortJobs(input);
		expect(input.map((j) => j.id)).toEqual(['b', 'a']);
	});
});
