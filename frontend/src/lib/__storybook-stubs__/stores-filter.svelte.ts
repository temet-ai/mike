import type { FilterRuleCreate, FilterRuleDto, FilterRulePatch, ProxyEvent } from '../types';

export { FILTER_RULES_STORE_KEY } from '../context-keys';

export interface FilterRulesStore {
	readonly rules: FilterRuleDto[];
	readonly loading: boolean;
	readonly initialized: boolean;
	readonly error: string | null;
	scrollTargetRuleId: string | null;
	refresh(): Promise<void>;
	create(body: FilterRuleCreate): Promise<FilterRuleDto>;
	update(id: string, patch: FilterRulePatch): Promise<FilterRuleDto>;
	remove(id: string): Promise<void>;
	handleEvent(event: ProxyEvent): void;
}

export function createFilterRulesStore(): FilterRulesStore {
	let scrollTarget = $state<string | null>(null);

	return {
		get rules(): FilterRuleDto[] {
			return [];
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
			return scrollTarget;
		},
		set scrollTargetRuleId(value: string | null) {
			scrollTarget = value;
		},
		async refresh() {},
		async create(_body: FilterRuleCreate): Promise<FilterRuleDto> {
			throw new Error('createFilterRulesStore (storybook stub): create is noop');
		},
		async update(_id: string, _patch: FilterRulePatch): Promise<FilterRuleDto> {
			throw new Error('createFilterRulesStore (storybook stub): update is noop');
		},
		async remove(_id: string) {},
		handleEvent(_event: ProxyEvent) {}
	};
}
