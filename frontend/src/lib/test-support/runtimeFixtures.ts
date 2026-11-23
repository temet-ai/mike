import type {
	RuntimeCaptureExchange,
	RuntimeItem,
	RuntimeItemPage,
	RuntimeSessionObservation
} from '$lib/types';

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

export function buildRuntimeCaptureExchange(
	overrides: Partial<RuntimeCaptureExchange> = {}
): RuntimeCaptureExchange {
	return {
		id: 'b3fbc6a0-c9aa-4cef-97cb-395b4659203e',
		session_uuid: 'f5f2e486-3f2f-413f-8c4e-419227f994d3',
		provider_thread_id: 'provider-thread-1',
		created_at: '2026-11-23T13:00:00Z',
		metadata: {
			method: 'POST',
			path: '/v1/responses',
			request_headers: { 'content-type': 'application/json', 'content-encoding': 'zstd' },
			request_stored_encoding: 'identity',
			response_status: 200,
			response_headers: { 'content-type': 'text/event-stream' },
			response_stored_encoding: 'identity',
			additional_provider_field: { complete_value: 'Preserve unknown metadata fields.' }
		},
		request_document_id: 1,
		sent_document_id: 1,
		response_document_id: 2,
		status: 'complete',
		...overrides
	};
}
