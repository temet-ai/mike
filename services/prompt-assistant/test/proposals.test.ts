import { describe, expect, it } from 'vitest';
import { computeAfter, computeBefore, concatSystemBlocks } from '../src/mcp/proposals.js';

describe('computeBefore', () => {
	it('returns the match text when it is verifiably present in the captured prompt', () => {
		expect(computeBefore('Prefer the dedicated file/search tools over shell commands.', 'dedicated file/search tools')).toBe(
			'dedicated file/search tools'
		);
	});

	it('returns empty when the model claims text that is not actually in the prompt', () => {
		expect(computeBefore('Prefer the dedicated file/search tools over shell commands.', 'a sentence that was never captured')).toBe(
			''
		);
	});
});

describe('computeAfter', () => {
	it('replace: is exactly the proposed text', () => {
		expect(computeAfter('replace', 'old text', 'new text')).toBe('new text');
	});
	it('append: is the match followed by the addition', () => {
		expect(computeAfter('append', 'Existing paragraph.', ' One more sentence.')).toBe(
			'Existing paragraph. One more sentence.'
		);
	});
	it('drop: is empty — the text disappears', () => {
		expect(computeAfter('drop', 'Existing paragraph.', 'ignored')).toBe('');
	});
});

describe('concatSystemBlocks', () => {
	it('joins block text with newlines, and tolerates undefined', () => {
		expect(concatSystemBlocks(undefined)).toBe('');
		expect(
			concatSystemBlocks([
				{ index: 0, text: 'first', cache_control: null, composition_match: null },
				{ index: 1, text: 'second', cache_control: null, composition_match: null }
			])
		).toBe('first\nsecond');
	});
});
