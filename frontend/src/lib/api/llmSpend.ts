import { ApiError } from '$lib/api';
import { formatUsd, formatUsdPrecise } from '$lib/format';
import { cachedGet } from '$lib/stores/cache/apiCache.svelte';

export interface LlmSpendWindowDto {
	since: string | null;
	until: string | null;
	defaulted: boolean;
}

export interface LlmSpendTodayDto {
	spent_usd: number;
	ceiling_usd: number;
	ceiling_enabled: boolean;
	alert_fraction: number;
	alert_threshold_usd: number | null;
	over_alert_threshold: boolean;
	over_ceiling: boolean;
}

export interface LlmSpendGroupDto {
	key: string;
	call_count: number;
	cost_usd: number;
	prompt_tokens: number;
	completion_tokens: number;
	reasoning_tokens: number;
	/** Calls made through the machine's Codex login: no per-call price exists. Absent on older builds. */
	subscription_call_count?: number;
}

export interface LlmSpendTotalsDto {
	call_count: number;
	cost_usd: number;
	prompt_tokens: number;
	completion_tokens: number;
	reasoning_tokens: number;
	subscription_call_count?: number;
}

export type LlmSpendCallerRole =
	| 'steerer'
	| 'judge_worker'
	| 'session_judge'
	| 'eval'
	| 'lesson_relations'
	| string;

export interface LlmSpendCallerDto {
	key: string;
	role: LlmSpendCallerRole;
	steerer_id: number | null;
	label: string;
	call_count: number;
	cost_usd: number;
	prompt_tokens: number;
	completion_tokens: number;
	reasoning_tokens: number;
	avg_cost_per_call: number;
	avg_prompt_tokens: number;
	failed_count: number;
	subscription_call_count?: number;
}

export interface LlmSpendDayCallerDto {
	day: string;
	key: string;
	cost_usd: number;
	call_count: number;
}

export interface LlmSpendResponse {
	window: LlmSpendWindowDto;
	today: LlmSpendTodayDto;
	totals: LlmSpendTotalsDto;
	by_day: LlmSpendGroupDto[];
	by_model: LlmSpendGroupDto[];
	by_role: LlmSpendGroupDto[];
	/** Absent on a proxy build that predates the per-caller breakdown — never assume present. */
	by_caller?: LlmSpendCallerDto[];
	/** Absent on a proxy build that predates the per-caller breakdown — never assume present. */
	by_day_caller?: LlmSpendDayCallerDto[];
}

export interface LlmSpendParams {
	since?: string;
	until?: string;
}

export const LLM_SPEND_WINDOW_OPTIONS = [7, 30, 90] as const;
export type LlmSpendWindowDays = (typeof LLM_SPEND_WINDOW_OPTIONS)[number];

export function llmSpendSinceForDays(days: number): string {
	return new Date(Date.now() - days * 24 * 60 * 60 * 1000).toISOString();
}

export function spendQueryString(params: LlmSpendParams): string {
	const qs = new URLSearchParams();
	if (params.since) qs.set('since', params.since);
	if (params.until) qs.set('until', params.until);
	const query = qs.toString();
	return query ? `?${query}` : '';
}

async function getJson<T>(path: string): Promise<T> {
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as T;
	});
}

export async function getLlmSpend(params: LlmSpendParams = {}): Promise<LlmSpendResponse> {
	return getJson<LlmSpendResponse>(`/api/llm/spend${spendQueryString(params)}`);
}

const CALLER_HREF_BY_ROLE: Partial<Record<string, string>> = {
	judge_worker: '/sessions/labelling',
	session_judge: '/sessions/labelling?for=sessions',
	eval: '/sessions/labelling/accuracy'
};

export function callerHref(caller: Pick<LlmSpendCallerDto, 'role' | 'steerer_id'>): string | null {
	if (caller.role === 'steerer') {
		return caller.steerer_id !== null ? `/steerers/${caller.steerer_id}` : null;
	}
	return CALLER_HREF_BY_ROLE[caller.role] ?? null;
}

const GROUP_LABEL_BY_ROLE: Partial<Record<string, string>> = {
	steerer: 'Steerers',
	judge_worker: 'Turn judge',
	session_judge: 'Session judge',
	eval: 'Eval runs',
	lesson_relations: 'Lesson relations'
};

export function groupLabelForRole(role: string): string {
	return GROUP_LABEL_BY_ROLE[role] ?? role;
}

const STEERER_LABEL_PREFIX = 'Steerer · ';
const STEERER_UNATTRIBUTED_LABEL = 'Steerer (unattributed)';

/**
 * The child-row label within a Steerers group: just the steerer's own name, since the group
 * header already says "Steerers". Every other role's members keep their full label.
 */
export function memberDisplayLabel(caller: Pick<LlmSpendCallerDto, 'role' | 'label'>): string {
	if (caller.role !== 'steerer') return caller.label;
	if (caller.label === STEERER_UNATTRIBUTED_LABEL) return 'Unattributed';
	return caller.label.startsWith(STEERER_LABEL_PREFIX)
		? caller.label.slice(STEERER_LABEL_PREFIX.length)
		: caller.label;
}

export type LlmSpendCallerSortKey =
	| 'cost'
	| 'calls'
	| 'share'
	| 'cost_per_call'
	| 'avg_input'
	| 'failed';

export const CALLER_SORT_ACCESSOR: Record<LlmSpendCallerSortKey, (c: LlmSpendCallerDto) => number> =
	{
		cost: (c) => c.cost_usd,
		calls: (c) => c.call_count,
		share: (c) => c.cost_usd,
		cost_per_call: (c) => c.avg_cost_per_call,
		avg_input: (c) => c.avg_prompt_tokens,
		failed: (c) => c.failed_count
	};

export interface LlmSpendCallerGroup {
	groupKey: string;
	groupLabel: string;
	members: LlmSpendCallerDto[];
	cost_usd: number;
	call_count: number;
	avg_cost_per_call: number;
	avg_prompt_tokens: number;
	failed_count: number;
	subscription_call_count: number;
	/** True for a single-member, non-steerer group: render one plain row, no header. */
	collapsedRow: boolean;
}

const GROUP_SORT_ACCESSOR: Record<LlmSpendCallerSortKey, (g: LlmSpendCallerGroup) => number> = {
	cost: (g) => g.cost_usd,
	calls: (g) => g.call_count,
	share: (g) => g.cost_usd,
	cost_per_call: (g) => g.avg_cost_per_call,
	avg_input: (g) => g.avg_prompt_tokens,
	failed: (g) => g.failed_count
};

export const SUBSCRIPTION_COST_LABEL = 'Subscription';
export const SUBSCRIPTION_PER_CALL_LABEL = 'no per-call price';

function allOnSubscription(callCount: number, subscriptionCalls: number | undefined): boolean {
	return callCount > 0 && (subscriptionCalls ?? 0) >= callCount;
}

export function costLabel(
	costUsd: number,
	callCount: number,
	subscriptionCalls: number | undefined
): string {
	return allOnSubscription(callCount, subscriptionCalls)
		? SUBSCRIPTION_COST_LABEL
		: formatUsd(costUsd);
}

export function perCallLabel(
	avgCostPerCall: number,
	callCount: number,
	subscriptionCalls: number | undefined
): string {
	return allOnSubscription(callCount, subscriptionCalls)
		? SUBSCRIPTION_PER_CALL_LABEL
		: formatUsdPrecise(avgCostPerCall, 3);
}

export function callsLabel(callCount: number, subscriptionCalls: number | undefined): string {
	const subscription = subscriptionCalls ?? 0;
	const base = callCount.toLocaleString();
	return subscription > 0 && subscription < callCount
		? `${base} (${subscription.toLocaleString()} subscription)`
		: base;
}

function sumBy<T>(items: T[], f: (item: T) => number): number {
	return items.reduce((acc, item) => acc + f(item), 0);
}

function round2(n: number): number {
	return Math.round(n * 100) / 100;
}

/**
 * Groups by_caller rows by role — Steerers (every "steerer" row, attributed or not), Turn judge,
 * Session judge, Eval runs, Lesson relations, and any unknown role as its own group labelled by
 * the role. Groups are ordered by the given sort key/direction (group value), and each group's
 * members are ordered by the same key/direction within the group — groups are never interleaved.
 * A non-steerer group with exactly one member is marked `collapsedRow` so the caller renders it
 * as a single plain row instead of a header with one child.
 */
export function groupCallers(
	callers: LlmSpendCallerDto[],
	sortKey: LlmSpendCallerSortKey = 'cost',
	sortDesc = true
): LlmSpendCallerGroup[] {
	const byRole = new Map<string, LlmSpendCallerDto[]>();
	for (const c of callers) {
		const list = byRole.get(c.role) ?? [];
		list.push(c);
		byRole.set(c.role, list);
	}

	const memberAccessor = CALLER_SORT_ACCESSOR[sortKey];
	const dir = sortDesc ? -1 : 1;

	const groups: LlmSpendCallerGroup[] = [...byRole.entries()].map(([role, members]) => {
		const sortedMembers = [...members].sort((a, b) => {
			const diff = memberAccessor(a) - memberAccessor(b);
			if (diff !== 0) return diff * dir;
			return a.key.localeCompare(b.key);
		});
		const cost_usd = round2(sumBy(members, (c) => c.cost_usd));
		const call_count = sumBy(members, (c) => c.call_count);
		const prompt_tokens = sumBy(members, (c) => c.prompt_tokens);
		const failed_count = sumBy(members, (c) => c.failed_count);
		const subscription_call_count = sumBy(members, (c) => c.subscription_call_count ?? 0);
		return {
			groupKey: role,
			groupLabel: groupLabelForRole(role),
			members: sortedMembers,
			cost_usd,
			call_count,
			avg_cost_per_call: call_count > 0 ? cost_usd / call_count : 0,
			avg_prompt_tokens: call_count > 0 ? prompt_tokens / call_count : 0,
			failed_count,
			subscription_call_count,
			collapsedRow: role !== 'steerer' && members.length === 1
		};
	});

	const groupAccessor = GROUP_SORT_ACCESSOR[sortKey];
	groups.sort((a, b) => {
		const diff = groupAccessor(a) - groupAccessor(b);
		if (diff !== 0) return diff * dir;
		return a.groupKey.localeCompare(b.groupKey);
	});

	return groups;
}
