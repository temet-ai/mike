import { describe, expect, it } from 'vitest';
import { cachePercent, formatCost, formatUsd, shortSessionId } from './format';
import { estimateCost } from './modelPricing';
import { pluralRequests } from './i18n';
import type { TokenUsage } from './types';

describe('shortSessionId', () => {
	it('truncates a UUID to 8 chars', () => {
		expect(shortSessionId('a1b2c3d4-e5f6-7890-abcd-ef1234567890')).toBe('a1b2c3d4');
	});

	it('truncates a hash id to 13 chars', () => {
		expect(shortSessionId('hash-abcdef1234567890')).toBe('hash-abcdef12');
	});

	it('returns short ids unchanged', () => {
		expect(shortSessionId('abc')).toBe('abc');
		expect(shortSessionId('hash-ab')).toBe('hash-ab');
	});

	it('truncates a generic non-UUID id to 8 chars', () => {
		expect(shortSessionId('plain-long-id')).toBe('plain-lo');
	});
});

describe('formatCost', () => {
	it('returns — when usage is null', () => {
		expect(formatCost(null)).toBe('—');
	});

	it('returns — when every field is zero (no real usage)', () => {
		const zero: TokenUsage = {
			input_tokens: 0,
			output_tokens: 0,
			cache_read_input_tokens: 0,
			cache_creation_input_tokens: 0
		};
		expect(formatCost(zero)).toBe('—');
	});

	it('formats a full-usage fixture with cache hits to 4 decimals', () => {
		const usage: TokenUsage = {
			input_tokens: 4_000,
			output_tokens: 3_847,
			cache_read_input_tokens: 38_200,
			cache_creation_input_tokens: 1_204
		};
		expect(formatCost(usage, 'claude-opus-4-6')).toBe('$0.1428');
	});

	it('formats a zero-cache fixture using only input + output prices', () => {
		const usage: TokenUsage = {
			input_tokens: 1_200,
			output_tokens: 850,
			cache_read_input_tokens: 0,
			cache_creation_input_tokens: 0
		};
		expect(formatCost(usage, 'claude-opus-4-6')).toBe('$0.0273');
	});

	it('treats null cache fields as zero (UsageReceived partials)', () => {
		const usage: TokenUsage = {
			input_tokens: 1_000,
			output_tokens: 0,
			cache_read_input_tokens: null,
			cache_creation_input_tokens: null
		};
		expect(formatCost(usage, 'claude-opus-4-6')).toBe('$0.0050');
	});

	it('renders sub-cent totals with four decimals of precision', () => {
		const usage: TokenUsage = {
			input_tokens: 10,
			output_tokens: 0,
			cache_read_input_tokens: 0,
			cache_creation_input_tokens: 0
		};
		const formatted = formatCost(usage);
		expect(formatted).toMatch(/^\$0\.\d{4}$/);
	});

	it('cost line items sum to formatCost total (FullUsage fixture)', () => {
		const usage: TokenUsage = {
			input_tokens: 4_000,
			output_tokens: 3_847,
			cache_read_input_tokens: 38_200,
			cache_creation_input_tokens: 1_204
		};
		const estimate = estimateCost(usage, 'claude-opus-4-6');
		const lineSum = estimate.lines.reduce((sum, l) => sum + l.cost, 0);
		expect(`$${lineSum.toFixed(4)}`).toBe(formatCost(usage, 'claude-opus-4-6'));
		expect(formatCost(usage, 'claude-opus-4-6')).toBe('$0.1428');
	});

	it('prices the same usage differently for different models', () => {
		const usage: TokenUsage = {
			input_tokens: 1_000_000,
			output_tokens: 1_000_000,
			cache_read_input_tokens: 0,
			cache_creation_input_tokens: 0
		};
		expect(formatCost(usage, 'claude-haiku-4-5-20251001')).toBe('$6.0000');
		expect(formatCost(usage, 'claude-sonnet-5-5')).toBe('$12.0000');
		expect(formatCost(usage, 'claude-opus-4-1-20250805')).toBe('$90.0000');
		expect(formatCost(usage, 'claude-fable-5-1')).toBe('$60.0000');
	});

	it('prices an unknown or missing model at the default, not at Opus 4.1 rates', () => {
		const usage: TokenUsage = {
			input_tokens: 1_000_000,
			output_tokens: 0,
			cache_read_input_tokens: 0,
			cache_creation_input_tokens: 0
		};
		expect(formatCost(usage, 'some-unknown-model')).toBe('$5.0000');
		expect(formatCost(usage, null)).toBe('$5.0000');
		expect(formatCost(usage)).toBe('$5.0000');
	});
});

describe('cachePercent', () => {
	it('returns null when usage is null', () => {
		expect(cachePercent(null)).toBe(null);
	});

	it('returns null when every field is zero (empty denominator)', () => {
		const zero: TokenUsage = {
			input_tokens: 0,
			output_tokens: 0,
			cache_read_input_tokens: 0,
			cache_creation_input_tokens: 0
		};
		expect(cachePercent(zero)).toBe(null);
	});

	it('matches the hand-computed value for the FullUsage fixture (91%)', () => {
		const usage: TokenUsage = {
			input_tokens: 4_000,
			output_tokens: 3_847,
			cache_read_input_tokens: 38_200,
			cache_creation_input_tokens: 1_204
		};
		expect(cachePercent(usage)).toBe(91);
	});

	it('treats null cache fields as zero (UsageReceived partials)', () => {
		const usage: TokenUsage = {
			input_tokens: 1_000,
			output_tokens: 0,
			cache_read_input_tokens: null,
			cache_creation_input_tokens: null
		};
		expect(cachePercent(usage)).toBe(0);
	});
});

describe('formatUsd', () => {
	it('renders two decimal places', () => {
		expect(formatUsd(1.69)).toBe('$1.69');
		expect(formatUsd(0.048)).toBe('$0.05');
		expect(formatUsd(20)).toBe('$20.00');
	});

	it('renders "—" for null/undefined (unconfigured, not a zero balance)', () => {
		expect(formatUsd(null)).toBe('—');
		expect(formatUsd(undefined)).toBe('—');
	});
});

describe('pluralRequests', () => {
	it('returns the plural form for zero', () => {
		expect(pluralRequests(0)).toBe('0 requests');
	});

	it('returns the singular form for one', () => {
		expect(pluralRequests(1)).toBe('1 request');
	});

	it('returns the plural form for two', () => {
		expect(pluralRequests(2)).toBe('2 requests');
	});

	it('returns the plural form for larger counts', () => {
		expect(pluralRequests(10)).toBe('10 requests');
	});
});
