import type { CaptureGapHealth, HealthResponse } from '$lib/types';

export function buildCaptureGapHealth(overrides: Partial<CaptureGapHealth> = {}): CaptureGapHealth {
	return {
		silence_threshold_secs: 300,
		sidecar_recent_secs: 120,
		lookback_secs: 1800,
		alarm: false,
		sessions: [],
		reading_age_secs: 0,
		...overrides
	};
}

export function buildHealthResponse(overrides: Partial<HealthResponse> = {}): HealthResponse {
	return { status: 'ok', capture_gap: buildCaptureGapHealth(), ...overrides };
}

export function buildDegradedCaptureHealth(): HealthResponse {
	return buildHealthResponse({
		status: 'degraded',
		capture_gap: buildCaptureGapHealth({
			alarm: true,
			reading_age_secs: 3,
			sessions: [
				{
					session_id: 'example-session-capture-gap',
					last_work_at: null,
					last_sidecar_at: '2026-10-13T12:00:00Z',
					work_silent_secs: 420
				}
			]
		})
	});
}
