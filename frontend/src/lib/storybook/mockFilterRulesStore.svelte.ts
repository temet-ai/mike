import type { FilterRuleCreate, FilterRuleDto, FilterRulePatch, ProxyEvent } from '$lib/types';
import type { FilterRulesStore } from '$lib/stores/filterRules.svelte';

/**
 * A reactive `FilterRulesStore` for stories. `$lib/stores/filterRules.svelte` is aliased in
 * Storybook (`.storybook/stub-aliases.ts`) to a stub whose mutations always throw — real
 * network I/O has no place in a story — so any story that needs a working enable-toggle,
 * create, or delete flow builds one of these instead of importing the real store or its stub.
 */
export function createMockFilterRulesStore(initial: FilterRuleDto[] = []): FilterRulesStore {
	let rules = $state<FilterRuleDto[]>(initial);
	let scrollTargetRuleId = $state<string | null>(null);
	let nextId = initial.length + 1;

	return {
		get rules() {
			return rules;
		},
		get loading() {
			return false;
		},
		get initialized() {
			return true;
		},
		get error() {
			return null;
		},
		get scrollTargetRuleId() {
			return scrollTargetRuleId;
		},
		set scrollTargetRuleId(value: string | null) {
			scrollTargetRuleId = value;
		},
		async refresh() {},
		async create(body: FilterRuleCreate): Promise<FilterRuleDto> {
			const rule: FilterRuleDto = {
				id: `mock-${nextId++}`,
				kind: body.kind,
				value: body.value,
				action: body.action,
				text: body.text ?? null,
				title: body.title ?? null,
				enabled: body.enabled ?? true,
				source: 'ui',
				updated_at: new Date().toISOString(),
				trigger_token: body.trigger_token ?? null,
				sort_order: rules.length + 1,
				fires_24h: 0,
				fires_7d: 0,
				last_fired_at: null,
				project_path: body.project_path ?? null
			};
			rules = [...rules, rule];
			return rule;
		},
		async update(id: string, patch: FilterRulePatch): Promise<FilterRuleDto> {
			let updated: FilterRuleDto | undefined;
			rules = rules.map((r) => {
				if (r.id !== id) return r;
				updated = { ...r, ...patch, updated_at: new Date().toISOString() };
				return updated;
			});
			if (updated === undefined) throw new Error(`mock store: no rule with id ${id}`);
			return updated;
		},
		async remove(id: string): Promise<void> {
			rules = rules.filter((r) => r.id !== id);
		},
		handleEvent(_event: ProxyEvent) {}
	};
}
