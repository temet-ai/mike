import type { LlmSpendCallerDto, LlmSpendDayCallerDto, LlmSpendResponse, LlmSpendTodayDto } from '$lib/api/llmSpend';
import type { OpenRouterUsageDto } from '$lib/api/usage';

export const FIXTURE_DAYS = [
	'2026-09-22',
	'2026-09-23',
	'2026-09-24',
	'2026-09-25',
	'2026-09-26',
	'2026-09-27',
	'2026-09-28'
] as const;

const DAY_WEIGHTS = [0.12, 0.11, 0.16, 0.13, 0.14, 0.12, 0.22] as const;

export function buildFixtureCallers(): LlmSpendCallerDto[] {
	return [
		{
			key: 'steerer:verifier',
			role: 'steerer',
			steerer_id: 6,
			label: 'Steerer · verifier',
			call_count: 950,
			cost_usd: 41.8,
			prompt_tokens: 106_400_000,
			completion_tokens: 190_000,
			reasoning_tokens: 0,
			avg_cost_per_call: 0.044,
			avg_prompt_tokens: 112_000,
			failed_count: 3
		},
		{
			key: 'steerer',
			role: 'steerer',
			steerer_id: null,
			label: 'Steerer (unattributed)',
			call_count: 120,
			cost_usd: 3.2,
			prompt_tokens: 960_000,
			completion_tokens: 24_000,
			reasoning_tokens: 0,
			avg_cost_per_call: 0.0267,
			avg_prompt_tokens: 8_000,
			failed_count: 0
		},
		{
			key: 'judge_worker',
			role: 'judge_worker',
			steerer_id: null,
			label: 'Turn judge',
			call_count: 2200,
			cost_usd: 5.1,
			prompt_tokens: 7_040_000,
			completion_tokens: 88_000,
			reasoning_tokens: 0,
			avg_cost_per_call: 0.00232,
			avg_prompt_tokens: 3_200,
			failed_count: 12
		},
		{
			key: 'session_judge',
			role: 'session_judge',
			steerer_id: null,
			label: 'Session judge',
			call_count: 340,
			cost_usd: 2.4,
			prompt_tokens: 3_332_000,
			completion_tokens: 40_800,
			reasoning_tokens: 0,
			avg_cost_per_call: 0.00706,
			avg_prompt_tokens: 9_800,
			failed_count: 1
		},
		{
			key: 'eval',
			role: 'eval',
			steerer_id: null,
			label: 'Eval runs',
			call_count: 80,
			cost_usd: 1.1,
			prompt_tokens: 1_200_000,
			completion_tokens: 9_600,
			reasoning_tokens: 0,
			avg_cost_per_call: 0.01375,
			avg_prompt_tokens: 15_000,
			failed_count: 0
		},
		{
			key: 'lesson_relations',
			role: 'lesson_relations',
			steerer_id: null,
			label: 'Lesson relations',
			call_count: 60,
			cost_usd: 0.42,
			prompt_tokens: 240_000,
			completion_tokens: 4_800,
			reasoning_tokens: 0,
			avg_cost_per_call: 0.007,
			avg_prompt_tokens: 4_000,
			failed_count: 0
		}
	];
}

export function buildFixtureByDayCaller(
	callers: LlmSpendCallerDto[] = buildFixtureCallers()
): LlmSpendDayCallerDto[] {
	const rows: LlmSpendDayCallerDto[] = [];
	FIXTURE_DAYS.forEach((day, i) => {
		const weight = DAY_WEIGHTS[i] ?? 0;
		for (const caller of callers) {
			const cost_usd = Math.round(caller.cost_usd * weight * 100) / 100;
			const call_count = Math.max(1, Math.round(caller.call_count * weight));
			if (cost_usd <= 0) continue;
			rows.push({ day, key: caller.key, cost_usd, call_count });
		}
	});
	return rows;
}

export function buildFixtureToday(overrides: Partial<LlmSpendTodayDto> = {}): LlmSpendTodayDto {
	return {
		spent_usd: 7.72,
		ceiling_usd: 10,
		ceiling_enabled: true,
		alert_fraction: 0.75,
		alert_threshold_usd: 7.5,
		over_alert_threshold: true,
		over_ceiling: false,
		...overrides
	};
}

export function buildFixtureSpend(): LlmSpendResponse {
	const by_caller = buildFixtureCallers();
	const totals = by_caller.reduce(
		(acc, c) => ({
			call_count: acc.call_count + c.call_count,
			cost_usd: Math.round((acc.cost_usd + c.cost_usd) * 100) / 100,
			prompt_tokens: acc.prompt_tokens + c.prompt_tokens,
			completion_tokens: acc.completion_tokens + c.completion_tokens,
			reasoning_tokens: acc.reasoning_tokens + c.reasoning_tokens
		}),
		{ call_count: 0, cost_usd: 0, prompt_tokens: 0, completion_tokens: 0, reasoning_tokens: 0 }
	);
	return {
		window: { since: `${FIXTURE_DAYS[0]}T00:00:00Z`, until: null, defaulted: false },
		today: buildFixtureToday(),
		totals,
		by_day: [],
		by_model: [],
		by_role: [],
		by_caller,
		by_day_caller: buildFixtureByDayCaller(by_caller)
	};
}

/**
 * What a proxy build that predates the by_caller/by_day_caller addition actually returns:
 * the same response shape, just missing those two fields. Used to prove the page degrades
 * honestly instead of crashing.
 */
export function buildFixtureSpendWithoutCallerBreakdown(): LlmSpendResponse {
	const { by_caller: _by_caller, by_day_caller: _by_day_caller, ...rest } = buildFixtureSpend();
	return rest;
}

export function buildFixtureUsage(overrides: Partial<OpenRouterUsageDto> = {}): OpenRouterUsageDto {
	return {
		configured: true,
		label: 'sk-or-v1-4d2...d99',
		limit: null,
		limit_remaining: null,
		limit_reset: null,
		usage_total: 87.6,
		usage_daily: 7.72,
		usage_weekly: 54.0,
		usage_monthly: 36.13,
		credits_total: 100,
		credits_used: 87.6,
		credits_remaining: 12.4,
		is_free_tier: false,
		recorded_month_usd: 20.36,
		unrecorded_month_usd: 15.77,
		avg_daily_spend_7d_usd: 7.72,
		days_left: 1.6,
		top_up_url: 'https://openrouter.ai/settings/credits',
		...overrides
	};
}

/**
 * What a proxy build that predates the recorded/unrecorded/days-left addition actually
 * returns from /api/usage/openrouter: the original fields only. Used to prove AccountStrip
 * degrades honestly instead of crashing or printing undefined as a number.
 */
export function buildFixtureUsageWithoutSpendFields(): OpenRouterUsageDto {
	const {
		recorded_month_usd: _recorded_month_usd,
		unrecorded_month_usd: _unrecorded_month_usd,
		avg_daily_spend_7d_usd: _avg_daily_spend_7d_usd,
		days_left: _days_left,
		top_up_url: _top_up_url,
		...rest
	} = buildFixtureUsage();
	return rest;
}
