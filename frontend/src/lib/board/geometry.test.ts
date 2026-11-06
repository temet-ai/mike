import { describe, it, expect } from 'vitest';
import {
	chipPads,
	crosses,
	crossPoint,
	crossings,
	entersRect,
	segments,
	tooClose
} from './geometry';
import type { ChipRect, Route } from './types';

const rect = (key: string, x: number, y: number, w: number, h: number): ChipRect => ({
	key,
	kind: 'tool',
	x,
	y,
	w,
	h
});

describe('chipPads', () => {
	it('places every named pad on the chip edge it faces', () => {
		const pads = chipPads(rect('c', 10, 20, 100, 40));
		expect(pads.right).toEqual([110, 40]);
		expect(pads.righthi).toEqual([110, 28]);
		expect(pads.rightlo).toEqual([110, 52]);
		expect(pads.left).toEqual([10, 40]);
		expect(pads.lefthi).toEqual([10, 28]);
		expect(pads.leftlo).toEqual([10, 52]);
		expect(pads.top).toEqual([98, 20]);
		expect(pads.topl).toEqual([22, 20]);
		expect(pads.topc).toEqual([60, 20]);
		expect(pads.bottom).toEqual([22, 60]);
		expect(pads.bottomc).toEqual([60, 60]);
		expect(pads.bottomr).toEqual([98, 60]);
	});
});

describe('segments', () => {
	it('turns a polyline into consecutive point pairs', () => {
		const pts: readonly [number, number][] = [
			[0, 0],
			[10, 0],
			[10, 10]
		];
		expect(segments(pts)).toEqual([
			[
				[0, 0],
				[10, 0]
			],
			[
				[10, 0],
				[10, 10]
			]
		]);
	});
});

describe('crosses / crossPoint', () => {
	const h: readonly [[number, number], [number, number]] = [
		[0, 5],
		[10, 5]
	];
	it('detects a proper interior crossing', () => {
		const v: readonly [[number, number], [number, number]] = [
			[5, 0],
			[5, 10]
		];
		expect(crosses(h, v)).toBe(true);
		expect(crossPoint(h, v)).toEqual([5, 5]);
	});

	it('rejects a crossing that only touches within the 0.5px tolerance of an endpoint', () => {
		const v: readonly [[number, number], [number, number]] = [
			[0.3, 0],
			[0.3, 10]
		];
		expect(crosses(h, v)).toBe(false);
		expect(crossPoint(h, v)).toBeNull();
	});

	it('rejects two segments with the same orientation', () => {
		const h2: readonly [[number, number], [number, number]] = [
			[0, 6],
			[10, 6]
		];
		expect(crosses(h, h2)).toBe(false);
		expect(crossPoint(h, h2)).toBeNull();
	});

	it('rejects a T-junction where the vertical only reaches the horizontal (no strict crossing)', () => {
		const v: readonly [[number, number], [number, number]] = [
			[5, 5],
			[5, 10]
		];
		expect(crosses(h, v)).toBe(false);
	});
});

describe('crossings', () => {
	it('finds every same-layer crossing among a set of routes, never within one route', () => {
		const routes: Route[] = [
			{
				key: 'a',
				points: [
					[0, 5],
					[10, 5]
				]
			},
			{
				key: 'b',
				points: [
					[5, 0],
					[5, 10]
				]
			},
			{
				key: 'c',
				points: [
					[0, 0],
					[0, 20],
					[20, 20]
				]
			}
		];
		const found = crossings(routes);
		expect(found).toHaveLength(1);
		expect(found[0]![0]).toBe('a');
		expect(found[0]![1]).toBe('b');
		expect(found[0]![2]).toEqual([5, 5]);
	});

	it('is empty for parallel non-crossing routes', () => {
		const routes: Route[] = [
			{
				key: 'a',
				points: [
					[0, 0],
					[10, 0]
				]
			},
			{
				key: 'b',
				points: [
					[0, 10],
					[10, 10]
				]
			}
		];
		expect(crossings(routes)).toEqual([]);
	});
});

describe('entersRect', () => {
	const r = rect('chip', 0, 0, 10, 10);

	it('is true when a segment passes through the interior', () => {
		const seg: readonly [[number, number], [number, number]] = [
			[-5, 5],
			[15, 5]
		];
		expect(entersRect(seg, r)).toBe(true);
	});

	it('is false when a segment only skims the boundary edge', () => {
		const seg: readonly [[number, number], [number, number]] = [
			[-5, 0],
			[15, 0]
		];
		expect(entersRect(seg, r)).toBe(false);
	});

	it('is false when a segment stays entirely outside the rect', () => {
		const seg: readonly [[number, number], [number, number]] = [
			[20, 20],
			[30, 20]
		];
		expect(entersRect(seg, r)).toBe(false);
	});
});

describe('tooClose', () => {
	it('flags two parallel same-direction segments overlapping closer than the pitch', () => {
		const routes: Route[] = [
			{
				key: 'a',
				points: [
					[0, 0],
					[10, 0]
				]
			},
			{
				key: 'b',
				points: [
					[0, 1],
					[10, 1]
				]
			}
		];
		expect(tooClose(routes, 3)).toHaveLength(1);
	});

	it('does not flag segments spaced at or beyond the pitch', () => {
		const routes: Route[] = [
			{
				key: 'a',
				points: [
					[0, 0],
					[10, 0]
				]
			},
			{
				key: 'b',
				points: [
					[0, 4],
					[10, 4]
				]
			}
		];
		expect(tooClose(routes, 3)).toEqual([]);
	});

	it('does not flag collinear overlap (a deliberate merge)', () => {
		const routes: Route[] = [
			{
				key: 'a',
				points: [
					[0, 0],
					[10, 0]
				]
			},
			{
				key: 'b',
				points: [
					[5, 0],
					[15, 0]
				]
			}
		];
		expect(tooClose(routes, 3)).toEqual([]);
	});

	it('does not flag non-overlapping parallel segments even if close', () => {
		const routes: Route[] = [
			{
				key: 'a',
				points: [
					[0, 0],
					[10, 0]
				]
			},
			{
				key: 'b',
				points: [
					[20, 1],
					[30, 1]
				]
			}
		];
		expect(tooClose(routes, 3)).toEqual([]);
	});
});
