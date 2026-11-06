export type ReviewKind = 'request' | 'response' | 'tool_result';
export type ReviewStatus = 'open' | 'investigating' | 'evaluated' | 'dismissed';
export interface CompressionTarget {
	kind: ReviewKind;
	request_id: number;
	tool_use_id?: number | null;
}
export interface ReviewEvent {
	action: 'flag' | 'review';
	at: string;
	actor: string;
	reason?: string;
	evidence?: string;
	note?: string;
	status?: ReviewStatus;
}
export interface CompressionReview {
	id: string;
	target: CompressionTarget;
	status: ReviewStatus;
	created_at: string;
	updated_at: string;
	history: ReviewEvent[];
	capture_verified: boolean;
}
async function send<T>(path: string, body?: unknown): Promise<T> {
	const response = await fetch(path, {
		method: body === undefined ? 'GET' : 'POST',
		headers: { 'content-type': 'application/json' },
		body: body === undefined ? undefined : JSON.stringify(body)
	});
	if (!response.ok) {
		throw new Error(
			response.status === 404
				? 'Compression review is unavailable on this server.'
				: `Could not save or load compression review (${response.status}).`
		);
	}
	return response.json();
}
export const listCompressionReviews = () => send<CompressionReview[]>('/api/compression-review');
export const flagCompression = (target: CompressionTarget, reason: string, evidence: string) =>
	send<{ id: string; added: boolean }>('/api/compression-review', {
		...target,
		reason,
		evidence,
		actor: 'inspect-ui'
	});
export const reviewCompression = (id: string, status: ReviewStatus, note: string) =>
	send(`/api/compression-review/${encodeURIComponent(id)}/review`, {
		status,
		note,
		actor: 'inspect-ui'
	});
