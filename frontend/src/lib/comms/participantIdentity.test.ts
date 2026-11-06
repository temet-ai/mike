import { describe, it, expect } from 'vitest';
import { labelCarriesShortId, resolveParticipantLabel } from './participantIdentity';

describe('labelCarriesShortId', () => {
	it('is true when the label already ends in the short id', () => {
		expect(
			labelCarriesShortId('coordinator-e74ba661', 'e74ba661-1111-2222-3333-444444444444')
		).toBe(true);
	});

	it('is false when the label does not carry the short id', () => {
		expect(labelCarriesShortId('rust-developer', 'e74ba661-1111-2222-3333-444444444444')).toBe(
			false
		);
	});

	it('is true when there is no session to append', () => {
		expect(labelCarriesShortId('mike', null)).toBe(true);
	});
});

describe('resolveParticipantLabel', () => {
	it('names a message with no sending session mike', () => {
		expect(resolveParticipantLabel(null, new Map())).toBe('mike');
	});
});
