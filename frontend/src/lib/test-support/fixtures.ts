import type {
	BodyPayload,
	FilterRuleDto,
	Finding,
	PromptClassDetailDto,
	PromptClassDto,
	PromptComposition,
	PromptCompositionListItem,
	PromptCompositionMutationEnvelope,
	RequestDetail,
	StatsResponse
} from '$lib/types';

export function buildBodyPayload(byteSize = 100, text = '{"hello":"world"}'): BodyPayload {
	return { text, byte_size: byteSize, has_body: true };
}

export function buildSummaryRow(id: number) {
	return {
		id,
		method: 'POST',
		path: '/v1/messages',
		status: 'Complete',
		session_id: 'sess-1',
		created_at: '2026-01-01T00:00:00Z'
	};
}

export function buildStatsResponse(overrides: Partial<StatsResponse> = {}): StatsResponse {
	return {
		request_count: 0,
		active_streams: 0,
		uptime_seconds: 0,
		db_enabled: true,
		...overrides
	};
}

export function buildCacheRequestDetail(
	id: number,
	status: RequestDetail['status'] = 'Complete'
): RequestDetail {
	return {
		id,
		session_id: 'sess-1',
		status,
		is_streaming: false,
		model: 'claude-sonnet-4-5',
		method: 'POST',
		path: '/v1/messages',
		accumulated_text: '',
		tool_calls: [],
		diagnostics: []
	} as unknown as RequestDetail;
}

export function buildFilterRule(overrides: Partial<FilterRuleDto> = {}): FilterRuleDto {
	return {
		id: 'rule-1',
		kind: 'xml_tag',
		value: 'system-reminder',
		action: 'drop',
		text: null,
		title: null,
		enabled: true,
		source: 'ui',
		updated_at: '2026-04-24T07:00:00.000Z',
		sort_order: 1,
		trigger_token: null,
		fires_24h: 0,
		fires_7d: 0,
		last_fired_at: null,
		...overrides
	};
}

export function buildFinding(overrides: Partial<Finding> = {}): Finding {
	return {
		request_id: 1,
		session_id: 'session-a',
		category: 'deflecting_blame',
		analyzer: 'deflecting_blame_analyzer',
		advice: 'run git blame',
		matched_text: 'pre-existing',
		match_range: [0, 12],
		other_hits: 0,
		created_at: '2026-04-18T13:16:29.199371+00:00',
		request: {
			model: 'claude-opus-4-7',
			status: 'Complete',
			path: '/v1/messages',
			project: 'ai/tools/proxy',
			human_prompt: 'fix the bug',
			created_at: '2026-04-18T13:16:22.000000+00:00'
		},
		...overrides
	};
}

export function buildCompositionListItem(
	overrides: Partial<PromptCompositionListItem> = {}
): PromptCompositionListItem {
	return {
		id: 'comp-1',
		prompt_sha256: 'a'.repeat(64),
		label: 'Default',
		version: 1,
		block_count: 3,
		total_chars: 240,
		applied_count_24h: 0,
		updated_at: '2026-04-25T07:00:00.000Z',
		agent_name: null,
		...overrides
	};
}

export function buildPromptComposition(
	overrides: Partial<PromptComposition> = {}
): PromptComposition {
	return {
		id: 'comp-1',
		prompt_sha256: 'a'.repeat(64),
		label: 'Default',
		version: 1,
		blocks: [
			{ index: 0, text: 'block zero text', cache_control: null },
			{ index: 1, text: 'block one text', cache_control: null },
			{ index: 2, text: 'block two text', cache_control: null }
		],
		mode: 'observe',
		notes: null,
		agent_name: null,
		agent_definition_path: null,
		created_at: '2026-04-25T07:00:00.000Z',
		updated_at: '2026-04-25T07:00:00.000Z',
		applied_count_24h: 0,
		...overrides
	};
}

export function buildCompositionMutationEnvelope(
	composition: PromptComposition,
	warnings: string[] = []
): PromptCompositionMutationEnvelope {
	return { composition, warnings };
}

export function buildPromptClassDto(overrides: Partial<PromptClassDto> = {}): PromptClassDto {
	return {
		lane: 'main',
		subagent_persona: null,
		observation_count: 0,
		first_seen: null,
		last_seen: null,
		block_count: 0,
		registration: {
			registered: false,
			composition_id: null,
			label: null,
			mode: null,
			agent_definition_path: null
		},
		model_breakdown: [],
		...overrides
	};
}

export function buildPromptClassDetailDto(
	overrides: Partial<PromptClassDetailDto> = {}
): PromptClassDetailDto {
	return {
		lane: 'main',
		subagent_persona: null,
		observation_count: 0,
		first_seen: null,
		last_seen: null,
		model_breakdown: [],
		sample_blocks: null,
		registration: {
			registered: false,
			composition_id: null,
			label: null,
			mode: null,
			agent_definition_path: null
		},
		recent_observations: [],
		observations_total_count: 0,
		observations_offset: 0,
		observations_limit: 1,
		...overrides
	};
}
