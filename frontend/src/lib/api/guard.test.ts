import { describe, expect, it } from 'vitest';
import { WRITABLE_MODES, formatRate } from './guard';

describe('guard api helpers', () => {
	it('formats a rate with its interval and n', () => {
		expect(formatRate({ k: 8, n: 10, value: 0.8, low: 0.49, high: 0.943 })).toBe(
			'80% (49% to 94%, n = 10)'
		);
		expect(formatRate({ k: 0, n: 0, value: null, low: null, high: null })).toBe('n = 0');
	});

	it('writes only off and shadow', () => {
		expect(WRITABLE_MODES).toEqual(['off', 'shadow']);
	});
});
