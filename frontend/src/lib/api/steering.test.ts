import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { getSessionSteering, getSessionSteeringPage } from './steering';
import type { SessionSteeringRow } from './steering';
import { __resetApiCacheForTests } from '$lib/stores/cache/apiCache.svelte';

function makeRow(id: number): SessionSteeringRow {
	return {
		intervention_id: id,
		session_id: 'sess-1',
		session_uuid: 'sess-1',
		project_path: null,
		steerer_name: 'verifier',
		steerer_display_name: 'Verifier',
		verdict: 'fired',
		authority: 'redirect',
		mode: 'augment',
		trigger_request_id: null,
		sent_request_id: null,
		sent_response_id: null,
		operator_turn_id: null,
		gate_reason: null,
		off_thread_topic: null,
		confidence: null,
		anchor_text: null,
		verdict_json: null,
		payload_text: null,
		steerer_model: null,
		sent_at: null,
		processed_at: null,
		expires_at: null,
		reverted_at: null,
		revert_reason: null,
		created_at: new Date(2026, 0, 1, 0, 0, id).toISOString()
	};
}

function jsonResponse(body: unknown, status = 200): Response {
	return {
		ok: status >= 200 && status < 300,
		status,
		statusText: status === 200 ? 'OK' : 'Error',
		json: async () => body
	} as Response;
}

describe('$lib/api/steering pagination', () => {
	let fetchMock: ReturnType<typeof vi.fn>;

	beforeEach(() => {
		__resetApiCacheForTests();
		fetchMock = vi.fn();
		vi.stubGlobal('fetch', fetchMock);
	});

	afterEach(() => {
		vi.unstubAllGlobals();
	});

	describe('getSessionSteeringPage', () => {
		it('sends offset/limit as query params and returns the envelope verbatim', async () => {
			const rows = [makeRow(1), makeRow(2)];
			fetchMock.mockResolvedValueOnce(
				jsonResponse({ rows, total_count: 53, offset: 10, limit: 2 })
			);

			const page = await getSessionSteeringPage('sess-1', { offset: 10, limit: 2 });

			expect(page).toEqual({ rows, total_count: 53, offset: 10, limit: 2 });
			const [url] = fetchMock.mock.calls[0]!;
			expect(url).toBe('/api/sessions/sess-1/steering?offset=10&limit=2');
		});

		it('returns an empty page (not an error) on 404', async () => {
			fetchMock.mockResolvedValueOnce(jsonResponse({}, 404));

			const page = await getSessionSteeringPage('sess-1', { limit: 50 });

			expect(page).toEqual({ rows: [], total_count: 0, offset: 0, limit: 50 });
		});
	});

	describe('getSessionSteering (fetch-all convenience)', () => {
		it('returns every row in one request when the session fits under one page', async () => {
			const rows = Array.from({ length: 53 }, (_, i) => makeRow(i + 1));
			fetchMock.mockResolvedValueOnce(
				jsonResponse({ rows, total_count: 53, offset: 0, limit: 500 })
			);

			const result = await getSessionSteering('sess-1');

			expect(result).toHaveLength(53);
			expect(result.map((r) => r.intervention_id)).toEqual(rows.map((r) => r.intervention_id));
			expect(fetchMock).toHaveBeenCalledTimes(1);
		});

		it('loops across multiple pages so a session with more than one page of interventions is still returned in full', async () => {
			const firstPage = Array.from({ length: 500 }, (_, i) => makeRow(i + 1));
			const secondPage = Array.from({ length: 53 }, (_, i) => makeRow(501 + i));
			fetchMock
				.mockResolvedValueOnce(
					jsonResponse({ rows: firstPage, total_count: 553, offset: 0, limit: 500 })
				)
				.mockResolvedValueOnce(
					jsonResponse({ rows: secondPage, total_count: 553, offset: 500, limit: 500 })
				);

			const result = await getSessionSteering('sess-1');

			expect(result).toHaveLength(553);
			expect(result[0]!.intervention_id).toBe(1);
			expect(result[552]!.intervention_id).toBe(553);
			expect(fetchMock).toHaveBeenCalledTimes(2);
			const [, secondCallUrl] = [fetchMock.mock.calls[0]![0], fetchMock.mock.calls[1]![0]];
			expect(secondCallUrl).toBe('/api/sessions/sess-1/steering?offset=500&limit=500');
		});

		it('stops instead of looping forever when the backend returns an empty page before total_count is reached', async () => {
			fetchMock.mockResolvedValueOnce(
				jsonResponse({ rows: [], total_count: 10, offset: 0, limit: 500 })
			);

			const result = await getSessionSteering('sess-1');

			expect(result).toEqual([]);
			expect(fetchMock).toHaveBeenCalledTimes(1);
		});
	});
});
