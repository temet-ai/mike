import { ApiError } from '$lib/api';
import { cachedGet, invalidateApiCache } from '$lib/stores/cache/apiCache.svelte';

export interface EvalRollupDto {
	id: number;
	for_day: string;
	sql_version: string;
	mode: string;
	labels_as_of: string;
	started_at: string;
	finished_at: string | null;
	rows_written: number | null;
}

export interface EvalRateDto {
	id: number;
	rollup_id: number;
	for_day: string;
	family: string;
	metric: string;
	population: string;
	scope_kind: string;
	project_id: number | null;
	window_start: string | null;
	window_end: string;
	window_days: number | null;
	role: string;
	bundle_id: number | null;
	bundle_label: string | null;
	axis: string;
	class: string;
	label_tier: string | null;
	label_split: string | null;
	label_source: string | null;
	dimension: string | null;
	dimension_value: string | null;
	steerer_id: number | null;
	question: string | null;
	mode: string | null;
	k: number;
	n: number;
	value: number | null;
	ci_low: number | null;
	ci_high: number | null;
	weighted: boolean;
	n_effective: number | null;
	detail: Record<string, unknown>;
}

export interface EvalRatesResponse {
	rollup: EvalRollupDto | null;
	total: number;
	limit: number;
	offset: number;
	rows: EvalRateDto[];
}

export interface EvalAlarmDto {
	id: number;
	kind: string;
	subject: string;
	first_seen_at: string;
	last_seen_at: string;
	cleared_at: string | null;
	evaluations: number;
	quiet_evaluations: number;
	detail: Record<string, unknown>;
	acknowledged_at: string | null;
	ack_note: string | null;
}

export interface EvalAlarmsResponse {
	total: number;
	limit: number;
	offset: number;
	alarms: EvalAlarmDto[];
}

export type RatesQuery = Partial<{
	family: string;
	role: string;
	axis: string;
	class: string;
	label_tier: string;
	label_split: string;
	label_source: string;
	window_days: number;
	latest: boolean;
	limit: number;
	offset: number;
}>;

export type AlarmsQuery = Partial<{
	state: 'open' | 'cleared' | 'all';
	kind: string;
	limit: number;
	offset: number;
}>;

export function queryString(params: Record<string, string | number | boolean | undefined>): string {
	const entries = Object.entries(params)
		.filter(([, v]) => v !== undefined)
		.sort(([a], [b]) => a.localeCompare(b))
		.map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(String(v))}`);
	return entries.length ? `?${entries.join('&')}` : '';
}

async function getJson<T>(path: string): Promise<T> {
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as T;
	});
}

export async function getEvalRates(query: RatesQuery = {}): Promise<EvalRatesResponse> {
	return getJson<EvalRatesResponse>(`/api/eval/rates${queryString(query)}`);
}

export async function getEvalAlarms(query: AlarmsQuery = {}): Promise<EvalAlarmsResponse> {
	return getJson<EvalAlarmsResponse>(`/api/eval/alarms${queryString(query)}`);
}

export async function acknowledgeEvalAlarm(id: number, note: string): Promise<EvalAlarmDto> {
	const path = `/api/eval/alarms/${id}/acknowledge`;
	const res = await fetch(path, {
		method: 'PATCH',
		headers: { 'content-type': 'application/json', accept: 'application/json' },
		body: JSON.stringify({ note })
	});
	if (!res.ok) {
		throw new ApiError(`PATCH ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	await invalidateApiCache('/api/eval/alarms');
	return (await res.json()) as EvalAlarmDto;
}

export function formatShare(value: number | null): string {
	return value === null ? '—' : `${(value * 100).toFixed(1)}%`;
}

export interface RejudgeRunDto {
	id: number;
	role: string;
	bundle_id: number;
	window_since: string;
	window_until: string;
	daily_call_cap: number;
	status: 'queued' | 'running' | 'done' | 'cancelled' | string;
	estimated_calls: number;
	estimated_usd: number | null;
	created_by: string;
	created_at: string;
	started_at: string | null;
	finished_at: string | null;
	eligible_total: number;
	ineligible_total: number;
	judged: number;
	permanent_failures: number;
	remaining: number;
	calls_today: number;
	cost_usd_so_far: number;
	ineligible_reprojected: number;
	projected_finish_date: string | null;
}

export async function getRejudgeRuns(): Promise<RejudgeRunDto[]> {
	const body = await getJson<{ runs: RejudgeRunDto[] }>('/api/eval/rejudge-runs');
	return body.runs;
}

export function rejudgeSummary(run: RejudgeRunDto | undefined): string {
	if (!run) return 'No history re-judge has been started.';
	const day = run.projected_finish_date?.split('T')[0];
	const finish = day ? `, finishing about ${day}` : '';
	return `Run ${run.id} ${run.status}: ${run.judged} of ${run.eligible_total} judged, $${run.cost_usd_so_far.toFixed(2)} so far${finish}`;
}
