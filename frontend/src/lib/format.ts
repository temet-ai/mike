import { estimateCost, formatCostAmount, hasAnyTokens } from './modelPricing';
import type { TokenUsage } from './types';

export function shortSessionId(id: string): string {
	if (id.startsWith('hash-')) {
		return id.length > 13 ? id.slice(0, 13) : id;
	}
	return id.length > 8 ? id.slice(0, 8) : id;
}

export function formatBytes(n: number): string {
	if (n < 1024) return `${n.toLocaleString('en-US')} B`;
	if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
	if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
	return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

export function formatCost(usage: TokenUsage | null, model?: string | null): string {
	if (!hasAnyTokens(usage)) return '—';
	return formatCostAmount(estimateCost(usage, model).total);
}

export function formatUsd(value: number | null | undefined): string {
	if (value === null || value === undefined) return '—';
	return `$${value.toFixed(2)}`;
}

export function formatUsdPrecise(value: number | null | undefined, decimals = 4): string {
	if (value === null || value === undefined) return '—';
	return `$${value.toFixed(decimals)}`;
}

export function cachePercent(usage: TokenUsage | null): number | null {
	if (usage === null) return null;
	const input = usage.input_tokens ?? 0;
	const cacheRead = usage.cache_read_input_tokens ?? 0;
	const cacheCreate = usage.cache_creation_input_tokens ?? 0;
	const total = input + cacheRead + cacheCreate;
	if (total <= 0) return null;
	return Math.round(((cacheRead + cacheCreate) / total) * 100);
}
