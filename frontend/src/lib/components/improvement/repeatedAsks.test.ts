import { describe, expect, it } from 'vitest';
import type { RepeatGroup } from '$lib/api/improvement';
import { REPEATS, REPEATS_NOW, REPEAT_WEEK_STARTS } from '$lib/test-support/repeatedAsksFixtures';
import {
	issueBody,
	issueHref,
	issueTitle,
	issueTooLong,
	scaleMax,
	spanText,
	weekCaption,
	weeklyReadout
} from './repeatedAsks';

const [GET_WORK_DONE, COORDINATE, ARTIFACT, COMMIT_AND_PUSH] = REPEATS.groups as [
	RepeatGroup,
	RepeatGroup,
	RepeatGroup,
	RepeatGroup
];

function longGroup(): RepeatGroup {
	const text = 'word '.repeat(400).trim();
	const items = Array.from({ length: 6 }, (_, i) => ({
		turn_id: i + 1,
		session_id: `00000000-0000-4000-8000-00000000000${i}`,
		at: '2026-09-01T10:00:00Z',
		project: 'temet-proxy',
		text
	}));
	return { ...GET_WORK_DONE, n: items.length, items };
}

describe('issue link', () => {
	it('titles the issue with the first message, cut at a word boundary within 80 characters', () => {
		const title = issueTitle(longGroup());
		expect(title).toBe(`Operator keeps saying: ${'word '.repeat(16).trim()}`);
		expect(title.replace('Operator keeps saying: ', '').length).toBeLessThanOrEqual(80);
	});

	it('keeps a short first message whole', () => {
		expect(issueTitle(GET_WORK_DONE)).toBe(
			'Operator keeps saying: commit all the work that has been done'
		);
	});

	it('writes one line per message, oldest first, then the measure', () => {
		const lines = issueBody(COMMIT_AND_PUSH).split('\n');
		expect(lines[0]).toBe('- 29 Jul · temet-proxy · commit and push everything.');
		expect(lines).toHaveLength(COMMIT_AND_PUSH.items.length + 2);
		expect(lines[COMMIT_AND_PUSH.items.length]).toBe('');
		expect(lines.at(-1)).toBe(
			"Measure: this group's count per week on /sessions/improvement should fall to zero."
		);
	});

	it('links to a new issue in the mike repository with the whole list encoded', () => {
		const href = issueHref(ARTIFACT);
		expect(
			href.startsWith('https://github.com/temet-ai/mike/issues/new?title=Operator%20keeps%20saying')
		).toBe(true);
		expect(issueTooLong(ARTIFACT)).toBe(false);
		const body = new URL(href).searchParams.get('body');
		expect(body).toBe(issueBody(ARTIFACT));
	});

	it('over 8,000 characters keeps the title and asks for the clipboard list, never cutting the body', () => {
		const group = longGroup();
		expect(issueTooLong(group)).toBe(true);
		const href = issueHref(group);
		expect(href.length).toBeLessThan(8000);
		expect(href).toContain('?title=Operator%20keeps%20saying');
		expect(new URL(href).searchParams.get('body')).toContain(
			'Paste the list from your clipboard here.'
		);
		expect(issueBody(group).split('\n')).toHaveLength(group.items.length + 2);
		expect(issueBody(group)).toContain('word '.repeat(399) + 'word');
	});
});

describe('spanText', () => {
	it('gives the dates, the days between and when it was last said', () => {
		expect(spanText(COORDINATE, REPEATS_NOW)).toBe(
			'29 Jul to 30 Sep · 63 days · last said 16 days ago'
		);
	});

	it('says today and yesterday in words', () => {
		expect(spanText(COMMIT_AND_PUSH, REPEATS_NOW)).toBe(
			'29 Jul to 16 Oct · 79 days · last said today'
		);
		expect(spanText(ARTIFACT, Date.parse('2026-10-16T12:00:00Z'))).toContain('last said yesterday');
	});

	it('names the year only when it is not this year', () => {
		const text = spanText(COORDINATE, Date.parse('2027-02-01T12:00:00Z'));
		expect(text.startsWith('29 Jul 2026 to 30 Sep 2026')).toBe(true);
	});

	it('says same day when the first and last are one day', () => {
		const same: RepeatGroup = { ...COORDINATE, first_at: COORDINATE.last_at };
		expect(spanText(same, REPEATS_NOW)).toContain('same day');
	});
});

describe('weeks', () => {
	it('captions the first and last week start', () => {
		expect(weekCaption(REPEAT_WEEK_STARTS)).toBe('Weeks starting 25 Jul to 10 Oct');
		expect(weekCaption([])).toBe('');
	});

	it('reads the counts in order for a screen reader', () => {
		expect(weeklyReadout([0, 1, 0, 2])).toBe('Per week, oldest first: 0, 1, 0, 2');
	});

	it('scales every group by the largest weekly count among all of them', () => {
		const heights = REPEATS.groups.flatMap((g) => g.weekly);
		expect(scaleMax(REPEATS.groups)).toBe(Math.max(...heights));
		expect(scaleMax([])).toBe(1);
		expect(scaleMax([{ ...COORDINATE, weekly: [0, 0, 0] }])).toBe(1);
	});

	it('has one count per week and the counts add up to the group size', () => {
		for (const g of REPEATS.groups) {
			expect(g.weekly).toHaveLength(REPEAT_WEEK_STARTS.length);
			expect(g.weekly.reduce((a, b) => a + b, 0)).toBe(g.n);
			expect(g.items).toHaveLength(g.n);
		}
	});
});
