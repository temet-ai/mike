import { describe, expect, it } from 'vitest';
import { buildCodeFinding, CODE_RULE_COUNTS } from '$lib/test-support/codeFindingsFixtures';
import {
	fixRate,
	formatLag,
	formatMicros,
	groupFindings,
	outcomeMark,
	severityMark,
	sortRules,
	totalRules
} from './codeFindingsFormat';

describe('groupFindings', () => {
	it('groups by checkout, path and rule and orders lines inside a group', () => {
		const groups = groupFindings([
			buildCodeFinding({ id: 3, line: 30, col: 1 }),
			buildCodeFinding({ id: 1, line: 5, col: 9 }),
			buildCodeFinding({ id: 2, line: 5, col: 2 }),
			buildCodeFinding({ id: 4, rule_id: 'other-rule' })
		]);
		expect(groups).toHaveLength(2);
		const group = groups.find((g) => g.rule_id === 'sh-fixed-tmp-path')!;
		expect(group.findings.map((f) => f.id)).toEqual([2, 1, 3]);
	});

	it('orders groups by oldest found_at descending, ties by smallest id', () => {
		const groups = groupFindings([
			buildCodeFinding({ id: 7, path: 'a', found_at: '2026-11-14T08:00:00Z' }),
			buildCodeFinding({ id: 5, path: 'b', found_at: '2026-11-14T08:00:00Z' }),
			buildCodeFinding({ id: 9, path: 'c', found_at: '2026-11-14T09:00:00Z' }),
			buildCodeFinding({ id: 1, path: 'c', found_at: '2026-11-14T10:00:00Z' })
		]);
		expect(groups.map((g) => g.path)).toEqual(['c', 'b', 'a']);
	});

	it('keeps group order when a newer finding joins an existing group', () => {
		const before = groupFindings([
			buildCodeFinding({ id: 1, path: 'a', found_at: '2026-11-14T08:00:00Z' }),
			buildCodeFinding({ id: 2, path: 'b', found_at: '2026-11-14T09:00:00Z' })
		]);
		const after = groupFindings([
			buildCodeFinding({ id: 1, path: 'a', found_at: '2026-11-14T08:00:00Z' }),
			buildCodeFinding({ id: 2, path: 'b', found_at: '2026-11-14T09:00:00Z' }),
			buildCodeFinding({ id: 3, path: 'a', line: 99, found_at: '2026-11-14T09:30:00Z' })
		]);
		expect(after.map((g) => g.path)).toEqual(before.map((g) => g.path));
	});
});

describe('marks', () => {
	it('maps every outcome and severity to its text', () => {
		expect(outcomeMark('fixed_by_other').label).toBe('fixed by other');
		expect(outcomeMark('superseded').label).toBe('file deleted');
		expect(outcomeMark('ignored').tone).toBe('strong');
		expect(severityMark('warning').tone).toBe('warning');
		expect(severityMark('error').tone).toBe('danger');
	});

	it('shows an unknown value as its own text', () => {
		expect(outcomeMark('mystery').label).toBe('mystery');
	});
});

describe('rule figures', () => {
	it('computes the fix rate as a whole percent and a dash when nothing was told', () => {
		expect(fixRate(64, 96)).toBe('67%');
		expect(fixRate(0, 0)).toBe('—');
	});

	it('formats scan times as microseconds below 1,000 and milliseconds above', () => {
		expect(formatMicros(640)).toBe('640 µs');
		expect(formatMicros(48_300)).toBe('48 ms');
		expect(formatMicros(1_500)).toBe('1.5 ms');
	});

	it('formats the telling lag and a dash for none', () => {
		expect(formatLag(null)).toBe('—');
		expect(formatLag(95)).toBe('1m 35s');
	});

	it('sorts by found descending then rule id and totals every column', () => {
		expect(sortRules(CODE_RULE_COUNTS).map((r) => r.rule_id)).toEqual([
			'rust-no-comments',
			'sh-fixed-tmp-path',
			'rust-temp-dir',
			'project-no-println'
		]);
		const totals = totalRules(CODE_RULE_COUNTS);
		expect(totals.found).toBe(171);
		expect(totals.delivered).toBe(129);
		expect(totals.pushed).toBe(53);
		expect(totals.fixed_after_told).toBe(87);
	});
});
