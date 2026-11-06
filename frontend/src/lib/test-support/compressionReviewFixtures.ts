import type { CompressionReview } from '$lib/api/compressionReview';
export function compressionReviewFixture(): CompressionReview {
	return {
		id: 'fixture-flag',
		target: { kind: 'tool_result', request_id: 473363, tool_use_id: 808536 },
		status: 'open',
		created_at: '2026-10-04T12:00:00Z',
		updated_at: '2026-10-04T12:00:00Z',
		capture_verified: false,
		history: [
			{
				action: 'flag',
				at: '2026-10-04T12:00:00Z',
				actor: 'inspect-ui',
				reason: 'Test result needed panic details',
				evidence: 'Follow-up result request 473366'
			}
		]
	};
}
