import { describe, expect, it } from 'vitest';
import {
	buildPlot,
	formatSignedCount,
	formatSignedPoints,
	nearestRun,
	summariseChange,
	valueAt
} from './trend';

const day = (n: number, value: number) => ({
	at: new Date(Date.UTC(2026, 9, 21 + n, 12)).toISOString(),
	value
});

describe('signed deltas', () => {
	it('use a real minus sign', () => {
		expect(formatSignedCount(-61)).toBe('−61');
		expect(formatSignedCount(4)).toBe('+4');
		expect(formatSignedCount(-9410)).toBe('−9,410');
		expect(formatSignedPoints(-0.006)).toBe('−0.6 pts');
	});
});

describe('summariseChange', () => {
	const points = [day(0, 275), day(10, 214)];

	it('calls a fall better when lower is better', () => {
		const change = summariseChange(points, 'lower', formatSignedCount);
		expect(change).toMatchObject({ kind: 'down', tone: 'strong' });
		expect(change.label).toBe('−61 since 21 Oct, better');
	});

	it('calls a rise worse when lower is better', () => {
		const change = summariseChange([day(0, 2), day(5, 6)], 'lower', formatSignedCount);
		expect(change).toMatchObject({ kind: 'up', tone: 'warning' });
		expect(change.label).toBe('+4 since 21 Oct, worse');
	});

	it('gives no verdict word without a direction', () => {
		const change = summariseChange(points, undefined, formatSignedCount);
		expect(change).toMatchObject({ kind: 'down', tone: 'quiet' });
		expect(change.label).toBe('−61 since 21 Oct');
	});

	it('says so when nothing moved or there is one run', () => {
		expect(summariseChange([day(0, 3), day(4, 3)], 'lower', formatSignedCount).label).toBe(
			'no change since 21 Oct'
		);
		expect(summariseChange([day(0, 3)], 'lower', formatSignedCount)).toMatchObject({
			kind: 'single',
			label: 'one run so far'
		});
	});
});

describe('buildPlot', () => {
	const domain: [number, number] = [Date.parse(day(0, 0).at), Date.parse(day(10, 0).at)];

	it('puts the minimum at the bottom and the maximum at the top of the plot', () => {
		const plot = buildPlot([day(0, 10), day(10, 20)], domain, 56);
		expect(plot.min).toBe(10);
		expect(plot.max).toBe(20);
		expect(plot.points[0]!.y).toBeGreaterThan(plot.points[1]!.y);
		expect(plot.points[0]!.x).toBe(0);
		expect(plot.points[1]!.x).toBe(100);
		expect(plot.path).toBe(`M0 ${plot.points[0]!.y} H100 V${plot.points[1]!.y}`);
	});

	it('draws a flat series through the middle', () => {
		const plot = buildPlot([day(0, 5), day(5, 5)], domain, 56);
		expect(plot.flat).toBe(true);
		expect(plot.points.every((p) => p.y === 28)).toBe(true);
	});
});

describe('run lookup', () => {
	it('reads the value in force at a time and the nearest run', () => {
		const plot = buildPlot([day(0, 1), day(4, 2), day(8, 3)], [0, Date.parse(day(8, 0).at)], 56);
		expect(valueAt(plot.points, Date.parse(day(5, 0).at))!.value).toBe(2);
		expect(valueAt(plot.points, 0)!.value).toBe(1);
		const times = plot.points.map((p) => p.t);
		expect(nearestRun(times, Date.parse(day(7, 0).at))).toBe(times[2]);
		expect(nearestRun([], 1)).toBeNull();
	});
});
