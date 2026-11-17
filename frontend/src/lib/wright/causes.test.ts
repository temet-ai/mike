import { describe, expect, it } from 'vitest';
import type { WrightCause } from '$lib/api/wright';
import { byPriorityDesc, causeLinks, causeState, openFor } from './causes';

const NOW = Date.parse('2026-11-17T12:00:00Z');
const hoursAgo = (h: number): string => new Date(NOW - h * 3_600_000).toISOString();

function cause(over: Partial<WrightCause> = {}): WrightCause {
	return {
		cause_id: 1,
		problem_id: 7,
		check_name: 'slow_queries',
		resource: 'query 1',
		problems: 1,
		observations: 1,
		opened_at: hoursAgo(1),
		attempts: 0,
		last_attempt_at: null,
		finding_id: null,
		followup_id: null,
		issue_number: null,
		remedied: false,
		priority: 1,
		notified_at: null,
		commented_at: null,
		...over
	};
}

describe('causeState', () => {
	it('names the answer a cause has, finding first', () => {
		expect(causeState(cause({ finding_id: 2, followup_id: 3, remedied: true }), 6, NOW)).toBe(
			'found'
		);
		expect(causeState(cause({ followup_id: 3, remedied: true }), 6, NOW)).toBe('filed');
		expect(causeState(cause({ remedied: true, opened_at: hoursAgo(40) }), 6, NOW)).toBe(
			'known_fix'
		);
	});

	it('is waiting only once nothing has answered it for the waited hours', () => {
		expect(causeState(cause({ opened_at: hoursAgo(7) }), 6, NOW)).toBe('waiting');
		expect(causeState(cause({ opened_at: hoursAgo(5) }), 6, NOW)).toBe('queued');
	});
});

describe('causeLinks', () => {
	it('links the finding, the issue on GitHub and the representative problem', () => {
		expect(causeLinks(cause({ finding_id: 5, issue_number: 272 }))).toEqual([
			{ kind: 'finding', text: '5', href: '#finding-5' },
			{
				kind: 'issue',
				text: '#272',
				href: 'https://github.com/temet-ai/mike/issues/272',
				external: true
			},
			{ kind: 'problem', text: '7', href: '#problem-7' }
		]);
	});
});

describe('openFor', () => {
	it('uses the largest whole unit', () => {
		expect(openFor(hoursAgo(0.5), NOW)).toBe('Open 30m');
		expect(openFor(hoursAgo(26), NOW)).toBe('Open 26h');
		expect(openFor(hoursAgo(72), NOW)).toBe('Open 3d');
	});
});

describe('byPriorityDesc', () => {
	it('ranks the higher priority first and breaks ties on the cause id', () => {
		const rows = [
			cause({ cause_id: 3, priority: 1 }),
			cause({ cause_id: 2, priority: 5 }),
			cause({ cause_id: 1, priority: 5 })
		];
		expect([...rows].sort(byPriorityDesc).map((c) => c.cause_id)).toEqual([1, 2, 3]);
	});
});
