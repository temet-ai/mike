import type { TokenUsage } from './types';

/** US dollars per million tokens. */
export interface ModelPrice {
	input: number;
	output: number;
	cacheRead: number;
	cacheWrite5m: number;
	cacheWrite1h: number;
}

export interface PriceLookup {
	price: ModelPrice;
	/** Plain name shown to the person, e.g. "Claude Opus 4.8". */
	label: string;
	/** False when the model id is not in the table and the default price was used. */
	known: boolean;
}

/**
 * List prices from https://platform.claude.com/docs/en/about-claude/pricing (model pricing
 * table, read 2026-10-07). Cache reads: 0.1x input, 0.05x on Opus 5.5, 0.025x on Fable 5.1
 * and Mythos 5.1. Cache writes: 1.25x input for 5 minutes, 2x for 1 hour.
 */
const FABLE_5: ModelPrice = {
	input: 10,
	output: 50,
	cacheRead: 1,
	cacheWrite5m: 12.5,
	cacheWrite1h: 20
};
const FABLE_5_1: ModelPrice = { ...FABLE_5, cacheRead: 0.25 };
const OPUS_5_5: ModelPrice = {
	input: 4,
	output: 20,
	cacheRead: 0.2,
	cacheWrite5m: 5,
	cacheWrite1h: 8
};
const OPUS_5: ModelPrice = {
	input: 5,
	output: 25,
	cacheRead: 0.5,
	cacheWrite5m: 6.25,
	cacheWrite1h: 10
};
const OPUS_4_1: ModelPrice = {
	input: 15,
	output: 75,
	cacheRead: 1.5,
	cacheWrite5m: 18.75,
	cacheWrite1h: 30
};
const SONNET_5: ModelPrice = {
	input: 2,
	output: 10,
	cacheRead: 0.2,
	cacheWrite5m: 2.5,
	cacheWrite1h: 4
};
const SONNET_4: ModelPrice = {
	input: 3,
	output: 15,
	cacheRead: 0.3,
	cacheWrite5m: 3.75,
	cacheWrite1h: 6
};
const HAIKU_4_5: ModelPrice = {
	input: 1,
	output: 5,
	cacheRead: 0.1,
	cacheWrite5m: 1.25,
	cacheWrite1h: 2
};

interface TableEntry {
	/** Model id with date, version and context suffixes removed. */
	id: string;
	label: string;
	price: ModelPrice;
}

export const MODEL_PRICE_TABLE: readonly TableEntry[] = [
	{ id: 'claude-fable-5-1', label: 'Claude Fable 5.1', price: FABLE_5_1 },
	{ id: 'claude-fable-5', label: 'Claude Fable 5', price: FABLE_5 },
	{ id: 'claude-mythos-5-1', label: 'Claude Mythos 5.1', price: FABLE_5_1 },
	{ id: 'claude-mythos-5', label: 'Claude Mythos 5', price: FABLE_5 },
	{ id: 'claude-opus-5-5', label: 'Claude Opus 5.5', price: OPUS_5_5 },
	{ id: 'claude-opus-5', label: 'Claude Opus 5', price: OPUS_5 },
	{ id: 'claude-opus-4-8', label: 'Claude Opus 4.8', price: OPUS_5 },
	{ id: 'claude-opus-4-7', label: 'Claude Opus 4.7', price: OPUS_5 },
	{ id: 'claude-opus-4-6', label: 'Claude Opus 4.6', price: OPUS_5 },
	{ id: 'claude-opus-4-5', label: 'Claude Opus 4.5', price: OPUS_5 },
	{ id: 'claude-opus-4-1', label: 'Claude Opus 4.1', price: OPUS_4_1 },
	{ id: 'claude-opus-4-0', label: 'Claude Opus 4', price: OPUS_4_1 },
	{ id: 'claude-opus-4', label: 'Claude Opus 4', price: OPUS_4_1 },
	{ id: 'claude-sonnet-5-5', label: 'Claude Sonnet 5.5', price: SONNET_5 },
	{ id: 'claude-sonnet-5', label: 'Claude Sonnet 5', price: SONNET_5 },
	{ id: 'claude-sonnet-4-6', label: 'Claude Sonnet 4.6', price: SONNET_4 },
	{ id: 'claude-sonnet-4-5', label: 'Claude Sonnet 4.5', price: SONNET_4 },
	{ id: 'claude-sonnet-4-0', label: 'Claude Sonnet 4', price: SONNET_4 },
	{ id: 'claude-sonnet-4', label: 'Claude Sonnet 4', price: SONNET_4 },
	{ id: 'claude-haiku-4-5', label: 'Claude Haiku 4.5', price: HAIKU_4_5 }
];

/** Used when the model id is missing or not in the table. Opus 5 list price. */
export const DEFAULT_MODEL_PRICE: ModelPrice = OPUS_5;
export const DEFAULT_PRICE_LABEL = 'Default price (model not recognised)';

const BY_ID: ReadonlyMap<string, TableEntry> = new Map(MODEL_PRICE_TABLE.map((e) => [e.id, e]));

/**
 * Reduces a captured model string to its family id: lower-case, provider prefix dropped
 * (`anthropic.`, `us.anthropic.`), and the `[1m]`, `-v1:0`, `@20251101`, `-20251001` and
 * `-latest` suffixes removed. `claude-haiku-4-5-20251001` becomes `claude-haiku-4-5`.
 */
export function normalizeModelId(model: string): string {
	let id = model.trim().toLowerCase();
	const start = id.indexOf('claude-');
	if (start > 0) id = id.slice(start);
	id = id.replace(/\[[^\]]*\]$/, '');
	id = id.replace(/-v\d+(?::\d+)?$/, '');
	id = id.replace(/@\d{8}$/, '');
	id = id.replace(/-\d{8}$/, '');
	id = id.replace(/-latest$/, '');
	return id;
}

export function lookupModelPrice(model: string | null | undefined): PriceLookup {
	if (model !== null && model !== undefined && model.trim() !== '') {
		const entry = BY_ID.get(normalizeModelId(model));
		if (entry) return { price: entry.price, label: entry.label, known: true };
	}
	return { price: DEFAULT_MODEL_PRICE, label: DEFAULT_PRICE_LABEL, known: false };
}

export type CostLineKey = 'input' | 'cacheRead' | 'cacheWrite5m' | 'cacheWrite1h' | 'output';

export interface CostLine {
	key: CostLineKey;
	label: string;
	tokens: number;
	/** US dollars per million tokens. */
	perMillion: number;
	cost: number;
}

export interface CostEstimate {
	lines: CostLine[];
	total: number;
	lookup: PriceLookup;
}

export function formatRatePerMillion(perMillion: number): string {
	return `$${perMillion.toFixed(2)}/M`;
}

export function formatCostAmount(total: number): string {
	return `$${total.toFixed(4)}`;
}

/** Plain-words note for a tooltip: where the price came from, and that it is not a bill. */
export function priceNote(lookup: PriceLookup, model: string | null | undefined): string {
	if (lookup.known) {
		return `Estimate from ${lookup.label} list prices (${formatRatePerMillion(lookup.price.input)} in, ${formatRatePerMillion(lookup.price.output)} out). Not a bill.`;
	}
	const seen = model && model.trim() !== '' ? `"${model}"` : 'this request';
	return `Estimate only. We have no price for ${seen}, so this uses a default price (${formatRatePerMillion(lookup.price.input)} in, ${formatRatePerMillion(lookup.price.output)} out). Not a bill.`;
}

export function hasAnyTokens(usage: TokenUsage | null): usage is TokenUsage {
	if (usage === null) return false;
	return (
		(usage.input_tokens ?? 0) !== 0 ||
		(usage.output_tokens ?? 0) !== 0 ||
		(usage.cache_read_input_tokens ?? 0) !== 0 ||
		(usage.cache_creation_input_tokens ?? 0) !== 0 ||
		(usage.cache_creation?.ephemeral_5m_input_tokens ?? 0) !== 0 ||
		(usage.cache_creation?.ephemeral_1h_input_tokens ?? 0) !== 0
	);
}

/**
 * Splits cache writes into the 5-minute and 1-hour parts. When the split is reported, each
 * part is priced at its own rate and any tokens it does not account for are priced as
 * 5-minute. When it is not reported, all cache writes are priced as 5-minute.
 */
function cacheWriteSplit(usage: TokenUsage): { fiveMinute: number; oneHour: number } {
	const split = usage.cache_creation;
	const total = usage.cache_creation_input_tokens ?? 0;
	if (!split) return { fiveMinute: total, oneHour: 0 };
	const five = split.ephemeral_5m_input_tokens ?? 0;
	const hour = split.ephemeral_1h_input_tokens ?? 0;
	const unaccounted = Math.max(0, total - five - hour);
	return { fiveMinute: five + unaccounted, oneHour: hour };
}

export function estimateCost(usage: TokenUsage, model: string | null | undefined): CostEstimate {
	const lookup = lookupModelPrice(model);
	const { price } = lookup;
	const writes = cacheWriteSplit(usage);
	const line = (key: CostLineKey, label: string, tokens: number, perMillion: number): CostLine => ({
		key,
		label,
		tokens,
		perMillion,
		cost: (tokens * perMillion) / 1_000_000
	});
	const lines: CostLine[] = [
		line('input', 'Uncached input', usage.input_tokens ?? 0, price.input),
		line('cacheRead', 'Cache read', usage.cache_read_input_tokens ?? 0, price.cacheRead),
		line(
			'cacheWrite5m',
			writes.oneHour > 0 ? 'Cache create (5 min)' : 'Cache create',
			writes.fiveMinute,
			price.cacheWrite5m
		)
	];
	if (writes.oneHour > 0) {
		lines.push(line('cacheWrite1h', 'Cache create (1 hour)', writes.oneHour, price.cacheWrite1h));
	}
	lines.push(line('output', 'Output', usage.output_tokens ?? 0, price.output));
	const total = lines.reduce((sum, l) => sum + l.cost, 0);
	return { lines, total, lookup };
}
