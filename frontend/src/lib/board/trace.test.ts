import { describe, it, expect } from 'vitest';
import { cleanPoints, dashArray, pathLength, tracePath } from './trace';
import { TRACE_KINDS, type Point } from './types';

describe('cleanPoints', () => {
	it('drops consecutive duplicate points', () => {
		const pts: Point[] = [
			[0, 0],
			[0, 0],
			[10, 0]
		];
		expect(cleanPoints(pts)).toEqual([
			[0, 0],
			[10, 0]
		]);
	});

	it('drops collinear middle points but keeps genuine corners', () => {
		const pts: Point[] = [
			[0, 0],
			[0, 5],
			[0, 10],
			[5, 10]
		];
		expect(cleanPoints(pts)).toEqual([
			[0, 0],
			[0, 10],
			[5, 10]
		]);
	});
});

describe('tracePath — chamfer geometry', () => {
	it('cuts a 45° chamfer of length min(chamfer, lIn/2, lOut/2) at a right-angle corner', () => {
		const pts: Point[] = [
			[0, 0],
			[20, 0],
			[20, 20]
		];
		const d = tracePath(pts);
		expect(d).toBe('M 0.0 0.0 L 14.0 0.0 L 20.0 6.0 L 20.0 20.0');
	});

	it('degrades to a plain L (no diagonal chamfer segment) when cc < 1', () => {
		const pts: Point[] = [
			[0, 0],
			[1, 0],
			[1, 1]
		];
		const d = tracePath(pts);
		expect(d).toBe('M 0.0 0.0 L 1.0 0.0 L 1.0 0.0 L 1.0 1.0');
	});

	it('places a semicircular hop arc on a vertical run at a crossover point', () => {
		const pts: Point[] = [
			[10, 0],
			[10, 20],
			[30, 20]
		];
		const d = tracePath(pts, 6, [[10, 8]]);
		expect(d).toContain('A 4 4 0 0 1 10.0 12.0');
		expect(d.match(/A /g)).toHaveLength(1);
	});

	it('omits the hop arc when the point is outside the vertical run, or off the vertical entirely', () => {
		const pts: Point[] = [
			[10, 0],
			[10, 20],
			[30, 20]
		];
		const d = tracePath(pts, 6, [
			[10, 1],
			[10, 19],
			[99, 8]
		]);
		expect(d).not.toContain('A ');
	});
});

describe('pathLength', () => {
	it('matches the chamfer-adjusted length for a 3-point route', () => {
		const pts: Point[] = [
			[0, 0],
			[0, 10],
			[10, 10]
		];
		const expected = 20 - (2 * 5 - 5 * Math.SQRT2);
		expect(pathLength(pts)).toBeCloseTo(expected, 6);
		expect(pathLength(pts)).toBeCloseTo(17.071067811865476, 6);
	});

	it('adds pi*4 - 8 of length per hop', () => {
		const pts: Point[] = [
			[0, 0],
			[0, 10],
			[10, 10]
		];
		const base = pathLength(pts);
		expect(pathLength(pts, 6, 1)).toBeCloseTo(base + (Math.PI * 4 - 8), 6);
	});
});

describe('dashArray', () => {
	it('returns the trace kind spec pattern', () => {
		for (const [kind, spec] of Object.entries(TRACE_KINDS)) {
			expect(dashArray(kind as keyof typeof TRACE_KINDS)).toBe(spec.pattern);
		}
	});

	it('is empty for a solid kind and non-empty for a dashed kind', () => {
		expect(dashArray('capture')).toBe('');
		expect(dashArray('toolcall')).toBe('8 3');
	});
});
