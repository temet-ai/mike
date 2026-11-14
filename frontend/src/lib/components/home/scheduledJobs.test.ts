import { describe, expect, it } from 'vitest';
import type { ScheduledJob } from '$lib/api/scheduledJobs';
import { NOW_DAY, TIME_ZONE, iso, realJob, realJobs } from '../__fixtures__/scheduledJobs';
import {
	axisPercent,
	describeLane,
	hoverSegments,
	offsetMinutes,
	runStrip,
	spanSegments,
	windowShape
} from './jobClock';
import { groupJobs, jobStatus, relativeTime, rowView, runsOnAnotherMachine } from './scheduledJobs';

const NOW = NOW_DAY;
const at = (offsetMs: number): string => iso(NOW + offsetMs);
const ctx = { queued: false, machineUuid: null, now: NOW, timeZone: TIME_ZONE };

function job(overrides: Partial<ScheduledJob> = {}): ScheduledJob {
	return realJob('cargo_sweep', overrides);
}

describe('relativeTime', () => {
	it('words past and future times', () => {
		expect(relativeTime(at(-5 * 60_000), NOW)).toBe('5m ago');
		expect(relativeTime(at(6 * 3_600_000), NOW)).toBe('in 6h');
		expect(relativeTime(at(500), NOW)).toBe('just now');
		expect(relativeTime(null, NOW)).toBe('—');
	});
});

describe('jobStatus', () => {
	it('takes the first match in the precedence order', () => {
		expect(jobStatus(job({ state: 'unavailable' }), ctx)).toBe('unavailable');
		const remote = job({ scope: 'one_machine', owner_machine_uuid: 'mac' });
		expect(jobStatus(remote, { ...ctx, machineUuid: 'omit' })).toBe('elsewhere');
		expect(jobStatus(job({ running_since: at(-1_000), enabled: false }), ctx)).toBe('running');
		expect(jobStatus(job({ run_requested_at: at(-1_000) }), ctx)).toBe('queued');
		expect(jobStatus(job(), { ...ctx, queued: true })).toBe('queued');
		expect(jobStatus(job({ enabled: false, state: 'failing' }), ctx)).toBe('off');
		expect(jobStatus(job({ state: 'failing' }), ctx)).toBe('failing');
		expect(
			jobStatus(
				job({ state: 'overdue', last_started_at: at(-86_400_000), next_run_at: at(-3_600_000) }),
				ctx
			)
		).toBe('overdue');
		expect(jobStatus(job({ state: 'never_run', last_started_at: null }), ctx)).toBe('never_run');
		expect(jobStatus(job(), ctx)).toBe('ok');
	});

	it('never calls a job overdue when it started a run after the missed time', () => {
		const ranSince = job({
			state: 'overdue',
			last_trigger: 'manual',
			last_started_at: at(-60_000),
			next_run_at: at(-3_600_000)
		});
		const view = rowView(ranSince, ctx);
		expect(view.status).toBe('ok');
		expect(view.lastRan).toBe('ran 1m ago · by hand');
	});

	it('tells another machine apart from this one', () => {
		const j = job({ scope: 'one_machine', owner_machine_uuid: 'mac' });
		expect(runsOnAnotherMachine(j, 'omit')).toBe(true);
		expect(runsOnAnotherMachine(j, 'mac')).toBe(false);
		expect(runsOnAnotherMachine(j, null)).toBe(false);
	});
});

describe('rowView', () => {
	it('reads last ran and next in local time', () => {
		const view = rowView(job(), ctx);
		expect(view.lastRan).toBe('ran 3h ago · on schedule');
		expect(view.next).toBe('next 14:00 · in 2h');
	});

	it('reads a run stamped later than now as just now, never "ran in"', () => {
		const future = job({ last_started_at: at(2 * 3_600_000) });
		expect(rowView(future, ctx).lastRan).toBe('ran just now · on schedule');
		const running = job({ running_since: at(5_000), last_started_at: at(5_000) });
		expect(rowView(running, ctx).lastRan).toBe('started just now · on schedule');
	});

	it('names the weekday for a weekly job', () => {
		const audit = realJob('learning_audit');
		expect(rowView(audit, ctx).next).toBe('next Mon 03:00 · in 2d');
	});

	it('says never ran, due now and next after this run', () => {
		expect(rowView(job({ last_started_at: null }), ctx).lastRan).toBe('never ran');
		expect(rowView(job({ next_run_at: at(-60_000) }), ctx).next).toBe('due now');
		const running = job({
			running_since: at(-20_000),
			last_started_at: at(-20_000),
			next_run_at: at(-60_000)
		});
		const view = rowView(running, ctx);
		expect(view.lastRan).toBe('started 20s ago · on schedule');
		expect(view.next).toBe('next after this run');
	});

	it('disables Run now exactly when the status blocks it', () => {
		const blocked = ['unavailable', 'elsewhere', 'running', 'queued', 'off'];
		const cases: [Partial<ScheduledJob>, string][] = [
			[{ state: 'unavailable' }, 'unavailable'],
			[{ scope: 'one_machine', owner_machine_uuid: 'mac' }, 'elsewhere'],
			[{ running_since: at(-1_000) }, 'running'],
			[{ run_requested_at: at(-1_000) }, 'queued'],
			[{ enabled: false, state: 'off' }, 'off'],
			[{ state: 'failing' }, 'failing'],
			[{}, 'ok']
		];
		for (const [overrides, status] of cases) {
			const view = rowView(job(overrides), { ...ctx, machineUuid: 'omit' });
			expect(view.status).toBe(status);
			expect(view.runDisabled).toBe(blocked.includes(status));
		}
	});

	it('makes Run now the resolving action on an attention row', () => {
		expect(rowView(job({ state: 'failing' }), ctx).runVariant).toBe('default');
		expect(rowView(job(), ctx).runVariant).toBe('outline');
	});
});

describe('groupJobs', () => {
	it('orders groups and sorts each by where its window starts', () => {
		const groups = groupJobs(realJobs());
		expect(groups.map((g) => g.label)).toEqual([
			'Learning and memory',
			'Measurement',
			'Builds and tests'
		]);
		expect(groups.map((g) => g.jobs.map((j) => j.id))).toEqual([
			['learning_promote', 'lesson_relations', 'learning_audit', 'lessons_commit', 'memory_review'],
			['compressor_loop', 'usage_audit'],
			['cargo_sweep', 'nightly_tests']
		]);
	});

	it('puts a job with no or an unknown group last under Other, without mutating the input', () => {
		const input = [job({ id: 'z', group: null }), realJob('nightly_tests')];
		const groups = groupJobs(input);
		expect(groups.map((g) => g.label)).toEqual(['Builds and tests', 'Other']);
		expect(input.map((j) => j.id)).toEqual(['z', 'nightly_tests']);
	});
});

describe('jobClock', () => {
	it('puts 00:00 UTC in the middle of the axis', () => {
		expect(axisPercent(0)).toBe(50);
		expect(axisPercent(12 * 60)).toBe(0);
		expect(axisPercent(9 * 60 + 30)).toBeCloseTo(89.58, 1);
	});

	it('splits a span at the 12:00 UTC seam', () => {
		expect(spanSegments(11 * 60, 120)).toEqual([
			{ left: expect.closeTo(95.83, 1), width: expect.closeTo(4.17, 1) },
			{ left: 0, width: expect.closeTo(4.17, 1) }
		]);
	});

	it('draws a day window as one bar and an hourly one as ticks', () => {
		const relations = windowShape(realJob('lesson_relations').window);
		expect(relations).toMatchObject({ kind: 'bar', weekday: null });
		const audit = windowShape(realJob('learning_audit').window);
		expect(audit).toMatchObject({ kind: 'bar', weekday: 'Mon' });
		const sweep = windowShape(realJob('cargo_sweep').window);
		expect(sweep).toMatchObject({ kind: 'ticks' });
		expect(hoverSegments(realJob('cargo_sweep').window)).toHaveLength(4);
		expect(windowShape(null)).toEqual({ kind: 'none' });
	});

	it('finds the zone offset', () => {
		expect(offsetMinutes(NOW, TIME_ZONE)).toBe(120);
		expect(offsetMinutes(NOW, 'UTC')).toBe(0);
	});

	it('draws a run from the last 24 hours only', () => {
		expect(runStrip(job(), NOW)?.kind).toBe('complete');
		expect(runStrip(job({ last_exit_code: 2 }), NOW)?.kind).toBe('error');
		expect(runStrip(job({ last_started_at: at(-30 * 3_600_000) }), NOW)).toBeNull();
	});

	it('describes a lane in words', () => {
		expect(describeLane(realJob('lesson_relations'), NOW, TIME_ZONE)).toBe(
			'Starts once a day between 22:00 and 23:59 UTC (00:00 and 01:59 local). Last run 22:00–22:06 UTC, on schedule, exit 0.'
		);
		expect(describeLane(realJob('learning_audit'), NOW, TIME_ZONE)).toContain(
			'Starts once on Monday between 01:00 and 06:59 UTC (03:00 and 08:59 local).'
		);
		expect(describeLane(realJob('cargo_sweep'), NOW, TIME_ZONE)).toBe(
			'Starts at 00:00, 06:00, 12:00 and 18:00 UTC (02:00, 08:00, 14:00 and 20:00 local). Last run 06:00–06:02 UTC, on schedule, exit 0.'
		);
		expect(describeLane(realJob('learning_promote'), NOW, TIME_ZONE)).toContain(
			'Starts hourly at :30 from 20:30 to 03:30 UTC (22:30 to 05:30 local).'
		);
		expect(describeLane(job({ window: null }), NOW, TIME_ZONE)).toBe(
			'No schedule window reported.'
		);
	});
});
