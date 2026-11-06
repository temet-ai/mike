import type { DetailMetrics } from './sessionRowTypes';
import { getSession, getRequest } from '$lib/api';
import { cachePercent } from '$lib/format';
import { estimateCost, formatCostAmount, hasAnyTokens } from '$lib/modelPricing';
import type { RequestDetail, TokenUsage } from '$lib/types';

const HOVER_INTENT_MS = 150;

export interface DetailMetricsLoader {
	readonly metrics: Map<string, DetailMetrics>;
	scheduleLoad(sessionId: string): void;
	cancelLoad(sessionId: string): void;
	dispose(): void;
}

export function createDetailMetricsLoader(): DetailMetricsLoader {
	let metrics = $state<Map<string, DetailMetrics>>(new Map());
	// eslint-disable-next-line svelte/prefer-svelte-reactivity -- pending-timer bookkeeping only; never read by a template or a derived, so no reactive tracking is needed
	const hoverTimers = new Map<string, ReturnType<typeof setTimeout>>();

	function scheduleLoad(id: string): void {
		if (metrics.has(id) || hoverTimers.has(id)) return;
		const t = setTimeout(() => {
			hoverTimers.delete(id);
			void loadDetailMetrics(id);
		}, HOVER_INTENT_MS);
		hoverTimers.set(id, t);
	}

	function cancelLoad(id: string): void {
		const t = hoverTimers.get(id);
		if (t !== undefined) {
			clearTimeout(t);
			hoverTimers.delete(id);
		}
	}

	async function loadDetailMetrics(id: string): Promise<void> {
		if (metrics.has(id)) return;
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- cloned, mutated, then reassigned wholesale; reactivity comes from the reassignment, not in-place mutation
		metrics = new Map(metrics).set(id, {
			loading: true,
			error: null,
			model: null,
			inTokens: null,
			outTokens: null,
			cachePct: null,
			costStr: '—',
			costEstimated: false,
			stop: null,
			partial: false
		});
		try {
			const detail = await getSession(id);
			const ids = detail.requests.map((r) => r.id);
			const settled = await Promise.all(ids.map((rid) => getRequest(rid).catch(() => null)));
			const valid = settled.filter((d): d is RequestDetail => d !== null);

			let inSum = 0;
			let outSum = 0;
			let cacheReadSum = 0;
			let cacheCreateSum = 0;
			// eslint-disable-next-line svelte/prefer-svelte-reactivity -- local tally map inside this async function; never stored in state or read reactively
			const modelCounts = new Map<string, number>();
			for (const d of valid) {
				const u = d.token_usage;
				if (u) {
					inSum += u.input_tokens ?? 0;
					outSum += u.output_tokens ?? 0;
					cacheReadSum += u.cache_read_input_tokens ?? 0;
					cacheCreateSum += u.cache_creation_input_tokens ?? 0;
				}
				if (d.model) modelCounts.set(d.model, (modelCounts.get(d.model) ?? 0) + 1);
			}
			let dominantModel: string | null = null;
			let dominantCount = 0;
			for (const [model, count] of modelCounts) {
				if (count > dominantCount) {
					dominantModel = model;
					dominantCount = count;
				}
			}
			if (!dominantModel) {
				const models = detail.summary.models;
				dominantModel = models.length > 0 ? (models[models.length - 1] ?? null) : null;
			}
			const last = valid.length > 0 ? valid[valid.length - 1] : undefined;
			const aggUsage: TokenUsage = {
				input_tokens: inSum,
				output_tokens: outSum,
				cache_creation_input_tokens: cacheCreateSum,
				cache_read_input_tokens: cacheReadSum
			};
			let costTotal = 0;
			let costCounted = false;
			let costEstimated = false;
			for (const d of valid) {
				if (!hasAnyTokens(d.token_usage)) continue;
				const est = estimateCost(d.token_usage, d.model ?? dominantModel);
				costTotal += est.total;
				costCounted = true;
				if (!est.lookup.known) costEstimated = true;
			}
			const totalRequests = detail.total_requests ?? ids.length;

			// eslint-disable-next-line svelte/prefer-svelte-reactivity -- cloned, mutated, then reassigned wholesale; reactivity comes from the reassignment, not in-place mutation
			metrics = new Map(metrics).set(id, {
				loading: false,
				error: null,
				model: dominantModel,
				inTokens: inSum,
				outTokens: outSum,
				cachePct: cachePercent(aggUsage),
				costStr: costCounted ? formatCostAmount(costTotal) : '—',
				costEstimated,
				stop: last?.stop_reason ?? null,
				partial: totalRequests > ids.length
			});
		} catch (err) {
			// eslint-disable-next-line svelte/prefer-svelte-reactivity -- cloned, mutated, then reassigned wholesale; reactivity comes from the reassignment, not in-place mutation
			metrics = new Map(metrics).set(id, {
				loading: false,
				error: err instanceof Error ? err.message : String(err),
				model: null,
				inTokens: null,
				outTokens: null,
				cachePct: null,
				costStr: '—',
				costEstimated: false,
				stop: null,
				partial: false
			});
		}
	}

	function dispose(): void {
		for (const t of hoverTimers.values()) clearTimeout(t);
		hoverTimers.clear();
	}

	return {
		get metrics() {
			return metrics;
		},
		scheduleLoad,
		cancelLoad,
		dispose
	};
}
