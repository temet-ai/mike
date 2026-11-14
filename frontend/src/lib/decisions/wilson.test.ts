import { describe, expect, it } from 'vitest';
import { formatInterval, wilson } from './wilson';

describe('wilson', () => {
	it('has no interval without observations', () => {
		expect(wilson(0, 0)).toBeNull();
		expect(formatInterval(null)).toBe('—');
	});

	it('matches the server formula for 8 of 10', () => {
		const interval = wilson(8, 10);
		expect(interval?.low).toBeCloseTo(0.49, 2);
		expect(interval?.high).toBeCloseTo(0.94, 2);
		expect(formatInterval(interval)).toBe('0.49–0.94');
	});

	it('stays inside 0 and 1 at the extremes', () => {
		expect(wilson(0, 5)?.low).toBe(0);
		expect(wilson(5, 5)?.high).toBe(1);
	});
});
