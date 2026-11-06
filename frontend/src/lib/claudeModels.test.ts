import { describe, expect, it } from 'vitest';
import { buildClaudeModelOptions } from './claudeModels';

describe('buildClaudeModelOptions', () => {
	it('includes every model observed in captured traffic, even ones not in the maintained list', () => {
		const observed = [
			'claude-sonnet-5',
			'claude-opus-5',
			'claude-haiku-4-5-20251001',
			'claude-fable-5-1',
			'claude-fable-5',
			'claude-opus-5-5',
			'claude-opus-4-8',
			'claude-haiku-4-5'
		];
		const options = buildClaudeModelOptions(observed);
		const ids = options.map((m) => m.id);
		for (const id of observed) {
			expect(ids).toContain(id);
		}
	});

	it('keeps a maintained model even when capture usage does not mention it', () => {
		const options = buildClaudeModelOptions([]);
		const ids = options.map((m) => m.id);
		expect(ids).toContain('claude-sonnet-5');
		expect(ids).toContain('claude-opus-5-5');
		expect(ids).toContain('claude-haiku-4-5-20251001');
		expect(ids).toContain('claude-fable-5-1');
		expect(ids).toContain('claude-opus-5');
	});

	it('adds a model seen only in capture traffic, with an honest fallback description', () => {
		const options = buildClaudeModelOptions(['claude-brand-new-model']);
		const added = options.find((m) => m.id === 'claude-brand-new-model');
		expect(added).toBeDefined();
		expect(added?.description).toMatch(/captured traffic/);
	});

	it('ignores blank observed ids and never duplicates a maintained model', () => {
		const options = buildClaudeModelOptions(['', 'claude-sonnet-5', 'claude-sonnet-5']);
		const sonnetRows = options.filter((m) => m.id === 'claude-sonnet-5');
		expect(sonnetRows).toHaveLength(1);
		expect(options.some((m) => m.id === '')).toBe(false);
	});

	it('sorts the merged list by model id', () => {
		const options = buildClaudeModelOptions(['claude-zzz-test']);
		const ids = options.map((m) => m.id);
		const sorted = [...ids].sort((a, b) => a.localeCompare(b));
		expect(ids).toEqual(sorted);
	});
});
