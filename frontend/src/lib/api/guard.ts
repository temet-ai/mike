import { ApiError } from '$lib/api';

export type GuardQuestion = 'relapse' | 'unbacked_wait' | 'authorised_handback' | 'unobserved_done';
export type GuardMode = 'off' | 'shadow' | 'append' | 'wake';

export interface GuardQuestionDto {
	question: GuardQuestion;
	mode: GuardMode;
	confidence_threshold: number | null;
	floor: number | null;
	bar: number;
	dependency_state: string;
	counts_24h: { rows: number; would_fire: number };
}

export interface GuardSummary {
	enabled: boolean;
	steerer_id: number;
	steerer_enabled: boolean;
	rubric_version: string;
	gate_version: string;
	steering_facts_version: number;
	labeller_version: string;
	labeller: {
		state: 'ok' | 'backing_off' | 'breaker_open';
		since: string | null;
		consecutive_failures: number;
	};
	settings: {
		max_calls_per_day: number;
		max_label_calls_per_day: number;
		recall_sample_pct: number;
	};
	questions: GuardQuestionDto[];
}

export interface Interval {
	k: number;
	n: number;
	value: number | null;
	low: number | null;
	high: number | null;
}

export interface GuardQuestionMetrics {
	question: string;
	trigger_point: string;
	funnel: Record<string, unknown> & { rows: number; would_fire: number; labelled: number };
	precision: Interval;
	recall: number | null;
	recall_ci: [number, number] | null;
	first_vote_precision: Interval;
	reach_share: Interval;
	bar: number;
	daily_cap_excluded: number;
}

export interface GuardMetrics {
	since: string;
	until: string;
	metrics: GuardQuestionMetrics[];
	audit: { audit_set_sessions: number; audit_set_fires: number; offending_evaluations: string[] };
}

export const WRITABLE_MODES: GuardMode[] = ['off', 'shadow'];

export function formatRate(i: Interval): string {
	if (i.value === null) return `n = ${i.n}`;
	const pct = (v: number | null) => (v === null ? '?' : `${Math.round(v * 100)}%`);
	return `${pct(i.value)} (${pct(i.low)} to ${pct(i.high)}, n = ${i.n})`;
}

async function send<T>(method: string, path: string, body?: unknown): Promise<T> {
	const res = await fetch(path, {
		method,
		headers: { accept: 'application/json', 'content-type': 'application/json' },
		body: body === undefined ? undefined : JSON.stringify(body)
	});
	if (!res.ok) {
		let message = `${method} ${path} failed: ${res.status} ${res.statusText}`;
		try {
			const parsed = (await res.json()) as { error?: string };
			if (parsed.error) message = parsed.error;
		} catch {
			message = `${method} ${path} failed: ${res.status}`;
		}
		throw new ApiError(message, res.status, path);
	}
	return (await res.json()) as T;
}

export function getGuard(): Promise<GuardSummary> {
	return send<GuardSummary>('GET', '/api/steering/guard');
}

export function getGuardMetrics(): Promise<GuardMetrics> {
	return send<GuardMetrics>('GET', '/api/steering/guard/metrics');
}

export function setGuardMode(question: GuardQuestion, mode: GuardMode): Promise<unknown> {
	return send('PATCH', `/api/steering/guard/questions/${question}`, { mode });
}

export function setGuardToggle(enabled: boolean): Promise<unknown> {
	return send('PATCH', '/api/settings/features/turn_guard', { enabled });
}
