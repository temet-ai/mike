import type { Component } from 'svelte';
import {
	AlarmClock,
	Ban,
	CircleCheck,
	CircleDashed,
	CircleOff,
	CircleX,
	Hourglass,
	Loader,
	Monitor
} from '@lucide/svelte';
import type { ScheduledJob, ScheduledJobGroup } from '$lib/api/scheduledJobs';
import type { StatusMarkTone } from '$lib/components/ui/status-mark';
import { formatRelative, parseIso } from '$lib/time';
import { localClock, windowStart, zonedParts } from './jobClock';

export type JobStatus =
	| 'unavailable'
	| 'elsewhere'
	| 'running'
	| 'queued'
	| 'off'
	| 'failing'
	| 'overdue'
	| 'never_run'
	| 'ok';

interface StatusLook {
	icon: Component;
	word: string;
	tone: StatusMarkTone;
	spin?: boolean;
}

export const STATUS_LOOK: Record<JobStatus, StatusLook> = {
	unavailable: { icon: Ban, word: 'no scripts', tone: 'quiet' },
	elsewhere: { icon: Monitor, word: 'elsewhere', tone: 'quiet' },
	running: { icon: Loader, word: 'running', tone: 'info', spin: true },
	queued: { icon: Hourglass, word: 'queued', tone: 'strong' },
	off: { icon: CircleOff, word: 'off', tone: 'quiet' },
	failing: { icon: CircleX, word: 'failing', tone: 'danger' },
	overdue: { icon: AlarmClock, word: 'overdue', tone: 'warning' },
	never_run: { icon: CircleDashed, word: 'never run', tone: 'warning' },
	ok: { icon: CircleCheck, word: 'ok', tone: 'success' }
};

/** Failing, overdue and never run: the rows that ask for a look. */
export const ATTENTION: readonly JobStatus[] = ['failing', 'overdue', 'never_run'];

export type LaneTone = 'neutral' | 'live' | 'danger' | 'warning' | 'hollow';

export interface RowView {
	status: JobStatus;
	icon: Component;
	word: string;
	tone: StatusMarkTone;
	spin: boolean;
	statusTip: string;
	attention: boolean;
	lastRan: string;
	next: string;
	runDisabled: boolean;
	runTip: string;
	runVariant: 'default' | 'outline';
	laneTone: LaneTone;
}

export interface RowContext {
	queued: boolean;
	machineUuid: string | null;
	now: number;
	timeZone: string;
}

/** True for a one-machine job whose owner is a machine other than this one. */
export function runsOnAnotherMachine(job: ScheduledJob, machineUuid: string | null): boolean {
	return (
		job.scope === 'one_machine' &&
		job.owner_machine_uuid !== null &&
		machineUuid !== null &&
		job.owner_machine_uuid !== machineUuid
	);
}

function compactSpan(ms: number): string {
	const sec = Math.floor(ms / 1_000);
	if (sec < 60) return `${sec}s`;
	const min = Math.floor(sec / 60);
	if (min < 60) return `${min}m`;
	const hr = Math.floor(min / 60);
	if (hr < 24) return `${hr}h`;
	return `${Math.floor(hr / 24)}d`;
}

/** "5m ago" for a past time, "in 5m" for a future one, an em dash for no time. */
export function relativeTime(iso: string | null, now: number): string {
	const parsed = parseIso(iso);
	if (!parsed) return '—';
	const diff = parsed.getTime() - now;
	if (diff > 999) return `in ${compactSpan(diff)}`;
	return formatRelative(iso, now);
}

/** A run is in progress, or has been asked for and not yet picked up. */
export function isBusy(job: ScheduledJob, queuedHere: boolean): boolean {
	return (
		queuedHere ||
		job.state === 'running' ||
		job.running_since !== null ||
		job.run_requested_at !== null
	);
}

/** True while the job runs, as opposed to waiting to start. */
export function isRunning(job: ScheduledJob): boolean {
	return job.state === 'running' || job.running_since !== null;
}

/** A run that started at or after the missed due time answers an "overdue" verdict. */
function ranSinceDue(job: ScheduledJob): boolean {
	const started = parseIso(job.last_started_at);
	const due = parseIso(job.next_run_at);
	return started !== null && due !== null && started.getTime() >= due.getTime();
}

/** The one status a row shows; first match wins. */
export function jobStatus(
	job: ScheduledJob,
	ctx: Pick<RowContext, 'queued' | 'machineUuid'>
): JobStatus {
	if (job.state === 'unavailable') return 'unavailable';
	if (runsOnAnotherMachine(job, ctx.machineUuid)) return 'elsewhere';
	if (isRunning(job)) return 'running';
	if (ctx.queued || job.run_requested_at !== null) return 'queued';
	if (!job.enabled || job.state === 'off') return 'off';
	if (job.state === 'failing') return 'failing';
	if (job.state === 'overdue' && !ranSinceDue(job)) return 'overdue';
	if (job.state === 'never_run') return 'never_run';
	return 'ok';
}

function lastRanLine(job: ScheduledJob, status: JobStatus, now: number): string {
	const running = status === 'running';
	const startedIso = running ? (job.running_since ?? job.last_started_at) : job.last_started_at;
	if (!parseIso(startedIso)) return status === 'elsewhere' ? '—' : 'never ran';
	const by =
		job.last_trigger === 'manual'
			? ' · by hand'
			: job.last_trigger === 'schedule'
				? ' · on schedule'
				: '';
	return `${running ? 'started' : 'ran'} ${relativeTime(startedIso, now)}${by}`;
}

function nextLine(job: ScheduledJob, status: JobStatus, ctx: RowContext): string {
	if (status === 'unavailable') return 'no scripts folder here';
	if (status === 'elsewhere') return 'runs on another machine';
	if (status === 'off') return 'not scheduled';
	const next = parseIso(job.next_run_at);
	const due = next !== null && next.getTime() - ctx.now <= 999;
	if ((status === 'running' || status === 'queued') && due) return 'next after this run';
	if (!next) return 'next —';
	if (due) return 'due now';
	const rel = relativeTime(job.next_run_at, ctx.now);
	const clock = localClock(next.getTime(), ctx.timeZone);
	if (job.window?.period === 'week') {
		return `next ${zonedParts(next.getTime(), ctx.timeZone).weekday} ${clock} · ${rel}`;
	}
	return `next ${clock} · ${rel}`;
}

function statusTip(job: ScheduledJob, status: JobStatus, ctx: RowContext): string {
	switch (status) {
		case 'ok':
			return 'The last run succeeded and none is overdue.';
		case 'failing':
			return job.detail || 'The last run failed.';
		case 'overdue':
			return job.detail || 'A scheduled run is overdue.';
		case 'unavailable':
			return job.detail || 'This mike has no scripts folder to run the job from.';
		case 'never_run':
			return 'No run recorded yet.';
		case 'running': {
			const since = parseIso(job.running_since ?? job.last_started_at);
			return since
				? `Running since ${localClock(since.getTime(), ctx.timeZone)} local.`
				: 'Running now.';
		}
		case 'queued':
			return 'Asked to run; it starts within a few seconds.';
		case 'off':
			return 'Turned off. It does not run on its schedule.';
		case 'elsewhere':
			return 'Another machine runs this job; this one does not.';
	}
}

const RUN_TIP: Record<JobStatus, string> = {
	running: 'Already running.',
	queued: 'Already asked; it starts shortly.',
	off: 'Turned off. Turn it on with the switch to run it.',
	elsewhere: "Another machine runs this job. Run it from that machine's mike.",
	unavailable: 'This mike has no job scripts to run it with.',
	failing: 'Runs this job once now. Its schedule does not change.',
	overdue: 'Runs this job once now. Its schedule does not change.',
	never_run: 'Runs this job once now. Its schedule does not change.',
	ok: 'Runs this job once now. Its schedule does not change.'
};

const RUN_BLOCKED: readonly JobStatus[] = ['running', 'queued', 'off', 'elsewhere', 'unavailable'];

function laneTone(status: JobStatus): LaneTone {
	switch (status) {
		case 'unavailable':
		case 'off':
			return 'hollow';
		case 'running':
			return 'live';
		case 'failing':
			return 'danger';
		case 'overdue':
		case 'never_run':
			return 'warning';
		default:
			return 'neutral';
	}
}

/** Everything a row shows, derived once so its words never disagree. */
export function rowView(job: ScheduledJob, ctx: RowContext): RowView {
	const status = jobStatus(job, ctx);
	const look = STATUS_LOOK[status];
	const attention = ATTENTION.includes(status);
	return {
		status,
		icon: look.icon,
		word: look.word,
		tone: look.tone,
		spin: look.spin ?? false,
		statusTip: statusTip(job, status, ctx),
		attention,
		lastRan: lastRanLine(job, status, ctx.now),
		next: nextLine(job, status, ctx),
		runDisabled: RUN_BLOCKED.includes(status),
		runTip: RUN_TIP[status],
		runVariant: attention ? 'default' : 'outline',
		laneTone: laneTone(status)
	};
}

export const GROUP_ORDER = ['learning_and_memory', 'measurement', 'builds_and_tests'] as const;

export const GROUP_LABEL: Record<ScheduledJobGroup | 'other', string> = {
	learning_and_memory: 'Learning and memory',
	measurement: 'Measurement',
	builds_and_tests: 'Builds and tests',
	other: 'Other'
};

export interface JobGroup {
	key: ScheduledJobGroup | 'other';
	label: string;
	jobs: ScheduledJob[];
}

/** An immutable order: where the window starts on the axis, then label, then id. */
export function compareJobs(a: ScheduledJob, b: ScheduledJob): number {
	const byStart = windowStart(a.window) - windowStart(b.window);
	if (byStart !== 0 && Number.isFinite(byStart)) return byStart;
	const aNone = !Number.isFinite(windowStart(a.window));
	const bNone = !Number.isFinite(windowStart(b.window));
	if (aNone !== bNone) return aNone ? 1 : -1;
	return a.label.localeCompare(b.label) || a.id.localeCompare(b.id);
}

/** Groups in a fixed order; a job with no or an unknown group goes last, under Other. */
export function groupJobs(jobs: readonly ScheduledJob[]): JobGroup[] {
	const known = new Set<string>(GROUP_ORDER);
	const keys: (ScheduledJobGroup | 'other')[] = [...GROUP_ORDER, 'other'];
	return keys
		.map((key) => ({
			key,
			label: GROUP_LABEL[key],
			jobs: jobs
				.filter((job) => (key === 'other' ? !known.has(job.group ?? '') : job.group === key))
				.sort(compareJobs)
		}))
		.filter((group) => group.jobs.length > 0);
}
