import type {
	SessionSummary,
	SessionDetail,
	UnifiedTextMatch,
	SearchMatchSnippet
} from '$lib/types';
import { buildRequestSummary } from './request';
import type { DetailMetrics } from '$lib/components/sessions/sessionRowTypes';

const STORY_SESSION_ID = 'storybook-session-1';
const STORY_PROJECT = '/Users/dev/myproject';

export function buildUnifiedTextMatch(overrides: Partial<UnifiedTextMatch> = {}): UnifiedTextMatch {
	return {
		request_id: 4242,
		session_id: '11111111-0000-0000-0000-000000000001',
		source: 'response',
		byte_offset: 1024,
		match_length: 5,
		line: 12,
		column: 7,
		before: 'the migration runner returned an ',
		matched: 'error',
		after: ' because the checksum column drifted from the file on disk.',
		...overrides
	};
}

export function buildSearchMatchSnippet(
	overrides: Partial<SearchMatchSnippet> = {}
): SearchMatchSnippet {
	return {
		request_id: 4242,
		line: 12,
		column: 7,
		before: 'the migration runner returned an ',
		matched: 'error',
		after: ' because the checksum column drifted from the file on disk.',
		provenance: 'response',
		provenanceTitle: 'Matched in the captured response body',
		...overrides
	};
}

export function buildSessionSummary(overrides: Partial<SessionSummary> = {}): SessionSummary {
	return {
		id: STORY_SESSION_ID,
		project: STORY_PROJECT,
		project_id: 1,
		cwd: STORY_PROJECT,
		project_resolution: 'git',
		first_seen: '2026-04-17T11:00:00Z',
		last_seen: '2026-04-17T12:05:00Z',
		request_count: 5,
		models: ['claude-opus-4-7'],
		last_status: 'Complete',
		title: null,
		pinned: false,
		avg_latency_ms: null,
		...overrides
	};
}

export function buildSessionDetail(
	overrides: Partial<SessionDetail> & { requestCount?: number } = {}
): SessionDetail {
	const { requestCount = 5, ...rest } = overrides;
	const requests =
		rest.requests ??
		Array.from({ length: requestCount }, (_, i) =>
			buildRequestSummary({
				id: 100 + i,
				session_id: STORY_SESSION_ID,
				project: STORY_PROJECT,
				created_at: `2026-04-17T12:0${i}:00Z`,
				completed_at: `2026-04-17T12:0${i}:03Z`
			})
		);
	return {
		summary: buildSessionSummary({ request_count: requests.length }),
		requests,
		total_requests: requests.length,
		offset: 0,
		limit: 50,
		...rest
	};
}

export function buildDetailMetrics(overrides: Partial<DetailMetrics> = {}): DetailMetrics {
	return {
		loading: false,
		error: null,
		model: 'claude-opus-4-7',
		inTokens: 12_000,
		outTokens: 800,
		cachePct: 74,
		costStr: '$0.42',
		costEstimated: false,
		stop: 'end_turn',
		partial: false,
		...overrides
	};
}

export { STORY_SESSION_ID, STORY_PROJECT };
