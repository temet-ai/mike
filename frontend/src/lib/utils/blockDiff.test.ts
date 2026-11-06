import { describe, expect, it } from 'vitest';
import { diffBlocks, type DiffRow } from './blockDiff';

describe('diffBlocks — identity / empty', () => {
	it('returns empty array when both inputs are empty', () => {
		expect(diffBlocks({ original: [], sent: [] })).toEqual([]);
	});

	it('returns all unchanged when the two sides are identical', () => {
		const blocks = ['alpha', 'beta', 'gamma'];
		const rows = diffBlocks({ original: blocks, sent: blocks });
		expect(rows).toHaveLength(3);
		for (let i = 0; i < rows.length; i++) {
			const r = rows[i]!;
			expect(r.status).toBe('unchanged');
			if (r.status === 'unchanged') {
				expect(r.text).toBe(blocks[i]);
				expect(r.originalIndex).toBe(i);
				expect(r.sentIndex).toBe(i);
			}
		}
	});

	it('treats empty-original non-empty-sent as all appended (with sentIndex)', () => {
		const rows = diffBlocks({ original: [], sent: ['x', 'y'] });
		expect(rows).toEqual<DiffRow[]>([
			{ status: 'appended', text: 'x', sentIndex: 0 },
			{ status: 'appended', text: 'y', sentIndex: 1 }
		]);
	});

	it('treats non-empty-original empty-sent as all dropped (with originalIndex)', () => {
		const rows = diffBlocks({ original: ['x', 'y'], sent: [] });
		expect(rows).toEqual<DiffRow[]>([
			{ status: 'dropped', text: 'x', originalIndex: 0 },
			{ status: 'dropped', text: 'y', originalIndex: 1 }
		]);
	});
});

describe('diffBlocks — single operations', () => {
	it('detects a dropped block in the middle', () => {
		const rows = diffBlocks({
			original: ['a', 'b', 'c'],
			sent: ['a', 'c']
		});
		expect(rows).toHaveLength(3);
		expect(rows[0]).toEqual({
			status: 'unchanged',
			text: 'a',
			originalIndex: 0,
			sentIndex: 0
		});
		expect(rows[1]).toEqual({ status: 'dropped', text: 'b', originalIndex: 1 });
		expect(rows[2]).toEqual({
			status: 'unchanged',
			text: 'c',
			originalIndex: 2,
			sentIndex: 1
		});
	});

	it('detects an appended block in the middle', () => {
		const rows = diffBlocks({
			original: ['a', 'c'],
			sent: ['a', 'b', 'c']
		});
		expect(rows).toHaveLength(3);
		expect(rows[0]).toEqual({
			status: 'unchanged',
			text: 'a',
			originalIndex: 0,
			sentIndex: 0
		});
		expect(rows[1]).toEqual({ status: 'appended', text: 'b', sentIndex: 1 });
		expect(rows[2]).toEqual({
			status: 'unchanged',
			text: 'c',
			originalIndex: 1,
			sentIndex: 2
		});
	});

	it('detects a block replaced in place as a single replaced row', () => {
		const rows = diffBlocks({
			original: ['a', 'b', 'c'],
			sent: ['a', 'B', 'c']
		});
		expect(rows).toHaveLength(3);
		expect(rows[0]).toMatchObject({ status: 'unchanged', text: 'a' });
		expect(rows[1]).toEqual({
			status: 'replaced',
			originalText: 'b',
			sentText: 'B',
			originalIndex: 1,
			sentIndex: 1
		});
		expect(rows[2]).toMatchObject({ status: 'unchanged', text: 'c' });
	});

	it('treats a dropped block appended back much later as dropped + appended (not replaced)', () => {
		const rows = diffBlocks({
			original: ['a', 'b', 'c'],
			sent: ['a', 'c', 'b']
		});
		expect(rows.some((r) => r.status === 'replaced')).toBe(false);
		const dropped = rows.filter((r) => r.status === 'dropped');
		const appended = rows.filter((r) => r.status === 'appended');
		expect(dropped.length + appended.length).toBeGreaterThanOrEqual(1);
	});
});

describe('diffBlocks — composite', () => {
	it('preserves order when multiple operations interleave', () => {
		const rows = diffBlocks({
			original: ['a', 'b', 'c', 'd', 'e'],
			sent: ['a', 'B', 'c', 'E']
		});
		const statuses = rows.map((r) => r.status);
		expect(statuses[0]).toBe('unchanged');
		expect(rows[0]).toMatchObject({ text: 'a', originalIndex: 0, sentIndex: 0 });
		expect(statuses[2]).toBe('unchanged');
		expect(rows[2]).toMatchObject({ text: 'c', originalIndex: 2, sentIndex: 2 });
		expect(rows[1]).toMatchObject({
			status: 'replaced',
			originalText: 'b',
			sentText: 'B'
		});
		const tail = rows.slice(3);
		const tailTexts = tail.flatMap((r): string[] => {
			if (r.status === 'replaced') return [r.originalText, r.sentText];
			if (r.status === 'unchanged') return [r.text];
			return [r.text];
		});
		expect(tailTexts).toContain('d');
		expect(tailTexts).toContain('e');
		expect(tailTexts).toContain('E');
	});

	it('handles a pure replacement of every block', () => {
		const rows = diffBlocks({
			original: ['aa', 'bb'],
			sent: ['AA', 'BB']
		});
		const replacements = rows.filter((r) => r.status === 'replaced');
		const totalCovered =
			replacements.length * 2 +
			rows.filter((r) => r.status === 'dropped').length +
			rows.filter((r) => r.status === 'appended').length;
		expect(totalCovered).toBe(4);
	});
});
