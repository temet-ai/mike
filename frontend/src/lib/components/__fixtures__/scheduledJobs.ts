import type {
	ScheduledJob,
	ScheduledJobGroup,
	ScheduledJobWindow,
	ScheduledJobsResponse
} from '$lib/api/scheduledJobs';

/** A Friday morning, after the night's jobs have run. */
export const NOW_DAY = Date.parse('2026-11-13T09:30:00Z');
/** The middle of the nightly chain. */
export const NOW_NIGHT = Date.parse('2026-11-13T02:40:00Z');

export const THIS_MACHINE = 'a1b2c3d4-0000-0000-0000-00000000000a';
export const OTHER_MACHINE = 'f9e8d7c6-0000-0000-0000-00000000000b';
export const TIME_ZONE = 'Africa/Johannesburg';

/** The window a job in `scripts` may start in, as the proxy reports it. */
const win = (
	period: ScheduledJobWindow['period'],
	hours_utc: number[],
	minute: number,
	weekday: string | null = null
): ScheduledJobWindow => ({ period, weekday, hours_utc, minute });

interface Spec {
	id: string;
	label: string;
	description: string;
	schedule: string;
	group: ScheduledJobGroup;
	window: ScheduledJobWindow;
	/** UTC start and run length in minutes of the last run, as of NOW_DAY. */
	lastStart: string;
	minutes: number;
	next: string;
	enabled_key: string;
	fix: string;
}

const SPECS: Spec[] = [
	{
		id: 'learning_promote',
		label: 'Learning promotion',
		description:
			'Turns accepted lessons into project memories and skills in the project folders on this machine, so Claude Code sessions here find them.',
		schedule: 'hourly at :30 from 20:30 to 03:30 UTC',
		group: 'learning_and_memory',
		window: win('hour', [20, 21, 22, 23, 0, 1, 2, 3], 30),
		lastStart: '2026-11-13T03:30:00Z',
		minutes: 3,
		next: '2026-11-13T20:30:00Z',
		enabled_key: 'learning_promote.enabled',
		fix: 'Exit code 1 means generated files conflict with manual edits.'
	},
	{
		id: 'lesson_relations',
		label: 'Lesson relation detector',
		description:
			'Finds lessons that supersede, refine, repeat or contradict each other and has a second model adjudicate each claim.',
		schedule: 'daily, once between 22:00 and 23:59 UTC, before the nightly lesson consolidation',
		group: 'learning_and_memory',
		window: win('day', [22, 23], 0),
		lastStart: '2026-11-12T22:00:00Z',
		minutes: 6,
		next: '2026-11-13T22:00:00Z',
		enabled_key: 'lesson_relations_job.enabled',
		fix: 'Check that both models answer and that lesson_relations.enabled is on.'
	},
	{
		id: 'learning_audit',
		label: 'Weekly learning audit',
		description:
			"Reviews the previous week's sessions, lessons and tool use and publishes a report, so you can see whether the learning is helping.",
		schedule: 'weekly, once on Monday between 01:00 and 06:59 UTC',
		group: 'learning_and_memory',
		window: win('week', [1, 2, 3, 4, 5, 6], 0, 'Mon'),
		lastStart: '2026-11-09T01:00:00Z',
		minutes: 12,
		next: '2026-11-16T01:00:00Z',
		enabled_key: 'learning_audit.enabled',
		fix: 'Check that psql and the inspect API on 127.0.0.1:9989 answer.'
	},
	{
		id: 'lessons_commit',
		label: 'Lesson file commit',
		description:
			'Commits the lesson files the knowledge collector writes into .claude/lessons of this checkout, so the durable copy of the lessons is in git.',
		schedule: 'daily, once between 01:30 and 03:59 UTC (03:30 in Johannesburg)',
		group: 'learning_and_memory',
		window: win('day', [1, 2, 3], 30),
		lastStart: '2026-11-13T01:30:00Z',
		minutes: 1,
		next: '2026-11-14T01:30:00Z',
		enabled_key: 'lessons_commit.enabled',
		fix: 'Check that git status works in the checkout.'
	},
	{
		id: 'memory_review',
		label: 'Daily memory review',
		description:
			'A Claude Code session reads what memory returned to sessions since the last review and records the fix on the memory-keeper thread.',
		schedule: 'daily, once between 05:15 and 07:59 UTC, after the overnight supply verdicts',
		group: 'learning_and_memory',
		window: win('day', [5, 6, 7], 15),
		lastStart: '2026-11-13T05:15:00Z',
		minutes: 20,
		next: '2026-11-14T05:15:00Z',
		enabled_key: 'memory_review.enabled',
		fix: 'Check that claude is logged in on this machine and that the proxy answers.'
	},
	{
		id: 'compressor_loop',
		label: 'File-read compressor loop',
		description:
			"Compares sessions that got shortened file reads with those that did not and adjusts the compressor's settings.",
		schedule: 'daily, once between 04:30 and 06:59 UTC',
		group: 'measurement',
		window: win('day', [4, 5, 6], 30),
		lastStart: '2026-11-13T04:30:00Z',
		minutes: 9,
		next: '2026-11-14T04:30:00Z',
		enabled_key: 'compressor_loop.enabled',
		fix: 'Check that psql and the inspect API answer.'
	},
	{
		id: 'usage_audit',
		label: 'Weekly usage audit',
		description:
			"Reports where the week's Claude tokens went and raises an alarm when the cache re-write rate or context size look wrong.",
		schedule: 'weekly, once on Sunday between 06:00 and 10:59 UTC (08:00 in Johannesburg)',
		group: 'measurement',
		window: win('week', [6, 7, 8, 9, 10], 0, 'Sun'),
		lastStart: '2026-11-08T06:00:00Z',
		minutes: 4,
		next: '2026-11-15T06:00:00Z',
		enabled_key: 'usage_audit.enabled',
		fix: 'Check that MIKE_DATABASE_URL in the checkout .env answers.'
	},
	{
		id: 'cargo_sweep',
		label: 'Rust build folder sweep',
		description:
			'Deletes old and idle Rust build folders on this machine, so builds do not fill the disk.',
		schedule: 'every six hours, at 00:00, 06:00, 12:00 and 18:00 UTC',
		group: 'builds_and_tests',
		window: win('hour', [0, 6, 12, 18], 0),
		lastStart: '2026-11-13T06:00:00Z',
		minutes: 2,
		next: '2026-11-13T12:00:00Z',
		enabled_key: 'cargo_sweep.enabled',
		fix: 'It removes only superseded or idle build folders.'
	},
	{
		id: 'nightly_tests',
		label: 'Nightly full Rust test run',
		description: 'Runs every Rust test tier against the tip of main in a worktree of its own.',
		schedule: 'daily, once between 02:00 and 04:59 UTC',
		group: 'builds_and_tests',
		window: win('day', [2, 3, 4], 0),
		lastStart: '2026-11-13T02:00:00Z',
		minutes: 41,
		next: '2026-11-14T02:00:00Z',
		enabled_key: 'nightly_tests.enabled',
		fix: 'The last line of its output names the commit; rerun the failing tier there.'
	}
];

const MIN = 60_000;
const DAY = 86_400_000;

/** Last run and next run of a spec as of `now`: a run later than `now` moves back whole periods. */
function relativeTo(spec: Spec, now: number): { start: number; next: string } {
	const period = spec.window.period === 'week' ? 7 * DAY : DAY;
	let start = Date.parse(spec.lastStart);
	let next = Date.parse(spec.next);
	while (start > now) {
		start -= period;
		next -= period;
	}
	return { start, next: new Date(next).toISOString() };
}

function build(spec: Spec, now: number): ScheduledJob {
	const { start, next } = relativeTo(spec, now);
	return {
		id: spec.id,
		label: spec.label,
		description: spec.description,
		schedule: spec.schedule,
		scope: 'this_machine',
		owner_machine_uuid: null,
		enabled: true,
		state: 'ok',
		last_started_at: new Date(start).toISOString(),
		last_finished_at: new Date(start + spec.minutes * MIN).toISOString(),
		last_exit_code: 0,
		last_output_tail: null,
		next_run_at: next,
		running_since: null,
		run_requested_at: null,
		last_trigger: 'schedule',
		consecutive_failures: 0,
		detail: '',
		fix: '',
		group: spec.group,
		window: spec.window,
		enabled_key: spec.enabled_key
	};
}

/** The nine real jobs, healthy as of `now` (NOW_DAY by default); no run is later than `now`. */
export function realJobs(now: number = NOW_DAY): ScheduledJob[] {
	return SPECS.map((spec) => build(spec, now));
}

/** One of the nine, with changes. */
export function realJob(
	id: string,
	overrides: Partial<ScheduledJob> = {},
	now: number = NOW_DAY
): ScheduledJob {
	const spec = SPECS.find((s) => s.id === id);
	if (!spec) throw new Error(`no fixture job ${id}`);
	return { ...build(spec, now), ...overrides };
}

/** The nine jobs with some replaced by id. */
export function realJobsWith(
	changes: Record<string, Partial<ScheduledJob>>,
	now: number = NOW_DAY
): ScheduledJob[] {
	return SPECS.map((spec) => ({ ...build(spec, now), ...changes[spec.id] }));
}

export const iso = (ms: number): string => new Date(ms).toISOString();

export function response(jobs: ScheduledJob[]): ScheduledJobsResponse {
	return { machine_uuid: THIS_MACHINE, machine_name: 'omit', jobs };
}
