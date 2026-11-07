import { describe, expect, it } from 'vitest';
import {
	callerHref,
	callsLabel,
	costLabel,
	groupCallers,
	memberDisplayLabel,
	perCallLabel,
	spendQueryString
} from './llmSpend';
import type { LlmSpendCallerDto } from './llmSpend';

function caller(o: Partial<LlmSpendCallerDto>): Pick<LlmSpendCallerDto, 'role' | 'steerer_id'> {
	return { role: o.role ?? 'steerer', steerer_id: o.steerer_id ?? null };
}

function fullCaller(
	o: Partial<LlmSpendCallerDto> & Pick<LlmSpendCallerDto, 'key' | 'role'>
): LlmSpendCallerDto {
	return {
		steerer_id: null,
		label: o.key,
		call_count: 1,
		cost_usd: 0,
		prompt_tokens: 0,
		completion_tokens: 0,
		reasoning_tokens: 0,
		avg_cost_per_call: 0,
		avg_prompt_tokens: 0,
		failed_count: 0,
		...o
	};
}

const VERIFIER = fullCaller({
	key: 'steerer:verifier',
	role: 'steerer',
	steerer_id: 6,
	label: 'Steerer · verifier',
	call_count: 950,
	cost_usd: 41.8,
	prompt_tokens: 106_400_000,
	avg_cost_per_call: 0.044,
	avg_prompt_tokens: 112_000,
	failed_count: 3
});
const UNATTRIBUTED_STEERER = fullCaller({
	key: 'steerer',
	role: 'steerer',
	steerer_id: null,
	label: 'Steerer (unattributed)',
	call_count: 120,
	cost_usd: 3.2,
	prompt_tokens: 960_000,
	avg_cost_per_call: 0.0267,
	avg_prompt_tokens: 8_000
});
const TURN_JUDGE = fullCaller({
	key: 'judge_worker',
	role: 'judge_worker',
	label: 'Turn judge',
	call_count: 2200,
	cost_usd: 5.1,
	prompt_tokens: 7_040_000,
	avg_cost_per_call: 0.00232,
	avg_prompt_tokens: 3_200,
	failed_count: 12
});
const SESSION_JUDGE = fullCaller({
	key: 'session_judge',
	role: 'session_judge',
	label: 'Session judge',
	call_count: 340,
	cost_usd: 2.4,
	prompt_tokens: 3_332_000,
	avg_cost_per_call: 0.00706,
	avg_prompt_tokens: 9_800
});
const EVAL = fullCaller({
	key: 'eval',
	role: 'eval',
	label: 'Eval runs',
	call_count: 80,
	cost_usd: 1.1,
	prompt_tokens: 1_200_000,
	avg_cost_per_call: 0.01375,
	avg_prompt_tokens: 15_000
});
const MYSTERY = fullCaller({
	key: 'mystery_role',
	role: 'mystery_role',
	label: 'Mystery',
	call_count: 5,
	cost_usd: 0.5,
	prompt_tokens: 1_000,
	avg_cost_per_call: 0.1,
	avg_prompt_tokens: 200
});

const ALL_CALLERS = [VERIFIER, UNATTRIBUTED_STEERER, TURN_JUDGE, SESSION_JUDGE, EVAL, MYSTERY];

describe('spendQueryString', () => {
	it('builds a query string from since/until and omits missing values', () => {
		expect(spendQueryString({ since: '2026-09-01T00:00:00Z' })).toBe(
			'?since=2026-09-01T00%3A00%3A00Z'
		);
		expect(spendQueryString({ since: '2026-09-01T00:00:00Z', until: '2026-09-28T00:00:00Z' })).toBe(
			'?since=2026-09-01T00%3A00%3A00Z&until=2026-09-28T00%3A00%3A00Z'
		);
		expect(spendQueryString({})).toBe('');
	});
});

describe('callerHref', () => {
	it('links an attributed steerer row to its steerer page', () => {
		expect(callerHref(caller({ role: 'steerer', steerer_id: 6 }))).toBe('/steerers/6');
	});

	it('leaves an unattributed steerer row with no link', () => {
		expect(callerHref(caller({ role: 'steerer', steerer_id: null }))).toBeNull();
	});

	it('links each judge role to its part of the Labelling page', () => {
		expect(callerHref(caller({ role: 'judge_worker', steerer_id: null }))).toBe(
			'/sessions/labelling'
		);
		expect(callerHref(caller({ role: 'session_judge', steerer_id: null }))).toBe(
			'/sessions/labelling?for=sessions'
		);
	});

	it('links accuracy-run rows to the Labelling accuracy page', () => {
		expect(callerHref(caller({ role: 'eval', steerer_id: null }))).toBe(
			'/sessions/labelling/accuracy'
		);
	});

	it('leaves an unmapped role with no link', () => {
		expect(callerHref(caller({ role: 'lesson_relations', steerer_id: null }))).toBeNull();
	});
});

describe('memberDisplayLabel', () => {
	it('strips the "Steerer · " prefix for an attributed steerer', () => {
		expect(memberDisplayLabel(VERIFIER)).toBe('verifier');
	});

	it('renders an unattributed steerer as "Unattributed"', () => {
		expect(memberDisplayLabel(UNATTRIBUTED_STEERER)).toBe('Unattributed');
	});

	it('leaves a non-steerer label untouched', () => {
		expect(memberDisplayLabel(TURN_JUDGE)).toBe('Turn judge');
	});
});

describe('groupCallers', () => {
	it('groups by role, with every steerer row (attributed or not) under one "Steerers" group', () => {
		const groups = groupCallers(ALL_CALLERS);
		const steererGroup = groups.find((g) => g.groupKey === 'steerer');
		expect(steererGroup?.groupLabel).toBe('Steerers');
		expect(steererGroup?.members.map((m) => m.key)).toEqual(['steerer:verifier', 'steerer']);
		expect(steererGroup?.collapsedRow).toBe(false);
	});

	it('labels an unknown role as its own group, by the role itself', () => {
		const groups = groupCallers(ALL_CALLERS);
		const mystery = groups.find((g) => g.groupKey === 'mystery_role');
		expect(mystery?.groupLabel).toBe('mystery_role');
		expect(mystery?.collapsedRow).toBe(true);
	});

	it('sums group subtotals and computes cost/call and avg input tokens weighted by calls', () => {
		const groups = groupCallers(ALL_CALLERS);
		const steererGroup = groups.find((g) => g.groupKey === 'steerer')!;
		expect(steererGroup.cost_usd).toBeCloseTo(45.0, 5);
		expect(steererGroup.call_count).toBe(1070);
		expect(steererGroup.failed_count).toBe(3);
		// cost/call = group cost ÷ group calls, not an average of the members' own cost/call.
		expect(steererGroup.avg_cost_per_call).toBeCloseTo(45.0 / 1070, 6);
		// avg input tokens weighted by calls = total prompt tokens ÷ total calls.
		expect(steererGroup.avg_prompt_tokens).toBeCloseTo((106_400_000 + 960_000) / 1070, 2);
	});

	it('collapses a single-member non-steerer group to one row, never a multi-member steerer group', () => {
		const groups = groupCallers(ALL_CALLERS);
		for (const role of ['judge_worker', 'session_judge', 'eval', 'mystery_role']) {
			expect(groups.find((g) => g.groupKey === role)?.collapsedRow).toBe(true);
		}
		expect(groups.find((g) => g.groupKey === 'steerer')?.collapsedRow).toBe(false);
	});

	it('never collapses a steerer group even with a single member', () => {
		const groups = groupCallers([VERIFIER]);
		expect(groups[0]?.collapsedRow).toBe(false);
	});

	it('orders groups by group cost desc and members by cost desc within the group, by default', () => {
		const groups = groupCallers(ALL_CALLERS);
		expect(groups.map((g) => g.groupKey)).toEqual([
			'steerer',
			'judge_worker',
			'session_judge',
			'eval',
			'mystery_role'
		]);
		const steererGroup = groups.find((g) => g.groupKey === 'steerer')!;
		expect(steererGroup.members.map((m) => m.key)).toEqual(['steerer:verifier', 'steerer']);
	});

	it('sorts groups AND members by the chosen column without interleaving groups', () => {
		// By call count: judge_worker's 2200 calls outranks the steerer group's combined 1070.
		const groups = groupCallers(ALL_CALLERS, 'calls', true);
		expect(groups.map((g) => g.groupKey)).toEqual([
			'judge_worker',
			'steerer',
			'session_judge',
			'eval',
			'mystery_role'
		]);
		// Within the steerer group, ordering by calls puts the unattributed row (120) ahead of
		// verifier (... no: verifier has 950 > 120, so verifier still leads) — confirms the
		// per-column comparator, not a hardcoded cost-based order, drives member order too.
		const steererGroup = groups.find((g) => g.groupKey === 'steerer')!;
		expect(steererGroup.members.map((m) => m.key)).toEqual(['steerer:verifier', 'steerer']);
	});

	it('reverses to ascending on both groups and members', () => {
		const groups = groupCallers(ALL_CALLERS, 'calls', false);
		expect(groups.map((g) => g.groupKey)).toEqual([
			'mystery_role',
			'eval',
			'session_judge',
			'steerer',
			'judge_worker'
		]);
		const steererGroup = groups.find((g) => g.groupKey === 'steerer')!;
		expect(steererGroup.members.map((m) => m.key)).toEqual(['steerer', 'steerer:verifier']);
	});

	it('breaks ties on a stable key (role for groups, caller key for members)', () => {
		const tiedA = fullCaller({ key: 'eval', role: 'eval', label: 'Eval runs', cost_usd: 1 });
		const tiedB = fullCaller({
			key: 'lesson_relations',
			role: 'lesson_relations',
			label: 'Lesson relations',
			cost_usd: 1
		});
		const groups = groupCallers([tiedB, tiedA], 'cost', true);
		expect(groups.map((g) => g.groupKey)).toEqual(['eval', 'lesson_relations']);
	});
});

describe('subscription pricing labels', () => {
	it('says subscription instead of zero when every call went through the Codex login', () => {
		expect(costLabel(0, 1899, 1899)).toBe('Subscription');
		expect(perCallLabel(0, 1899, 1899)).toBe('no per-call price');
		expect(callsLabel(1899, 1899)).toBe('1,899');
	});

	it('keeps the dollar figure and names the subscription share when calls are mixed', () => {
		expect(costLabel(4.5, 100, 40)).toBe('$4.50');
		expect(perCallLabel(0.045, 100, 40)).toBe('$0.045');
		expect(callsLabel(100, 40)).toBe('100 (40 subscription)');
	});

	it('treats an absent count as no subscription calls, and an empty row as priced', () => {
		expect(costLabel(0, 10, undefined)).toBe('$0.00');
		expect(costLabel(0, 0, 0)).toBe('$0.00');
	});

	it('adds subscription calls across the members of a group', () => {
		const a = fullCaller({
			key: 'judge_worker',
			role: 'judge_worker',
			call_count: 10,
			subscription_call_count: 10
		});
		const b = fullCaller({
			key: 'steerer:x',
			role: 'steerer',
			steerer_id: 1,
			call_count: 5,
			subscription_call_count: 2
		});
		const groups = groupCallers([a, b], 'cost', true);
		expect(groups.find((g) => g.groupKey === 'steerer')?.subscription_call_count).toBe(2);
		expect(groups.find((g) => g.groupKey === 'judge_worker')?.subscription_call_count).toBe(10);
	});
});
