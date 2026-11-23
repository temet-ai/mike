import { afterEach, describe, expect, it, vi } from 'vitest';
import {
	applyConsolidationDecision,
	getConsolidationDecisions,
	getConsolidationMeasures,
	getConsolidationSubject,
	getConsolidationSubjects,
	settleConsolidationConflict,
	undoConsolidationDecision
} from './knowledgeConsolidation';

function reply(body: unknown, status = 200) {
	return vi.fn().mockResolvedValue(
		new Response(JSON.stringify(body), {
			status,
			headers: { 'content-type': 'application/json' }
		})
	);
}

afterEach(() => vi.unstubAllGlobals());

describe('knowledge consolidation client', () => {
	it('unwraps the list responses and sends the filters the routes take', async () => {
		const fetchMock = reply({ project_id: 3, measures: [{ id: 1 }] });
		vi.stubGlobal('fetch', fetchMock);
		await expect(
			getConsolidationMeasures('3', { since: '2026-11-01T00:00:00.000Z', limit: 500 })
		).resolves.toEqual([{ id: 1 }]);
		expect(fetchMock.mock.calls[0]![0]).toBe(
			'/api/knowledge/consolidation/measures?project=3&since=2026-11-01T00%3A00%3A00.000Z&limit=500'
		);

		vi.stubGlobal('fetch', reply({ project_id: 3, subjects: [{ id: 9 }] }));
		await expect(getConsolidationSubjects('3', { limit: 50, offset: 50 })).resolves.toEqual([
			{ id: 9 }
		]);
	});

	it('joins verdicts, passes the cursor and returns next_before', async () => {
		const fetchMock = reply({ project_id: 3, decisions: [{ id: 5 }], next_before: 5 });
		vi.stubGlobal('fetch', fetchMock);
		const page = await getConsolidationDecisions({
			project: '3',
			verdicts: ['covered', 'merged'],
			before: 9,
			limit: 20,
			open: true
		});
		expect(page).toEqual({ decisions: [{ id: 5 }], next_before: 5 });
		expect(fetchMock.mock.calls[0]![0]).toBe(
			'/api/knowledge/consolidation/decisions?project=3&before=9&limit=20&verdict=covered%2Cmerged&open=true'
		);
	});

	it('posts settle with the kept statement and unwraps undo and apply', async () => {
		const settle = reply({ decision: { id: 1 }, conflict: { id: 2 } });
		vi.stubGlobal('fetch', settle);
		await settleConsolidationConflict(2, 8101);
		const [url, init] = settle.mock.calls[0]!;
		expect(url).toBe('/api/knowledge/consolidation/decisions/2/settle');
		expect(init.method).toBe('POST');
		expect(JSON.parse(init.body)).toEqual({ keep_statement_id: 8101 });

		vi.stubGlobal('fetch', reply({ decision: { id: 7, application: 'undone' } }));
		await expect(undoConsolidationDecision(7)).resolves.toMatchObject({ application: 'undone' });
		vi.stubGlobal('fetch', reply({ decision: { id: 7, application: 'applied' } }));
		await expect(applyConsolidationDecision(7)).resolves.toMatchObject({ application: 'applied' });
	});

	it('carries the server message and status on a refusal', async () => {
		vi.stubGlobal('fetch', reply({ error: 'this conflict was already settled' }, 409));
		await expect(undoConsolidationDecision(7)).rejects.toMatchObject({
			message: 'this conflict was already settled',
			status: 409
		});
		vi.stubGlobal('fetch', reply({ error: 'no subject 5' }, 404));
		await expect(getConsolidationSubject(5)).rejects.toMatchObject({ status: 404 });
	});
});
