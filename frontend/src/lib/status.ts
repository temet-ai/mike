import type { ProxyEvent, RequestStatus } from './types';

export type StatusKind = 'complete' | 'streaming' | 'pending' | 'error' | 'unknown';

export interface SessionStatus {
	kind: StatusKind;
	label: string;
	raw: string;
}

function classifyRestStatus(status: string): SessionStatus {
	if (status === 'Complete') return { kind: 'complete', label: 'Complete', raw: status };
	if (status === 'Streaming') return { kind: 'streaming', label: 'In flight', raw: status };
	if (status === 'Pending') return { kind: 'pending', label: 'In flight', raw: status };
	if (status.startsWith('Error')) return { kind: 'error', label: 'Error', raw: status };
	return { kind: 'complete', label: status, raw: status };
}

export function deriveSessionStatus(
	lastStatus: string,
	wsEvent?: ProxyEvent | null
): SessionStatus {
	if (!wsEvent) return classifyRestStatus(lastStatus);

	switch (wsEvent.type) {
		case 'ResponseComplete':
			return { kind: 'complete', label: 'Complete', raw: 'Complete' };
		case 'ResponseError':
			return { kind: 'error', label: 'Error', raw: `Error: ${wsEvent.error}` };
		case 'TextDelta':
		case 'ThinkingDelta':
		case 'ToolUseStart':
		case 'ToolInputDelta':
		case 'ToolUseEnd':
			return { kind: 'streaming', label: 'In flight', raw: 'Streaming' };
		case 'ModelConfirmed':
		case 'UsageReceived':
		case 'DiagnosticReady':
		case 'Lagged':
			return classifyRestStatus(lastStatus);
		default:
			return classifyRestStatus(lastStatus);
	}
}

const LIVE_STREAMING_EVENT_TYPES = new Set<ProxyEvent['type']>([
	'TextDelta',
	'ThinkingDelta',
	'ToolUseStart',
	'ToolInputDelta',
	'ToolUseEnd'
]);

export function isLiveStreamingEvent(ev?: ProxyEvent | null): boolean {
	return ev != null && LIVE_STREAMING_EVENT_TYPES.has(ev.type);
}

export function isStreamingLive(
	status: SessionStatus,
	wsEvent: ProxyEvent | null | undefined,
	lastSeenMs: number | null,
	now: number,
	staleAfterMs: number
): boolean {
	if (status.kind !== 'streaming' && status.kind !== 'pending') return false;
	if (isLiveStreamingEvent(wsEvent)) return true;
	if (lastSeenMs === null) return false;
	return now - lastSeenMs < staleAfterMs;
}

export function classifyRequestStatus(status: string): StatusKind {
	if (status === 'Complete') return 'complete';
	if (status === 'Streaming') return 'streaming';
	if (status === 'Pending') return 'pending';
	if (status.startsWith('Error')) return 'error';
	return 'unknown';
}

export function isRequestLive(status: RequestStatus): boolean {
	return status === 'Streaming' || status === 'Pending';
}

export function isAbortedRequest(status: RequestStatus): boolean {
	return (
		typeof status === 'object' &&
		status !== null &&
		'Error' in status &&
		status.Error.trim().toLowerCase() === 'stream aborted'
	);
}

export function isNonAbortedStreamingError(status: RequestStatus): boolean {
	return (
		typeof status === 'object' && status !== null && 'Error' in status && !isAbortedRequest(status)
	);
}
