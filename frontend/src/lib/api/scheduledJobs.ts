import { ApiError } from '$lib/api';

export type ScheduledJobState =
	'ok' | 'failing' | 'overdue' | 'never_run' | 'off' | 'running' | 'unavailable';

export type ScheduledJobScope = 'this_machine' | 'one_machine';

export type ScheduledJobTrigger = 'schedule' | 'manual';

export type ScheduledJobGroup = 'learning_and_memory' | 'measurement' | 'builds_and_tests';

export interface ScheduledJobWindow {
	period: 'hour' | 'day' | 'week';
	/** "Mon".."Sun" for a weekly job, else null. */
	weekday: string | null;
	/** Hours in window order (may wrap midnight, e.g. [20,21,22,23,0,1,2,3]). */
	hours_utc: number[];
	minute: number;
}

export interface ScheduledJob {
	id: string;
	label: string;
	description: string;
	schedule: string;
	scope: ScheduledJobScope;
	owner_machine_uuid: string | null;
	enabled: boolean;
	state: ScheduledJobState;
	last_started_at: string | null;
	last_finished_at: string | null;
	last_exit_code: number | null;
	last_output_tail: string | null;
	next_run_at: string | null;
	/** Set while a run is in progress. */
	running_since: string | null;
	/** Set from a "Run now" press until the worker picks the request up. */
	run_requested_at: string | null;
	/** What started the last run; null when there is no recorded run. */
	last_trigger: ScheduledJobTrigger | null;
	consecutive_failures: number;
	detail: string;
	fix: string;
	/** Which block of the panel the job sits in; null from a proxy that does not say. */
	group: ScheduledJobGroup | null;
	/** When the job may start, in UTC; null from a proxy that does not say. */
	window: ScheduledJobWindow | null;
	/** The setting the on/off switch writes; null from a proxy that does not say. */
	enabled_key: string | null;
}

export interface ScheduledJobsResponse {
	machine_uuid: string | null;
	machine_name: string | null;
	jobs: ScheduledJob[];
}

export async function getScheduledJobs(): Promise<ScheduledJobsResponse> {
	const path = '/api/scheduled-jobs';
	const res = await fetch(path, { headers: { accept: 'application/json' } });
	if (!res.ok) {
		throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	const data = (await res.json()) as Partial<ScheduledJobsResponse>;
	return {
		machine_uuid: data.machine_uuid ?? null,
		machine_name: data.machine_name ?? null,
		jobs: (data.jobs ?? []).map((job) => ({
			...job,
			running_since: job.running_since ?? null,
			run_requested_at: job.run_requested_at ?? null,
			last_trigger: job.last_trigger ?? null,
			group: job.group ?? null,
			window: job.window ?? null,
			enabled_key: job.enabled_key ?? null
		}))
	};
}

export interface RunJobAccepted {
	job: string;
	run_requested_at: string;
	already_requested: boolean;
}

/**
 * Asks the proxy to run one job now. The browser adds the Origin header the route checks.
 * A refusal (409) throws an ApiError whose message is the proxy's own `error` text.
 */
export async function runScheduledJob(id: string): Promise<void> {
	const path = `/api/scheduled-jobs/${encodeURIComponent(id)}/run`;
	const res = await fetch(path, { method: 'POST', headers: { accept: 'application/json' } });
	if (res.ok) return;
	let message = `POST ${path} failed: ${res.status} ${res.statusText}`;
	try {
		const body = (await res.json()) as { error?: string };
		if (body.error) message = body.error;
	} catch {
		// The body was not JSON; the status line above stands.
	}
	throw new ApiError(message, res.status, path);
}
