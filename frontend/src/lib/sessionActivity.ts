import type { ProxyEvent, SessionSummary } from '$lib/types';
import { deriveSessionStatus, isStreamingLive, type SessionStatus } from '$lib/status';
import { parseIso } from '$lib/time';
import { RECENT_ACTIVE_WINDOW_MS, LIVE_STREAMING_STALE_MS } from '$lib/stores/sessions.svelte';

export function sessionLastSeenMs(s: SessionSummary): number | null {
	const d = parseIso(s.last_seen);
	return d ? d.getTime() : null;
}

export interface SessionActivity {
	status: SessionStatus;
	isLive: boolean;
	isActive: boolean;
}

export function deriveSessionActivity(
	s: SessionSummary,
	wsEvent: ProxyEvent | null,
	now: number
): SessionActivity {
	const status = deriveSessionStatus(s.last_status, wsEvent);
	const seenMs = sessionLastSeenMs(s);
	const isLive = isStreamingLive(status, wsEvent, seenMs, now, LIVE_STREAMING_STALE_MS);
	const isActive = isLive || (seenMs !== null && now - seenMs < RECENT_ACTIVE_WINDOW_MS);
	return { status, isLive, isActive };
}
