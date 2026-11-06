import { ApiError } from '$lib/api';
import type { SessionFlowComponent } from './generated/SessionFlowComponent';
import type { SessionFlowPage } from './generated/SessionFlowPage';

export interface SessionFlowPageRequest {
	sessionId: string;
	component: SessionFlowComponent;
	cursor?: string;
	limit?: number;
	signal?: AbortSignal;
}
export type FetchSessionFlowPage = (request: SessionFlowPageRequest) => Promise<SessionFlowPage>;

export const fetchSessionFlowPage: FetchSessionFlowPage = async ({
	sessionId,
	component,
	cursor,
	limit = 100,
	signal
}) => {
	if (!Number.isInteger(limit) || limit < 1 || limit > 500)
		throw new RangeError('Flow page limit must be between 1 and 500');
	const query = new URLSearchParams({ component, limit: String(limit) });
	if (cursor !== undefined) query.set('cursor', cursor);
	const url = `/api/sessions/${encodeURIComponent(sessionId)}/flow?${query}`;
	const response = await fetch(url, { signal, headers: { accept: 'application/json' } });
	if (!response.ok) {
		const message = await response.text();
		throw new ApiError(message || `Flow request failed (${response.status})`, response.status, url);
	}
	const page: SessionFlowPage = await response.json();
	if (
		page.version !== 1 ||
		page.session_id !== sessionId ||
		page.coverage.component !== component
	) {
		throw new ApiError('Flow response contract or scope mismatch', 502, url);
	}
	return page;
};

export async function fetchSessionFlowSourceDetail(detailHref: string): Promise<unknown> {
	const response = await fetch(detailHref, { headers: { accept: 'application/json' } });
	if (!response.ok) {
		const message = await response.text();
		throw new ApiError(
			message || `Recorded source request failed (${response.status})`,
			response.status,
			detailHref
		);
	}
	return response.json();
}
