import { ApiError } from '$lib/api';

export interface CalibrationResponse {
	snapshot: {
		settings: { input_mode: string; threshold_mode: string; calibration_min_labelled: number };
		rollup: { id: number; finished_at: string } | null;
		rows: { family: string; question: string | null; scope_kind: string; dimension: string | null; dimension_value: string | null; k: number; n: number; ci_low: number | null; ci_high: number | null }[];
		thresholds: { id: number; question: string; scope: string; applied_threshold: number; computed_at: string }[];
	};
	block_if_live: { text: string } | null;
}

export async function getCalibration(): Promise<CalibrationResponse> {
	const path = '/api/cross-session/calibration';
	const res = await fetch(path, { headers: { accept: 'application/json' } });
	if (!res.ok) throw new ApiError(`GET ${path} failed: ${res.status}`, res.status, path);
	return (await res.json()) as CalibrationResponse;
}

export function candidateRows(r: CalibrationResponse) {
	return r.snapshot.rows.filter((row) => row.family === 'steer_calibration' && row.dimension === 'all');
}
