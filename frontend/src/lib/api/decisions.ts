import { ApiError } from '$lib/api';
import { cachedGet } from '$lib/stores/cache/apiCache.svelte';

export type DecisionMode = 'off' | 'shadow' | 'on';

export interface DecisionLayer {
	on: boolean;
	reason: string | null;
}

export interface PointCounts {
	point: string;
	calls: number;
	applied_on: number;
	fail_open: number;
	schema_errors: number;
	refused: number;
	question_refusals: number;
	cost_usd: number;
	latency_p50_ms: number | null;
	latency_p95_ms: number | null;
	labelled: number;
	labelled_last_7d: number;
}

export interface PointScore {
	counts: PointCounts;
	auroc: number | null;
	held_labelled: number;
	held_wrong: number;
	held_precision_lower: number | null;
	sent_labelled: number;
	sent_right: number;
	labels_per_day: number;
	days_to_min_labels: number | null;
	applied_labelled: number;
	applied_right: number;
	applied_precision_lower: number | null;
	findings: string[];
}

export interface DecisionPointRow {
	point: string;
	label: string | null;
	area: string;
	mode: string;
	mode_source: string;
	score: PointScore | null;
}

export interface DecisionSpend {
	today_usd: number;
	ceiling_usd: number;
	reached: boolean;
}

export interface DecisionsCalibration {
	layer: DecisionLayer;
	spend?: DecisionSpend;
	window_days: number;
	last_loop_at: string | null;
	points: DecisionPointRow[];
}

export const TURN_GUARD_POINT = 'turn_guard_precheck';

export function modeSettingKey(point: string): string {
	return point === TURN_GUARD_POINT ? 'decisions.turn_guard.mode' : `decisions.${point}.mode`;
}

export function modeIsSetting(row: DecisionPointRow): boolean {
	return row.mode_source === 'setting' || row.mode_source === 'decisions.turn_guard.mode';
}

export function offReason(layer: DecisionLayer): string {
	return (layer.reason ?? 'off').replace(/^off:\s*/, '');
}

export async function getDecisionsCalibration(): Promise<DecisionsCalibration> {
	const path = '/api/decisions/calibration';
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			let message = `GET ${path} failed: ${res.status} ${res.statusText}`;
			try {
				const body = (await res.json()) as { error?: string };
				if (typeof body.error === 'string' && body.error.length > 0) message = body.error;
			} catch {
				// A body that is not JSON keeps the status line as the message.
			}
			throw new ApiError(message, res.status, path);
		}
		return (await res.json()) as DecisionsCalibration;
	});
}
