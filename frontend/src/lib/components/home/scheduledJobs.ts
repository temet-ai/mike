import type { ScheduledJob, ScheduledJobState } from '$lib/api/scheduledJobs';
import type { BadgeVariant } from '$lib/components/shared/Badge.variants';
import { formatRelative, parseIso } from '$lib/time';

export interface StateView {
	label: string;
	variant: BadgeVariant;
	/** The row shows its detail and fix inline and can be expanded. */
	needsAttention: boolean;
	/** Semantic border token for the row, so the state reads without colour in the text. */
	border: string;
}

export const STATE_VIEW: Record<ScheduledJobState, StateView> = {
	ok: {
		label: 'ok',
		variant: 'chip-active',
		needsAttention: false,
		border: 'var(--border-subtle)'
	},
	running: {
		label: 'running',
		variant: 'chip-analyzed',
		needsAttention: false,
		border: 'var(--border-subtle)'
	},
	off: {
		label: 'off',
		variant: 'chip-muted',
		needsAttention: false,
		border: 'var(--border-subtle)'
	},
	failing: {
		label: 'failing',
		variant: 'chip-error',
		needsAttention: true,
		border: 'var(--status-danger-border)'
	},
	overdue: {
		label: 'overdue',
		variant: 'chip-warning',
		needsAttention: true,
		border: 'var(--status-warning-border)'
	},
	never_run: {
		label: 'never run',
		variant: 'chip-warning',
		needsAttention: true,
		border: 'var(--status-warning-border)'
	}
};

export function stateView(state: ScheduledJobState): StateView {
	return STATE_VIEW[state] ?? STATE_VIEW.off;
}

/**
 * A fixed order: label, then id. Never the last-run time, which changes under the reader.
 */
export function sortJobs(jobs: readonly ScheduledJob[]): ScheduledJob[] {
	return [...jobs].sort((a, b) => a.label.localeCompare(b.label) || a.id.localeCompare(b.id));
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

export function lastRanText(job: ScheduledJob, now: number): string {
	if (!parseIso(job.last_started_at)) return 'never ran';
	const byHand = job.last_trigger === 'manual' ? ', by hand' : '';
	if (job.state === 'running') return `started ${relativeTime(job.last_started_at, now)}${byHand}`;
	return `last ran ${relativeTime(job.last_started_at, now)}${byHand}`;
}

/** A run is in progress, or has been asked for and not yet picked up. */
export function isBusy(job: ScheduledJob, queuedHere: boolean): boolean {
	return (
		queuedHere || job.state === 'running' || job.running_since !== null || job.run_requested_at !== null
	);
}

/** True while the job runs, as opposed to waiting to start. */
export function isRunning(job: ScheduledJob): boolean {
	return job.state === 'running' || job.running_since !== null;
}

export function isTurnedOff(job: ScheduledJob): boolean {
	return !job.enabled || job.state === 'off';
}

/** The fix for a refused or lost run is to run it again: failing, overdue and never-run rows. */
export function runIsTheFix(job: ScheduledJob): boolean {
	return stateView(job.state).needsAttention;
}

export function nextText(job: ScheduledJob, now: number): string {
	const next = parseIso(job.next_run_at);
	if (!job.enabled || job.state === 'off') return 'not scheduled';
	if (!next) return 'next —';
	if (next.getTime() - now <= 999) return 'next due now';
	return `next ${relativeTime(job.next_run_at, now)}`;
}
