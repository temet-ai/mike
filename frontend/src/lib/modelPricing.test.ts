import { describe, expect, it } from 'vitest';
import {
	DEFAULT_MODEL_PRICE,
	MODEL_PRICE_TABLE,
	estimateCost,
	formatCostAmount,
	hasAnyTokens,
	lookupModelPrice,
	normalizeModelId,
	priceNote
} from './modelPricing';
import type { TokenUsage } from './types';

function usage(overrides: Partial<TokenUsage> = {}): TokenUsage {
	return {
		input_tokens: 0,
		output_tokens: 0,
		cache_read_input_tokens: 0,
		cache_creation_input_tokens: 0,
		...overrides
	};
}

describe('normalizeModelId', () => {
	it('drops a trailing date', () => {
		expect(normalizeModelId('claude-haiku-4-5-20251001')).toBe('claude-haiku-4-5');
	});

	it('drops the context suffix, version suffix and provider prefix', () => {
		expect(normalizeModelId('claude-opus-4-6[1m]')).toBe('claude-opus-4-6');
		expect(normalizeModelId('anthropic.claude-opus-5-5-v1:0')).toBe('claude-opus-5-5');
		expect(normalizeModelId('us.anthropic.claude-sonnet-4-5-20250929-v1:0')).toBe(
			'claude-sonnet-4-5'
		);
		expect(normalizeModelId('claude-opus-4-5@20251101')).toBe('claude-opus-4-5');
	});

	it('ignores case and surrounding spaces', () => {
		expect(normalizeModelId('  Claude-Sonnet-5-5 ')).toBe('claude-sonnet-5-5');
	});
});

describe('lookupModelPrice', () => {
	it.each([
		['claude-fable-5-1', 10, 50, 0.25, 12.5, 20],
		['claude-fable-5', 10, 50, 1, 12.5, 20],
		['claude-opus-5-5', 4, 20, 0.2, 5, 8],
		['claude-opus-5', 5, 25, 0.5, 6.25, 10],
		['claude-opus-4-8', 5, 25, 0.5, 6.25, 10],
		['claude-opus-4-7', 5, 25, 0.5, 6.25, 10],
		['claude-opus-4-6', 5, 25, 0.5, 6.25, 10],
		['claude-opus-4-5-20251101', 5, 25, 0.5, 6.25, 10],
		['claude-opus-4-1-20250805', 15, 75, 1.5, 18.75, 30],
		['claude-opus-4-20250514', 15, 75, 1.5, 18.75, 30],
		['claude-sonnet-5-5', 2, 10, 0.2, 2.5, 4],
		['claude-sonnet-5', 2, 10, 0.2, 2.5, 4],
		['claude-sonnet-4-6', 3, 15, 0.3, 3.75, 6],
		['claude-sonnet-4-5-20250929', 3, 15, 0.3, 3.75, 6],
		['claude-haiku-4-5-20251001', 1, 5, 0.1, 1.25, 2]
	])('%s', (model, input, output, cacheRead, write5m, write1h) => {
		const found = lookupModelPrice(model);
		expect(found.known).toBe(true);
		expect(found.price).toEqual({
			input,
			output,
			cacheRead,
			cacheWrite5m: write5m,
			cacheWrite1h: write1h
		});
	});

	it('does not confuse Opus 5 with Opus 5.5, or Fable 5 with Fable 5.1', () => {
		expect(lookupModelPrice('claude-opus-5').label).toBe('Claude Opus 5');
		expect(lookupModelPrice('claude-opus-5-5-20260401').label).toBe('Claude Opus 5.5');
		expect(lookupModelPrice('claude-fable-5').label).toBe('Claude Fable 5');
		expect(lookupModelPrice('claude-fable-5-1').label).toBe('Claude Fable 5.1');
	});

	it('falls back to the labelled default for an unknown model', () => {
		const found = lookupModelPrice('claude-opus-4-9');
		expect(found.known).toBe(false);
		expect(found.price).toBe(DEFAULT_MODEL_PRICE);
		expect(found.label).toContain('not recognised');
	});

	it('falls back for null, undefined and blank model ids', () => {
		expect(lookupModelPrice(null).known).toBe(false);
		expect(lookupModelPrice(undefined).known).toBe(false);
		expect(lookupModelPrice('   ').known).toBe(false);
	});

	it('does not price anything at the old Opus 4.1 rates by accident', () => {
		expect(DEFAULT_MODEL_PRICE.input).not.toBe(15);
	});

	it('prices cache writes at 1.25x input (5 minutes) and 2x input (1 hour) for every entry', () => {
		for (const entry of MODEL_PRICE_TABLE) {
			expect(entry.price.cacheWrite5m).toBeCloseTo(entry.price.input * 1.25, 10);
			expect(entry.price.cacheWrite1h).toBeCloseTo(entry.price.input * 2, 10);
			expect(entry.price.output).toBeGreaterThan(entry.price.input);
		}
	});
});

describe('priceNote', () => {
	it('names the model and says it is not a bill for a known model', () => {
		const note = priceNote(lookupModelPrice('claude-haiku-4-5'), 'claude-haiku-4-5');
		expect(note).toContain('Claude Haiku 4.5');
		expect(note).toContain('Not a bill');
	});

	it('says the price is an estimate and names the unknown model', () => {
		const note = priceNote(lookupModelPrice('mystery-model'), 'mystery-model');
		expect(note).toContain('Estimate only');
		expect(note).toContain('"mystery-model"');
	});
});

describe('hasAnyTokens', () => {
	it('is false for null and for all-zero usage', () => {
		expect(hasAnyTokens(null)).toBe(false);
		expect(hasAnyTokens(usage())).toBe(false);
	});

	it('is true when only the cache split carries tokens', () => {
		expect(hasAnyTokens(usage({ cache_creation: { ephemeral_1h_input_tokens: 5 } }))).toBe(true);
	});
});

describe('estimateCost', () => {
	it('prices each line at its own rate (Haiku 4.5)', () => {
		const est = estimateCost(
			usage({
				input_tokens: 1_000_000,
				output_tokens: 2_000_000,
				cache_read_input_tokens: 10_000_000,
				cache_creation_input_tokens: 4_000_000
			}),
			'claude-haiku-4-5-20251001'
		);
		const byKey = Object.fromEntries(est.lines.map((l) => [l.key, l.cost]));
		expect(byKey.input).toBeCloseTo(1, 10);
		expect(byKey.output).toBeCloseTo(10, 10);
		expect(byKey.cacheRead).toBeCloseTo(1, 10);
		expect(byKey.cacheWrite5m).toBeCloseTo(5, 10);
		expect(est.total).toBeCloseTo(17, 10);
		expect(est.lookup.known).toBe(true);
	});

	it('prices all cache writes at the 5-minute rate when no split is reported', () => {
		const est = estimateCost(usage({ cache_creation_input_tokens: 1_000_000 }), 'claude-opus-5');
		expect(est.lines.map((l) => l.key)).not.toContain('cacheWrite1h');
		expect(est.total).toBeCloseTo(6.25, 10);
	});

	it('prices the 5-minute and 1-hour parts at their own rates when the split is reported', () => {
		const est = estimateCost(
			usage({
				cache_creation_input_tokens: 3_000_000,
				cache_creation: {
					ephemeral_5m_input_tokens: 1_000_000,
					ephemeral_1h_input_tokens: 2_000_000
				}
			}),
			'claude-opus-5'
		);
		const byKey = Object.fromEntries(est.lines.map((l) => [l.key, l]));
		expect(byKey.cacheWrite5m?.tokens).toBe(1_000_000);
		expect(byKey.cacheWrite1h?.tokens).toBe(2_000_000);
		expect(est.total).toBeCloseTo(6.25 + 20, 10);
	});

	it('prices cache writes the split does not account for at the 5-minute rate', () => {
		const est = estimateCost(
			usage({
				cache_creation_input_tokens: 3_000_000,
				cache_creation: { ephemeral_1h_input_tokens: 1_000_000 }
			}),
			'claude-sonnet-5'
		);
		const byKey = Object.fromEntries(est.lines.map((l) => [l.key, l]));
		expect(byKey.cacheWrite5m?.tokens).toBe(2_000_000);
		expect(byKey.cacheWrite1h?.tokens).toBe(1_000_000);
		expect(est.total).toBeCloseTo(2_000_000 * 2.5e-6 + 1_000_000 * 4e-6, 10);
	});

	it('uses the split alone when the total is null', () => {
		const est = estimateCost(
			usage({
				cache_creation_input_tokens: null,
				cache_creation: {
					ephemeral_5m_input_tokens: 1_000_000,
					ephemeral_1h_input_tokens: 1_000_000
				}
			}),
			'claude-haiku-4-5'
		);
		expect(est.total).toBeCloseTo(1.25 + 2, 10);
	});

	it('uses the Fable 5.1 cache read rate, which differs from Fable 5', () => {
		const reads = usage({ cache_read_input_tokens: 1_000_000 });
		expect(estimateCost(reads, 'claude-fable-5-1').total).toBeCloseTo(0.25, 10);
		expect(estimateCost(reads, 'claude-fable-5').total).toBeCloseTo(1, 10);
		expect(estimateCost(reads, 'claude-opus-5-5').total).toBeCloseTo(0.2, 10);
	});

	it('treats null token counts as zero', () => {
		const est = estimateCost(
			usage({ cache_read_input_tokens: null, cache_creation_input_tokens: null }),
			'claude-opus-5'
		);
		expect(est.total).toBe(0);
	});

	it('marks an unknown model as not known', () => {
		expect(estimateCost(usage({ input_tokens: 1 }), 'nope').lookup.known).toBe(false);
	});

	it('formats a total to four decimals with a dollar sign', () => {
		expect(formatCostAmount(0.14283)).toBe('$0.1428');
	});
});
