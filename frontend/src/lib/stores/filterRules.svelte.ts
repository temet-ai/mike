import {
	ApiError,
	createFilterRule,
	deleteFilterRule,
	listFilterRules,
	patchFilterRule
} from '$lib/api';
import type { FilterRuleCreate, FilterRuleDto, FilterRulePatch, ProxyEvent } from '$lib/types';

export { FILTER_RULES_STORE_KEY } from '$lib/context-keys';

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

interface MutableState {
	rules: FilterRuleDto[];
	loading: boolean;
	initialized: boolean;
	error: string | null;
	scrollTargetRuleId: string | null;
}

export function createFilterRulesStore(): FilterRulesStore {
	const s = $state<MutableState>({
		rules: [],
		loading: false,
		initialized: false,
		error: null,
		scrollTargetRuleId: null
	});

	let inFlight: Promise<void> | null = null;

	function errorMessage(err: unknown): string {
		return err instanceof ApiError ? err.message : String(err);
	}

	/**
	 * Upserts a rule regardless of its `enabled` state (round-4/G4 fix). Disabled edits stay
	 * LISTED — with their switch shown off — so an operator can turn one back on; the previous
	 * behaviour deleted a rule from the local snapshot the moment it was disabled, making 24 of
	 * 36 real rules unreachable in the workbench. Callers that need "only what's live" (the
	 * dry-run request, "N changes this prompt") filter on `.enabled` themselves.
	 */
	function reconcileRule(rule: FilterRuleDto): void {
		const idx = s.rules.findIndex((r) => r.id === rule.id);
		if (idx === -1) {
			s.rules = [...s.rules, rule];
		} else {
			const next = [...s.rules];
			next[idx] = rule;
			s.rules = next;
		}
	}

	async function refresh(): Promise<void> {
		if (inFlight) return inFlight;
		s.loading = true;
		const promise = (async (): Promise<void> => {
			try {
				const fetched = await listFilterRules();
				s.rules = fetched;
				s.error = null;
			} catch (err) {
				s.error = errorMessage(err);
			} finally {
				s.loading = false;
				s.initialized = true;
				inFlight = null;
			}
		})();
		inFlight = promise;
		return promise;
	}

	async function create(body: FilterRuleCreate): Promise<FilterRuleDto> {
		try {
			const envelope = await createFilterRule(body);
			reconcileRule(envelope.rule);
			s.error = null;
			return envelope.rule;
		} catch (err) {
			s.error = errorMessage(err);
			throw err;
		}
	}

	async function update(id: string, patch: FilterRulePatch): Promise<FilterRuleDto> {
		try {
			const envelope = await patchFilterRule(id, patch);
			reconcileRule(envelope.rule);
			s.error = null;
			return envelope.rule;
		} catch (err) {
			s.error = errorMessage(err);
			throw err;
		}
	}

	async function remove(id: string): Promise<void> {
		try {
			await deleteFilterRule(id);
			s.rules = s.rules.filter((r) => r.id !== id);
			s.error = null;
		} catch (err) {
			if (err instanceof ApiError && err.status === 404) {
				s.rules = s.rules.filter((r) => r.id !== id);
				return;
			}
			s.error = errorMessage(err);
			throw err;
		}
	}

	function handleEvent(event: ProxyEvent): void {
		if (event.type === 'Lagged') {
			void refresh();
			return;
		}
		if (event.type === 'RowChanged' && event.table === 'filter_rules') {
			void refresh();
		}
	}

	return {
		get rules() {
			return s.rules;
		},
		get loading() {
			return s.loading;
		},
		get initialized() {
			return s.initialized;
		},
		get error() {
			return s.error;
		},
		get scrollTargetRuleId() {
			return s.scrollTargetRuleId;
		},
		set scrollTargetRuleId(value: string | null) {
			s.scrollTargetRuleId = value;
		},
		refresh,
		create,
		update,
		remove,
		handleEvent
	};
}
