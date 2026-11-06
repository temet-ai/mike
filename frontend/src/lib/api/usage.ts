import { ApiError } from '$lib/api';
import { cachedGet } from '$lib/stores/cache/apiCache.svelte';

export interface OpenRouterUsageDto {
	configured: boolean;
	label: string | null;
	limit: number | null;
	limit_remaining: number | null;
	limit_reset: string | null;
	usage_total: number | null;
	usage_daily: number | null;
	usage_weekly: number | null;
	usage_monthly: number | null;
	credits_total: number | null;
	credits_used: number | null;
	credits_remaining: number | null;
	is_free_tier: boolean | null;
	credits_unavailable?: boolean;
	recorded_month_usd?: number;
	unrecorded_month_usd?: number;
	avg_daily_spend_7d_usd?: number;
	days_left?: number | null;
	top_up_url?: string;
}

export interface ModelUsageRow {
	model: string | null;
	requests: number;
	input_tokens: number;
	output_tokens: number;
	cache_creation_tokens: number;
	cache_read_tokens: number;
	last_seen: string;
}

export interface ModelUsageResponse {
	days: number;
	models: ModelUsageRow[];
}

export const USAGE_MODELS_DAYS = 30;

async function getJson<T>(path: string): Promise<T> {
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as T;
	});
}

export async function getOpenRouterUsage(): Promise<OpenRouterUsageDto> {
	return getJson<OpenRouterUsageDto>('/api/usage/openrouter');
}

export async function getModelUsage(days = 30): Promise<ModelUsageResponse> {
	return getJson<ModelUsageResponse>(`/api/usage/models?days=${days}`);
}
