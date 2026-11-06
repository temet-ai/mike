import type { RequestSummary, RequestDetail } from '$lib/types';

export function buildRequestSummary(overrides: Partial<RequestSummary> = {}): RequestSummary {
	return {
		id: 42,
		method: 'POST',
		path: '/v1/messages',
		model: 'claude-opus-4-6',
		status: 'Complete',
		is_streaming: false,
		message_count: 2,
		max_tokens: 8096,
		created_at: '2026-05-04T10:00:00Z',
		completed_at: '2026-05-04T10:00:03.141Z',
		session_id: 'storybook-session',
		project: '/Users/dev/myproject',
		human_prompt: 'How do I fix this bug?',
		turn_type: 'user_prompt',
		turn_origin: 'primary',
		subagent_persona: null,
		response_tool_count: 0,
		response_tools_used: [],
		tool_names: [],
		request_number: 1,
		...overrides
	};
}

export function buildPrimaryFirstSummary(overrides: Partial<RequestSummary> = {}): RequestSummary {
	return buildRequestSummary({ turn_origin: 'primary_first', ...overrides });
}

export function buildRequestDetail(overrides: Partial<RequestDetail> = {}): RequestDetail {
	return {
		id: 42,
		request_number: 7,
		upstream_request_id: 'req_01AbcDef',
		upstream_message_id: null,
		session_id: 'storybook-session',
		project: '/Users/dev/myproject',
		human_prompt: 'How do I fix this bug?',
		turn_type: 'user_prompt',
		turn_origin: 'primary',
		subagent_persona: null,
		method: 'POST',
		path: '/v1/messages',
		request_headers: [
			['content-type', 'application/json'],
			['anthropic-version', '2023-06-01']
		],
		request_body_size: 1024,
		model: 'claude-opus-4-6',
		is_streaming: false,
		message_count: 2,
		is_tool_loop: false,
		last_user_message: 'How do I fix this bug?',
		tool_names: [],
		params: {
			max_tokens: 8096,
			temperature: null,
			thinking_enabled: false,
			thinking_budget: null,
			thinking_mode: null
		},
		status: 'Complete',
		response_status: 200,
		response_headers: [
			['content-type', 'application/json'],
			['x-request-id', 'req_01AbcDef']
		],
		response_body_size: 512,
		accumulated_text: '',
		accumulated_thinking: '',
		tool_calls: [],
		token_usage: {
			input_tokens: 1500,
			output_tokens: 250,
			cache_creation_input_tokens: 0,
			cache_read_input_tokens: 0
		},
		stop_reason: 'end_turn',
		created_at: '2026-05-04T10:00:00Z',
		completed_at: '2026-05-04T10:00:03.141Z',
		diagnostics: [],
		...overrides
	};
}
