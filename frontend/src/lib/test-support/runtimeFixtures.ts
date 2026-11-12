import type { RuntimeItem, RuntimeItemPage, RuntimeSessionObservation } from '$lib/types';

export function buildRuntimeObservation(
	overrides: Partial<RuntimeSessionObservation> = {}
): RuntimeSessionObservation {
	return {
		runtime: 'fixture_runtime',
		source: 'fixture_adapter',
		source_version: '1',
		fidelity: 'normalized_metadata',
		completeness: 'metadata_only',
		observed_at: '2026-11-12T13:00:00Z',
		lifecycle: 'idle',
		control: 'hook_boundary',
		requested_model: 'requested-model',
		observed_model: null,
		usage_scope: 'own_thread',
		usage_basis: 'cumulative',
		input_tokens: null,
		cached_input_tokens: null,
		output_tokens: 0,
		reasoning_output_tokens: null,
		total_tokens: null,
		context_window: null,
		model_call_count: 1,
		...overrides
	};
}

export function buildRuntimeItem(overrides: Partial<RuntimeItem> = {}): RuntimeItem {
	return {
		item_id: 'tool-1',
		turn_id: 'turn-1',
		kind: 'command',
		name: 'cargo check',
		status: 'completed',
		success: true,
		exit_code: 0,
		source: 'fixture_adapter',
		fidelity: 'normalized_metadata',
		completeness: 'metadata_only',
		first_seen_at: '2026-11-12T12:59:00Z',
		observed_at: '2026-11-12T13:00:00Z',
		content_availability: 'not_collected',
		...overrides
	};
}

export function buildRuntimeItemPage(overrides: Partial<RuntimeItemPage> = {}): RuntimeItemPage {
	return { items: [buildRuntimeItem()], offset: 0, limit: 1, has_more: true, ...overrides };
}
