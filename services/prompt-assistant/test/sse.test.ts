import { describe, expect, it } from 'vitest';
import { startSse, writeSse } from '../src/http/sse.js';
import { asResponse, FakeResponse } from './fakes.js';

describe('SSE framing', () => {
	it('startSse sets the event-stream headers', () => {
		const fake = new FakeResponse();
		startSse(asResponse(fake));
		expect(fake.statusCode).toBe(200);
		expect(fake.headers['content-type']).toBe('text/event-stream');
		expect(fake.headers['cache-control']).toBe('no-cache');
	});

	it('writeSse frames exactly "event: <name>\\ndata: <json>\\n\\n"', () => {
		const fake = new FakeResponse();
		writeSse(asResponse(fake), { event: 'text', data: { delta: 'hi' } });
		expect(fake.body).toBe('event: text\ndata: {"delta":"hi"}\n\n');
	});

	it('frames every event name the spec defines', () => {
		const fake = new FakeResponse();
		const res = asResponse(fake);
		writeSse(res, { event: 'status', data: { phase: 'reading prompt' } });
		writeSse(res, {
			event: 'proposal',
			data: {
				id: 'p1',
				title: 'T',
				edit_id: null,
				action: 'replace',
				match: 'm',
				before: 'b',
				after: 'a',
				reach: 'r'
			}
		});
		writeSse(res, { event: 'applied', data: { proposal_id: 'p1', rule_id: '5' } });
		writeSse(res, { event: 'error', data: { message: 'nope' } });
		writeSse(res, { event: 'done', data: { cost_usd: 0.01 } });
		const frames = fake.body.split('\n\n').filter(Boolean);
		expect(frames).toHaveLength(5);
		expect(frames[0]).toBe('event: status\ndata: {"phase":"reading prompt"}');
		expect(frames[4]).toBe('event: done\ndata: {"cost_usd":0.01}');
	});
});
