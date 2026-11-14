import { describe, expect, it } from 'vitest';
import { chanceFor, chanceText, orderFromParam, sortQuery } from './suggestedOrder';

describe('suggested order', () => {
	it('keeps the default order unless suggested was asked for', () => {
		expect(orderFromParam('suggested')).toBe('suggested');
		expect(orderFromParam('newest')).toBe('default');
		expect(orderFromParam(null)).toBe('default');
		expect(sortQuery('suggested')).toBe('sort=suggested');
		expect(sortQuery('default')).toBe('');
	});

	it('reads a chance by row id and shows a dash when there is none', () => {
		const chances = { '12': 0.624 };
		expect(chanceFor(chances, 12)).toBe(0.624);
		expect(chanceFor(chances, 13)).toBeNull();
		expect(chanceFor(undefined, 12)).toBeNull();
		expect(chanceFor(chances, null)).toBeNull();
		expect(chanceText(0.624)).toBe('62%');
		expect(chanceText(null)).toBe('—');
	});
});
