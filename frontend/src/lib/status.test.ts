import { describe, expect, it } from 'vitest';
import {
	classifyRequestStatus,
	deriveSessionStatus,
	isAbortedRequest,
	isLiveStreamingEvent,
	isRequestLive,
	isStreamingLive,
	type SessionStatus
} from './status';

describe('deriveSessionStatus', () => {
	it('REST-only: Complete / Streaming / Pending / Error', () => {
		expect(deriveSessionStatus('Complete').kind).toBe('complete');
		expect(deriveSessionStatus('Streaming').kind).toBe('streaming');
		expect(deriveSessionStatus('Pending').kind).toBe('pending');
		expect(deriveSessionStatus('Error: timeout').kind).toBe('error');
		expect(deriveSessionStatus('Error: timeout').label).toBe('Error');
		expect(deriveSessionStatus('SomethingNew').kind).toBe('complete');
	});

	it('Streaming labels show "In flight"', () => {
		expect(deriveSessionStatus('Streaming').label).toBe('In flight');
		expect(deriveSessionStatus('Pending').label).toBe('In flight');
	});

	it('ResponseComplete overrides REST to complete', () => {
		const s = deriveSessionStatus('Streaming', {
			type: 'ResponseComplete',
			id: 1,
			stop_reason: null,
			usage: null
		});
		expect(s.kind).toBe('complete');
	});

	it('ResponseError overrides REST to error', () => {
		const s = deriveSessionStatus('Complete', {
			type: 'ResponseError',
			id: 1,
			error: 'rate_limited'
		});
		expect(s.kind).toBe('error');
		expect(s.raw).toBe('Error: rate_limited');
	});

	it('streaming deltas override REST to streaming', () => {
		for (const ev of [
			{ type: 'TextDelta' as const, id: 1, text: 'hi' },
			{ type: 'ThinkingDelta' as const, id: 1, thinking: '...' },
			{ type: 'ToolUseStart' as const, id: 1, tool_id: 't1', tool_name: 'bash' },
			{ type: 'ToolInputDelta' as const, id: 1, partial_json: '{}' },
			{ type: 'ToolUseEnd' as const, id: 1 }
		]) {
			expect(deriveSessionStatus('Complete', ev).kind).toBe('streaming');
		}
	});

	it('informational events do NOT override REST status', () => {
		expect(
			deriveSessionStatus('Complete', { type: 'ModelConfirmed', id: 1, model: 'x' }).kind
		).toBe('complete');
		expect(
			deriveSessionStatus('Streaming', {
				type: 'UsageReceived',
				id: 1,
				usage: {
					input_tokens: 1,
					output_tokens: 1,
					cache_creation_input_tokens: null,
					cache_read_input_tokens: null
				}
			}).kind
		).toBe('streaming');
		expect(
			deriveSessionStatus('Streaming', { type: 'DiagnosticReady', id: 1, count: 0 }).kind
		).toBe('streaming');
		expect(deriveSessionStatus('Complete', { type: 'Lagged', skipped: 5 }).kind).toBe('complete');
	});

	it('null wsEvent behaves like no override', () => {
		expect(deriveSessionStatus('Complete', null).kind).toBe('complete');
	});

	it('unknown event type falls through to REST status via default branch', () => {
		// eslint-disable-next-line @typescript-eslint/no-explicit-any
		const fakeEvent = { type: 'SomeNewFutureEvent', id: 42 } as any;
		expect(deriveSessionStatus('Streaming', fakeEvent).kind).toBe('streaming');
		expect(deriveSessionStatus('Complete', fakeEvent).kind).toBe('complete');
	});
});

describe('classifyRequestStatus', () => {
	it('classifies all 5 variants correctly', () => {
		expect(classifyRequestStatus('Complete')).toBe('complete');
		expect(classifyRequestStatus('Streaming')).toBe('streaming');
		expect(classifyRequestStatus('Pending')).toBe('pending');
		expect(classifyRequestStatus('Error: timeout')).toBe('error');
		expect(classifyRequestStatus('Error')).toBe('error');
		expect(classifyRequestStatus('SomethingUnexpected')).toBe('unknown');
	});
});

describe('isRequestLive', () => {
	it('returns true for Streaming', () => {
		expect(isRequestLive('Streaming')).toBe(true);
	});

	it('returns true for Pending', () => {
		expect(isRequestLive('Pending')).toBe(true);
	});

	it('returns false for Complete (terminal state)', () => {
		expect(isRequestLive('Complete')).toBe(false);
	});

	it('returns false for Error variant (terminal state)', () => {
		expect(isRequestLive({ Error: 'rate_limited' })).toBe(false);
	});
});

describe('isAbortedRequest', () => {
	it('returns true for the stream-aborted Error variant', () => {
		expect(isAbortedRequest({ Error: 'stream aborted' })).toBe(true);
	});

	it('is case- and whitespace-insensitive on the abort message', () => {
		expect(isAbortedRequest({ Error: '  Stream Aborted  ' })).toBe(true);
	});

	it('returns false for a non-abort Error variant', () => {
		expect(isAbortedRequest({ Error: 'rate_limited' })).toBe(false);
	});

	it('returns false for terminal Complete', () => {
		expect(isAbortedRequest('Complete')).toBe(false);
	});

	it('returns false for in-flight Pending / Streaming', () => {
		expect(isAbortedRequest('Pending')).toBe(false);
		expect(isAbortedRequest('Streaming')).toBe(false);
	});
});

describe('isLiveStreamingEvent', () => {
	it('true for streaming-delta / tool WS events', () => {
		expect(isLiveStreamingEvent({ type: 'TextDelta', id: 1, text: 'hi' })).toBe(true);
		expect(isLiveStreamingEvent({ type: 'ThinkingDelta', id: 1, thinking: '...' })).toBe(true);
		expect(
			isLiveStreamingEvent({ type: 'ToolUseStart', id: 1, tool_id: 't1', tool_name: 'bash' })
		).toBe(true);
		expect(isLiveStreamingEvent({ type: 'ToolInputDelta', id: 1, partial_json: '{}' })).toBe(true);
		expect(isLiveStreamingEvent({ type: 'ToolUseEnd', id: 1 })).toBe(true);
	});

	it('false for terminal / informational events and null', () => {
		expect(
			isLiveStreamingEvent({ type: 'ResponseComplete', id: 1, stop_reason: null, usage: null })
		).toBe(false);
		expect(isLiveStreamingEvent({ type: 'ResponseError', id: 1, error: 'x' })).toBe(false);
		expect(isLiveStreamingEvent({ type: 'ModelConfirmed', id: 1, model: 'x' })).toBe(false);
		expect(isLiveStreamingEvent(null)).toBe(false);
		expect(isLiveStreamingEvent(undefined)).toBe(false);
	});
});

describe('isStreamingLive', () => {
	const STALE = 3 * 60 * 1000;
	const streaming: SessionStatus = { kind: 'streaming', label: 'In flight', raw: 'Streaming' };
	const pending: SessionStatus = { kind: 'pending', label: 'In flight', raw: 'Pending' };
	const complete: SessionStatus = { kind: 'complete', label: 'Complete', raw: 'Complete' };

	it('non-streaming/pending statuses are never live', () => {
		expect(isStreamingLive(complete, null, 1000, 1000, STALE)).toBe(false);
		const error: SessionStatus = { kind: 'error', label: 'Error', raw: 'Error: x' };
		expect(isStreamingLive(error, null, 1000, 1000, STALE)).toBe(false);
	});

	it('fresh streaming/pending (last_seen within window) is live', () => {
		const now = 10 * 60 * 1000;
		expect(isStreamingLive(streaming, null, now - 30_000, now, STALE)).toBe(true);
		expect(isStreamingLive(pending, null, now - 60_000, now, STALE)).toBe(true);
	});

	it('STALE streaming/pending (last_seen beyond window) is NOT live — the 30-min-spinner bug', () => {
		const now = 60 * 60 * 1000;
		expect(isStreamingLive(streaming, null, now - 30 * 60 * 1000, now, STALE)).toBe(false);
		expect(isStreamingLive(pending, null, now - 5 * 60 * 1000, now, STALE)).toBe(false);
	});

	it('live WS streaming frames override staleness — genuinely in-flight long request', () => {
		const now = 60 * 60 * 1000;
		const oldSeen = now - 30 * 60 * 1000;
		expect(
			isStreamingLive(streaming, { type: 'TextDelta', id: 1, text: 'hi' }, oldSeen, now, STALE)
		).toBe(true);
	});

	it('null last_seen with no live WS event is not live', () => {
		expect(isStreamingLive(streaming, null, null, 1000, STALE)).toBe(false);
	});
});
