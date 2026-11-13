import { describe, expect, it } from 'vitest';
import { suggestCompositionName } from './suggestName';

describe('suggestCompositionName', () => {
	it('names a role prompt by its role', () => {
		expect(suggestCompositionName('You are a senior code reviewer. Review the diff.')).toBe(
			'Senior code reviewer'
		);
		expect(suggestCompositionName("You are Claude Code, Anthropic's official CLI")).toBe(
			'Claude Code'
		);
	});

	it('names a task prompt by its first clause, without trailing filler', () => {
		expect(
			suggestCompositionName(
				'You write a 3-5 word progress label for a background agent, from short notes.'
			)
		).toBe('Write a 3-5 word progress label');
	});

	it('skips blank and markdown heading lines', () => {
		expect(suggestCompositionName('\n\n# Summarise the session transcript\nMore.')).toBe(
			'Summarise the session transcript'
		);
	});

	it('returns null for empty instructions', () => {
		expect(suggestCompositionName('  \n ')).toBeNull();
	});
});
