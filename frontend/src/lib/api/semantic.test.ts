import { describe, expect, it } from 'vitest';
import { medoidText, validSlug } from './semantic';
import type { CandidateEntry } from './semantic';

describe('semantic api helpers', () => {
	it('validates pattern slugs as the route does', () => {
		expect(validSlug('asked-twice')).toBe(true);
		expect(validSlug('ab')).toBe(false);
		expect(validSlug('Has Spaces')).toBe(false);
	});

	it('finds the medoid text among the members', () => {
		const entry = {
			candidate: { medoid_item_id: 2 },
			members: [
				{ item_id: 1, text: 'one' },
				{ item_id: 2, text: 'two' }
			]
		} as unknown as CandidateEntry;
		expect(medoidText(entry)).toBe('two');
	});
});
