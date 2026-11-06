import { describe, expect, it } from 'vitest';
import { isAtBottom } from './scroll-utils';

describe('isAtBottom', () => {
	it('returns true when at exact bottom', () => {
		expect(isAtBottom(800, 1000, 200)).toBe(true);
	});

	it('returns false when scrolled up beyond threshold', () => {
		expect(isAtBottom(600, 1000, 200)).toBe(false);
	});

	it('returns true when within default threshold (150px)', () => {
		expect(isAtBottom(700, 1000, 200)).toBe(true);
	});

	it('returns true when content is shorter than viewport', () => {
		expect(isAtBottom(0, 300, 600)).toBe(true);
	});

	it('honours a custom threshold tighter than the default', () => {
		expect(isAtBottom(700, 1000, 200, 50)).toBe(false);
		expect(isAtBottom(700, 1000, 200, 100)).toBe(true);
	});

	it('honours a custom threshold wider than the default', () => {
		expect(isAtBottom(600, 1000, 200, 300)).toBe(true);
	});
});
