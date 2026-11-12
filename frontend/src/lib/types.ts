import type { TurnOrigin } from './turn-origins';
import type { Authority, SteeringMode } from './steering';

export type { TurnOrigin };
export type { Authority, SteeringMode };

export interface CaptureGapHealth {
	// Mirrors api/handlers/health/gap.rs; absence is possible on older servers.
	silence_threshold_secs: number;
	sidecar_recent_secs: number;
	lookback_secs: number;
	alarm: boolean;
	sessions: Array<{
		session_id: string;
		last_work_at: string | null;
		last_sidecar_at: string;
		work_silent_secs: number;
	}>;
	reading_age_secs: number | null;
}

export interface HealthResponse {
	status: 'ok' | 'degraded';
	capture_gap?: CaptureGapHealth;
}

export interface RequestSummary {
	id: number;
	method: string;
	path: string;
	model: string | null;
	status: string;
	is_streaming: boolean;
	message_count: number;
	max_tokens: number | null;
	created_at: string;
	completed_at: string | null;
	session_id: string;
	project: string | null;
	human_prompt: string;
	turn_type: 'user_prompt' | 'tool_loop' | 'continuation';
	turn_origin?: TurnOrigin;
	subagent_persona?: string | null;
	agent_instance_id?: number | null;
	response_tool_count: number;
	response_tools_used: string[];
	tool_names: string[];
	predecessor_request_id?: number | null;
	request_number?: number | null;
}

export interface StatsResponse {
	request_count: number;
	active_streams: number;
	uptime_seconds: number;
	db_enabled?: boolean;
}

export type DiagnosticCategory =
	| 'justifying_shortcuts'
	| 'deflecting_blame'
	| 'avoiding_work'
	| 'faking_understanding'
	| 'overselling';

export interface Diagnostic {
	category: DiagnosticCategory;
	analyzer: string;
	advice: string;
	matched_text: string;
	match_range: [number, number];
	other_hits: number;
	created_at: string;
}

export interface DiagnosticStats {
	totals_by_category: Partial<Record<DiagnosticCategory, number>>;
	totals_by_analyzer: Record<string, number>;
	last_24h: number;
}

export interface FindingRequestRef {
	model: string | null;
	status: string;
	path: string;
	project: string | null;
	human_prompt: string;
	created_at: string;
}

export interface Finding {
	request_id: number;
	session_id: string;
	category: DiagnosticCategory;
	analyzer: string;
	advice: string;
	matched_text: string;
	match_range: [number, number];
	other_hits: number;
	created_at: string;
	request: FindingRequestRef;
}

export interface CacheCreationSplit {
	ephemeral_5m_input_tokens?: number | null;
	ephemeral_1h_input_tokens?: number | null;
}

export interface TokenUsage {
	input_tokens: number;
	output_tokens: number;
	cache_creation_input_tokens: number | null;
	cache_read_input_tokens: number | null;
	cache_creation?: CacheCreationSplit | null;
}

export interface RequestParams {
	max_tokens: number | null;
	temperature: number | null;
	thinking_enabled: boolean;
	thinking_budget: number | null;
	thinking_mode: string | null;
	effort?: string | null;
}

export interface ToolCall {
	tool_id: string;
	name: string;
	input_json: string;
	complete: boolean;
}

export type RequestStatus = 'Pending' | 'Streaming' | 'Complete' | { Error: string };

export type SystemPromptSource =
	'exact' | 'class' | 'agent_instance' | 'session_main' | 'not_captured';

export interface RequestDetail {
	id: number;
	upstream_request_id: string | null;
	upstream_message_id: string | null;
	session_id: string;
	request_number?: number | null;
	project: string | null;
	human_prompt: string;
	turn_type: string;
	turn_origin?: string;
	subagent_persona?: string | null;

	method: string;
	path: string;
	request_headers: Array<[string, string]>;
	request_body_size: number;
	model: string | null;
	is_streaming: boolean;
	message_count: number;
	is_tool_loop: boolean;
	last_user_message: string;
	tool_names: string[];
	params: RequestParams;

	status: RequestStatus;
	response_status: number | null;
	response_headers: Array<[string, string]> | null;
	response_body_size: number;

	accumulated_text?: string;
	accumulated_text_truncated?: boolean;
	accumulated_text_sha256?: string | null;
	accumulated_thinking?: string;
	tool_calls: ToolCall[];
	token_usage: TokenUsage | null;
	stop_reason: string | null;

	created_at: string;
	completed_at: string | null;

	diagnostics?: Diagnostic[];

	system_blocks?: SystemBlocksProjection;
	system_prompt_source?: SystemPromptSource;
	composition_applied?: boolean;
	unknown_blocks?: Record<string, unknown> | null;
	filter_modified?: boolean;
	firing_rule_ids?: string[];
	predecessor_request_id?: number | null;
}

export type AgentSource = 'observed' | 'file_backed' | 'dynamic';

export interface Agent {
	id: string;
	name: string;
	definition_path: string | null;
	description: string | null;
	source: AgentSource;
	created_at: string;
	updated_at: string;
}

export interface AgentListItem extends Agent {
	version_count: number;
}

export interface AgentDetail extends Agent {
	compositions: PromptComposition[];
}

export type AgentPatch = Partial<{
	name: string;
	definition_path: string | null;
	description: string | null;
	source: AgentSource;
}>;

export interface SessionOverviewTurnDto {
	turn_id: number;
	user_prompt: string;
	stop_response?: string;
	stop_reason: string | null;
	stop_request_id: number | null;
	interrupted: boolean;
	outcome: string;
	start_request_id: number;
	end_request_id: number;
	start_request_number: number;
	end_request_number: number;
	created_at: string;
	closed_at: string | null;
	prompt_kind?: string;
	subject_command?: string;
	subject_agent?: string;
	model?: string;
	reaction: string | null;
	delivery: string | null;
	outcome_source: string | null;
	live_verdict_id: number | null;
	outcome_projection_version: number | null;
	legacy: boolean;
	pending: boolean;
}

export interface SessionSummary {
	id: string;
	project: string | null;
	project_id: number | null;
	cwd: string | null;
	project_resolution: 'unresolved' | 'git' | 'operator';
	first_seen: string;
	last_seen: string;
	request_count: number;
	models: string[];
	last_status: string;
	title: string | null;
	pinned?: boolean;
	avg_latency_ms?: number | null;
	originating_operator_prompt?: string | null;
	narrative_overview?: string | null;
	narrative_overview_generated_at?: string | null;
	pending_inbox_count?: number;
}

export interface SessionProjectAssignmentResponse {
	session_id: string;
	project_id: number;
	project_path: string;
	project_resolution: 'operator';
	cwd: string | null;
}

export interface SessionQuality {
	outcome: 'success' | 'partial' | 'failure' | 'unknown';
	quality_score: number | null;
	rationale: string | null;
	judge_model: string | null;
	rubric_version: number | null;
	judged_at: string | null;
	bundle_id?: number | null;
	verdict_id?: number;
	end_turn_id?: number | null;
	end_label?: string | null;
	end_source?: string | null;
	end_rule?: string | null;
	end_confidence?: string | null;
	end_evidence?: string | null;
	failure?: string | null;
	failure_tags?: string[];
	severity?: string | null;
}

export interface ModelBreakdownEntry {
	model: string;
	count: number;
}

export interface ModelTokenUsage {
	model: string;
	response_count: number;
	input_tokens: number;
	output_tokens: number;
	cache_read_tokens: number;
	cache_write_tokens: number;
}

export interface SessionTokenUsage {
	input_tokens: number;
	output_tokens: number;
	cache_read_tokens: number;
	cache_write_tokens: number;
	response_count: number;
	by_model: ModelTokenUsage[];
}

export interface SessionDetail {
	summary: SessionSummary;
	requests: RequestSummary[];
	observation?: RuntimeSessionObservation | null;
	runtime_items?: RuntimeItemPage | null;
	quality?: SessionQuality | null;
	model_breakdown?: ModelBreakdownEntry[];
	token_usage?: SessionTokenUsage;
	total_requests?: number;
	offset?: number;
	limit?: number;
}

export interface RuntimeSessionObservation {
	runtime: string;
	source: string;
	source_version: string | null;
	fidelity: string;
	completeness: string;
	observed_at: string;
	lifecycle: string | null;
	control: string | null;
	requested_model: string | null;
	observed_model: string | null;
	usage_scope: string | null;
	usage_basis: string | null;
	input_tokens: number | null;
	cached_input_tokens: number | null;
	output_tokens: number | null;
	reasoning_output_tokens: number | null;
	total_tokens: number | null;
	context_window: number | null;
	model_call_count: number;
}

export interface RuntimeItem {
	item_id: string;
	turn_id: string | null;
	kind: string;
	name: string | null;
	status: string | null;
	success: boolean | null;
	exit_code: number | null;
	source: string;
	fidelity: string;
	completeness: string;
	first_seen_at: string;
	observed_at: string;
	content_availability: string;
}

export interface RuntimeItemPage {
	items: RuntimeItem[];
	offset: number;
	limit: number;
	has_more: boolean;
}

export interface SessionHead {
	session_id: string;
	last_status?: string;
	newest_request_id?: number;
	newest_request_number?: number;
	originating_operator_prompt?: string;
	prompt_kind_counts: Record<string, number>;
}

export interface BodyPayload {
	text: string | null;
	byte_size: number;
	has_body: boolean;
	reconstructed?: boolean;
}

export type ManifestOutcome = 'applied' | 'unchanged' | 'skipped' | 'failed_closed';

export interface ManifestByteRange {
	start: number;
	end: number;
}

export interface ManifestTarget {
	outcome: ManifestOutcome;
	content_kind: string;
	json_path: string | null;
	tool_use_id: string | null;
	producer: string | null;
	input_range: ManifestByteRange;
	output_range: ManifestByteRange;
	original_bytes: number;
	sent_bytes: number;
	original_tokens: number | null;
	sent_tokens: number | null;
	original_content_sha256: string;
	sent_content_sha256: string;
	recovery_ref: string | null;
	reason_code: string;
	detail: string | null;
}

export interface ManifestStage {
	ordinal: number;
	stage: string;
	outcome: ManifestOutcome;
	policy_id: string;
	policy_version: number;
	resolved_parameters: unknown;
	input_sha256: string;
	output_sha256: string;
	targets: ManifestTarget[];
	reason_code: string;
	detail: string | null;
}

export interface TransformationManifest {
	schema_version: number;
	original_sha256: string;
	sent_sha256: string;
	stages: ManifestStage[];
}

export interface TransformationManifestPayload {
	request_id: number;
	has_manifest: boolean;
	manifest: TransformationManifest | null;
	pointer?: string;
	pointer_resolved?: boolean;
}

export interface UnifiedTextMatch {
	request_id: number;
	session_id: string;
	source: 'request' | 'request_sent' | 'response';
	byte_offset: number;
	match_length: number;
	line: number;
	column: number;
	before: string;
	matched: string;
	after: string;
}

export interface FindTextUnifiedResponse {
	needle: string;
	case_sensitive: boolean;
	context_bytes: number;
	full_match_count: number;
	matches_truncated: boolean;
	matches: UnifiedTextMatch[];
	window_note?: string | null;
}

export interface ContentSearchMatch {
	turn_id: number;
	request_id: number;
	role: 'prompt' | 'assistant';
	chunk_idx: number;
	byte_offset: number;
	match_length: number;
	line: number;
	column: number;
	before: string;
	matched: string;
	after: string;
	created_at: string;
}

export interface ContentSearchSessionGroup {
	session_id: string;
	project_path: string;
	project_display_name: string;
	title: string | null;
	match_count: number;
	turn_count: number;
	last_match_at: string;
	matches: ContentSearchMatch[];
	matches_truncated: boolean;
}

export interface ContentSearchResponse {
	query: string;
	total_matches: number;
	total_sessions: number;
	sessions: ContentSearchSessionGroup[];
	has_more: boolean;
	next_offset?: number | null;
}

export interface SearchMatchSnippet {
	request_id: number;
	line: number;
	column: number;
	before: string;
	matched: string;
	after: string;
	provenance: string;
	provenanceTitle: string;
}

export interface StreamBlock {
	seq_ord: number;
	content: string;
	byte_size: number;
	content_sha256: string;
	block_type: string;
	signature: string | null;
	wire_tool_use_id: string | null;
	arrived_at: string;
}

export interface RequestStreamBlocksPayload {
	request_id: number;
	stream_blocks: StreamBlock[];
}

export interface DeltaRowDto {
	change_kind: 'added' | 'removed';
	ord: number;
	message_id: number;
	message_sha256: string;
	role: 'user' | 'assistant' | 'system';
	byte_size: number;
	body: unknown;
	removed_reason?: string;
}

export interface RequestDeltasPayload {
	request_id: number;
	predecessor_request_id: number | null;
	deltas: DeltaRowDto[];
}

export type FilterKind = 'text_substring' | 'xml_tag' | 'lazy_section' | 'inject_reminder';

export type FilterAction = 'drop' | 'replace' | 'append';

export type RuleSource = 'toml' | 'ui';

export interface FilterRuleDto {
	id: string;
	kind: FilterKind;
	value: string;
	action: FilterAction;
	text: string | null;
	title: string | null;
	enabled: boolean;
	source: RuleSource;
	updated_at: string;
	trigger_token: string | null;
	sort_order: number;
	fires_24h: number;
	fires_7d: number;
	last_fired_at: string | null;
	/** Absent/omitted means the rule is global. See `crates/mike/src/filter/mod.rs::project_scope_matches`. */
	project_path?: string | null;
}

export interface FilterRuleCreate {
	kind: FilterKind;
	value: string;
	action: FilterAction;
	text?: string | null;
	title?: string | null;
	enabled?: boolean;
	trigger_token?: string | null;
	/** Omit for a global rule; set to scope it to one project's canonical path. */
	project_path?: string | null;
}

export type FilterRulePatch = Partial<FilterRuleCreate> & { sort_order?: number };

export interface FilterRuleMutationEnvelope {
	rule: FilterRuleDto;
	warnings: string[];
}

export interface FilterRuleDeleteResponse {
	warnings: string[];
}

export interface FilterDryRunBlock {
	type: string;
	text: string;
	cache_control?: SystemBlockCacheControl | null;
}

export interface FilterDryRunRequest {
	request_id: number;
	rules: FilterRuleDto[];
}

export interface FilterDryRunResponse {
	request_id: number;
	changed: boolean;
	project_path: string;
	/**
	 * The real rule id (parsed from the submitted `FilterRuleDto.id`) for a saved rule that
	 * fired. A submitted rule with no id, or an id that isn't a real rule id (an unsaved
	 * preview such as `candidate-0`), reports its 1-based position in `rules` negated
	 * (`-1`, `-2`, ...) instead, so it never collides with a real id.
	 */
	firing_rule_ids: number[];
	original_len: number;
	filtered_len: number;
	filtered_system: FilterDryRunBlock[] | null;
	/**
	 * The post-filter `messages` array as raw JSON (server-side an `Option<serde_json::Value>`,
	 * `crates/mike/src/api/handlers/filters.rs`) — the ordinary Anthropic Messages-API shape
	 * (`{role, content}[]`), never `FilterDryRunBlock[]`. Read it with
	 * `extractLastUserMessageText` (`lastUserMessage.ts`), which mirrors the server's own
	 * `extract_last_user_message` extraction, rather than assuming a block shape here.
	 */
	filtered_messages: unknown;
}

export interface SystemBlockCacheControl {
	type: string;
	ttl?: string;
}

export interface SystemBlock {
	index: number;
	text: string;
	cache_control: SystemBlockCacheControl | null;
	composition_match: 'matched' | 'unknown' | 'extra' | null;
}

export interface SystemBlocksProjection {
	prompt_sha256: string | null;
	composition_id: string | null;
	composition_version: number | null;
	blocks: SystemBlock[];
}

export interface PromptCompositionBlock {
	index: number;
	text: string;
	cache_control: SystemBlockCacheControl | null;
}

export type PromptCompositionMode = 'observe' | 'template' | 'replace';

export interface CompositionUsageSample {
	request_id: number;
	session_id: string;
	created_at: string;
	model: string | null;
	input_tokens: number | null;
	output_tokens: number | null;
	cache_read_tokens: number | null;
	cache_write_tokens: number | null;
}

export interface CompositionUsageStats {
	prompt_sha256: string;
	count_1h: number;
	count_24h: number;
	count_7d: number;
	count_total: number;
	total_input_tokens: number;
	total_output_tokens: number;
	recent: CompositionUsageSample[];
	agent: Agent | null;
}

export interface PromptComposition {
	id: string;
	prompt_sha256: string;
	label: string;
	version: number;
	blocks: PromptCompositionBlock[];
	mode: PromptCompositionMode;
	notes: string | null;
	agent_name: string | null;
	agent_definition_path: string | null;
	created_at: string;
	updated_at: string;
	applied_count_24h: number;
	lane?: 'main' | 'subagent';
}

export interface PromptClassDto {
	lane: 'main' | 'subagent';
	subagent_persona: string | null;
	observation_count: number;
	first_seen: string | null;
	last_seen: string | null;
	block_count: number;
	registration: PromptRegistrationDto;
	model_breakdown: PromptModelBreakdown[];
}

/** GET /api/projects — one row per captured project. The counts are present only when the list
 *  is requested with `?counts=true`. `session_count` is the project's all-time total, not a
 *  recent window; label it plainly ("N sessions"), never "recent". */
export interface ProjectSummaryDto {
	project_id: number;
	project_uuid: string;
	project_path: string;
	session_count?: number;
	request_count?: number;
	first_seen: string;
	last_seen: string;
	pinned: boolean;
	custom_name: string | null;
}

export interface TimeSpentTotalsDto {
	wait_seconds: number;
	model_seconds: number;
	answers: number;
}

export interface TimeSpentKindDto {
	kind: string;
	answers: number;
	answers_over_an_hour: number;
	wait_seconds: number;
	p50_wait_seconds: number;
	p90_wait_seconds: number;
	model_seconds: number;
	sessions: number;
}

export interface TimeSpentCommandDto {
	command: string;
	kind: string;
	calls: number;
	seconds: number;
	p50_seconds: number;
	sessions: number;
}

export interface TimeSpentBuildDto {
	running_at_once: string;
	sort_key: number;
	calls: number;
	seconds: number;
	p50_seconds: number;
}

/** GET /api/time_spent — where the model's time and the waiting on tools went. */
export interface TimeSpentDto {
	from: string;
	to: string;
	project: string | null;
	include_subagents: boolean;
	session_id: string | null;
	totals: TimeSpentTotalsDto;
	by_kind: TimeSpentKindDto[];
	slow_commands: TimeSpentCommandDto[];
	builds_running_at_once: TimeSpentBuildDto[];
	notes: string[];
}

export interface PromptModelBreakdown {
	model: string | null;
	count: number;
}

export interface PromptRegistrationDto {
	registered: boolean;
	composition_id: string | null;
	label: string | null;
	mode: PromptCompositionMode | null;
	agent_definition_path: string | null;
}

export interface PromptClassObservation {
	request_id: string;
	session_id: string;
	created_at: string;
	content_sha256: string;
	altered: boolean;
	model: string | null;
}

export interface ObservedSystemBlock {
	type: string;
	text: string;
	cache_control?: SystemBlockCacheControl | null;
}

export interface PromptClassDetailDto {
	lane: 'main' | 'subagent';
	subagent_persona: string | null;
	observation_count: number;
	first_seen: string | null;
	last_seen: string | null;
	model_breakdown: PromptModelBreakdown[];
	sample_blocks: ObservedSystemBlock[] | null;
	registration: PromptRegistrationDto;
	recent_observations: PromptClassObservation[];
	observations_total_count: number;
	observations_offset: number;
	observations_limit: number;
}

export interface PromptClassVersionModelDto {
	model: string | null;
	observation_count: number;
	first_seen: string;
	last_seen: string;
	latest_request_id: string;
	latest_session_id: string;
}

export interface PromptClassVersionDto {
	version: string | null;
	builds: number;
	observation_count: number;
	first_seen: string;
	last_seen: string;
	models: PromptClassVersionModelDto[];
}

export interface PromptClassVersionsDto {
	lane: 'main' | 'subagent';
	subagent_persona: string | null;
	versions: PromptClassVersionDto[];
}

export interface PromptCompositionListItem {
	id: string;
	prompt_sha256: string;
	label: string;
	version: number;
	block_count: number;
	total_chars: number;
	applied_count_24h: number;
	updated_at: string;
	agent_name: string | null;
}

export interface PromptCompositionCreate {
	prompt_sha256: string;
	label: string;
	blocks: PromptCompositionBlock[];
	mode?: PromptCompositionMode;
	agent_name?: string;
	agent_definition_path?: string;
}

export type PromptCompositionPatch = Partial<{
	label: string;
	blocks: PromptCompositionBlock[];
	mode: PromptCompositionMode;
	notes: string | null;
	agent_name: string | null;
	agent_definition_path: string | null;
}>;

export interface PromptCompositionMutationEnvelope {
	composition: PromptComposition;
	warnings: string[];
}

export interface CompositionRevision {
	id: string;
	composition_id: string;
	version: number;
	label: string;
	blocks: PromptCompositionBlock[];
	mode: PromptCompositionMode;
	notes: string | null;
	created_at: string;
}

export interface VolatileLinePattern {
	id: string;
	name: string;
	pattern: string;
	applies_to_block_index: 0 | 1 | 2;
	enabled: boolean;
	notes: string | null;
	created_at: string;
	updated_at: string;
}

export interface VolatileLinePatternCreate {
	name: string;
	pattern: string;
	applies_to_block_index: 0 | 1 | 2;
	enabled?: boolean;
	notes?: string | null;
}

export interface PromptSubPattern {
	name:
		| 'unregistered-class'
		| 'unregistered-subagent'
		| 'filter-induced-drop'
		| 'missing-system'
		| 'string-form-system'
		| 'non-three-block-layout'
		| 'non-text-block-at-index'
		| 'block-2-text-missing'
		| 'unknown';
	description: string;
	recommended_action:
		| 'register-class'
		| 'register-variant'
		| 'view-filter-diff'
		| 'investigate-caller'
		| 'open-sample-request';
}

export interface PromptAnomalyObservation {
	request_id: number;
	session_id: string;
}

export interface PromptAnomalyGroup {
	classification: 'unknown-class' | 'structural-mismatch';
	signature_hash: string;
	prompt_sha256: string | null;
	block_text: string | null;
	unknown_blocks: Record<string, unknown> | null;
	count: number;
	first_seen: string;
	last_seen: string;
	observations: PromptAnomalyObservation[];
	sub_pattern: PromptSubPattern;
	filter_modified: boolean;
	firing_rule_ids: string[];
	persona?: string | null;
}

export interface SteererDto {
	id: number;
	uuid: string;
	name: string;
	display_name: string;
	description: string;
	agent_id: number;
	authority: Authority;
	mode: SteeringMode;
	model: string;
	effective_model?: string;
	model_inherited?: boolean;
	priority: number;
	enabled: boolean;
	auto_enrol: boolean;
	kind?: 'refocus' | 'memory_keeper' | 'verifier' | string;
	project_id?: number | null;
	project_path?: string | null;
	prompt?: string | null;
	trigger_point: string;
	invocation: string;
	pattern: string | null;
	config: {
		confidence_threshold: number;
		debounce_turns: number;
		[key: string]: unknown;
	};
	created_at: string;
	updated_at: string;
	active_enrolment_count?: number;
}

export interface SteererEnvelope {
	steerer: SteererDto;
	warnings: string[];
}

export interface SteererCreate {
	name: string;
	display_name: string;
	description?: string;
	agent_id?: number;
	authority: Authority;
	mode: SteeringMode;
	model: string;
	prompt?: string;
	project_path?: string;
	priority?: number;
	enabled?: boolean;
	trigger_point?: string;
	invocation?: string;
	pattern?: string | null;
	config?: {
		confidence_threshold?: number;
		debounce_turns?: number;
	};
}

export interface SteererPatch {
	display_name?: string;
	description?: string;
	enabled?: boolean;
	authority?: Authority;
	mode?: SteeringMode;
	model?: string;
	prompt?: string | null;
	project_path?: string | null;
	priority?: number;
	trigger_point?: string;
	invocation?: string;
	pattern?: string | null;
	config?: {
		confidence_threshold?: number;
		debounce_turns?: number;
	};
}

export interface ModelEntry {
	id: string;
	name: string;
	prompt_price_per_1m: number | null;
	completion_price_per_1m: number | null;
	context_length: number | null;
	known_good: boolean;
	supports_structured_output: boolean;
	description?: string;
	input_modalities?: string[];
	supports_tools?: boolean;
	supports_reasoning?: boolean;
}

export interface ModelsResponse {
	models: ModelEntry[];
}

export interface AssistRequest {
	session_id: string;
	user_intent: string;
}

export interface AssistSuggestion {
	prompt: string;
	authority: Authority;
	mode: SteeringMode;
	config: {
		confidence_threshold?: number;
		debounce_turns?: number;
	};
}

export type AssistResponse =
	| {
			suggestion: AssistSuggestion;
			rationale: Record<string, string>;
			model_used: string;
	  }
	| { error: 'assist_unavailable'; message: string };

export interface EnrolSessionBody {
	steerer_id: number;
	mode: SteeringMode;
	authority: Authority;
	enrolled_by?: string;
}

export interface EnrolSessionResponse {
	enrolment_id: number;
}

export interface RevertInterventionBody {
	revert_reason: string;
}

export interface MemoryTickEvent {
	entry_count: number;
	request_body_bytes: number;
	response_body_bytes: number;
	filtered_request_body_bytes: number;
	tool_calls_bytes: number;
	tool_results_bytes: number;
	tool_results_in_body_bytes: number;
	system_prompt_blocks_bytes: number;
	entries_total_bytes: number;
	session_index_keys: number;
	composable_session_index_keys: number;
	session_meta_keys: number;
	in_flight_count: number;
	process_rss_bytes: number | null;
	fastembed_model_bytes: number;
	max_entries: number;
	max_store_bytes: number;
	over_byte_budget: boolean;
	broadcast_receivers: number;
	broadcast_backlog: number;
	db_pool_size: number | null;
	db_pool_idle: number | null;
	tick_duration_ms: number;
}

export type RowChangedTable =
	| 'requests'
	| 'sessions'
	| 'filter_rules'
	| 'prompt_compositions'
	| 'interventions'
	| 'mailbox_messages'
	| 'mailbox_threads'
	| 'lesson_injections'
	| 'steering_evaluations'
	| 'consolidation_actions'
	| 'consolidation_runs'
	| 'session_judges_verdicts'
	| 'llm_calls'
	| 'session_embeddings'
	| 'launch_launches'
	| 'launch_fleets';

export type RowChangedOp = 'INSERT' | 'UPDATE' | 'DELETE';

export type ResyncReason = 'listener_establishment' | 'recovery' | 'unusable_notification';

export type MemoryShedPath =
	'broadcast_lag' | 'response_body' | 'header_capture' | 'store_event_mpsc' | 'store_insert';

export type MemoryShedReason =
	'reject_incoming' | 'evict_existing' | 'truncate_accumulator' | 'soft_watermark';

export type MemoryShedNote =
	| 'request_body_rejected'
	| 'response_body_rejected'
	| 'store_lru_evicted_to_fit'
	| 'sse_accumulator_clipped'
	| 'broadcast_lag_dropped'
	| 'fetch_all_rows_clipped'
	| 'header_capture_clipped'
	| 'mcp_response_too_large'
	| 'store_event_backpressured';

export interface MemoryShedEvent {
	event: 'memory.shed';
	path: MemoryShedPath;
	reason: MemoryShedReason;
	bytes: number;
	limit_bytes: number;
	request_id: number | null;
	session_id: string | null;
	note: MemoryShedNote | string;
}

export type ProxyEvent =
	| { type: 'ModelConfirmed'; id: number; model: string }
	| { type: 'TextDelta'; id: number; text: string }
	| { type: 'ThinkingDelta'; id: number; thinking: string }
	| {
			type: 'ToolUseStart';
			id: number;
			tool_id: string;
			tool_name: string;
	  }
	| { type: 'ToolInputDelta'; id: number; partial_json: string }
	| { type: 'ToolUseEnd'; id: number }
	| {
			type: 'ResponseComplete';
			id: number;
			stop_reason: string | null;
			usage: TokenUsage | null;
	  }
	| { type: 'ResponseError'; id: number; error: string }
	| { type: 'UsageReceived'; id: number; usage: TokenUsage }
	| { type: 'DiagnosticReady'; id: number; count: number }
	| {
			type: 'RowChanged';
			table: RowChangedTable;
			id: string;
			session_id: string | null;
			op: RowChangedOp;
	  }
	| { type: 'ResyncRequired'; reason: ResyncReason }
	| ({ type: 'MemoryShed' } & MemoryShedEvent)
	| ({ type: 'MemoryTick' } & MemoryTickEvent)
	| {
			type: 'StreamingStarted';
			id: number;
			status: number;
			ttfb_ms: number;
			model: string | null;
	  }
	| { type: 'Lagged'; skipped: number }
	| {
			type: 'SteeringFired';
			intervention_id: number;
			session_id: string;
			session_uuid: string;
			steerer_name: string | null;
			steerer_display_name: string | null;
			authority: string | null;
			mode: string | null;
			verdict: string | null;
			confidence: number | null;
			off_thread_topic: string | null;
			payload_text: string | null;
	  };

export type ProxyEventWithId = Exclude<
	ProxyEvent,
	| { type: 'Lagged' }
	| { type: 'RowChanged' }
	| { type: 'ResyncRequired' }
	| { type: 'MemoryShed' }
	| { type: 'MemoryTick' }
	| { type: 'SteeringFired' }
>;

export type ConnectionState = 'connecting' | 'connected' | 'reconnecting' | 'disconnected';

export interface ToolUseDto {
	use_id?: number;
	request_id: number;
	session_id: string;
	created_at: string;
	tool_id: string;
	tool_name: string;
	input: unknown;
	file_path: string | null;
	ord: number;
	result?: {
		tool_use_id: string;
		request_id?: number;
		is_error: boolean;
		content: unknown;
		content_sha256: string;
		content_bytes: number;
	} | null;
}

export interface ToolUsesResponse {
	session_id: string;
	total_count: number;
	offset: number;
	limit: number;
	rows: ToolUseDto[];
}

export interface ToolResultDto {
	tool_use_id: string;
	is_error: boolean;
	content_sha256: string;
	content_bytes: number;
	content: unknown;
}

export interface RequestToolResultsResponse {
	request_id: number;
	total_count: number;
	results: ToolResultDto[];
}

export interface ToolErrorRow {
	result_id: number;
	request_id: number;
	session_id: string;
	project: string | null;
	tool_use_id: string;
	tool_name: string | null;
	created_at: string;
	content_sha256: string | null;
	content_bytes: number | null;
	content: unknown;
}

export interface ToolErrorsResponse {
	total_count: number;
	total_count_capped?: boolean;
	offset: number;
	limit: number;
	next_cursor?: string | null;
	rows: ToolErrorRow[];
}

export interface ToolErrorFacets {
	projects: string[];
	tool_names: string[];
}

export interface OperatorTurnDto {
	turn_id: number;
	session_id: string;
	project_id: number;
	start_request_id: number;
	end_request_id: number;
	start_request_number: number;
	end_request_number: number;
	user_prompt: string;
	final_assistant_text: string;
	tool_call_count: number;
	tool_error_count: number;
	files_touched: unknown;
	tools_used: unknown;
	agents_dispatched: unknown;
	weasel_findings_count: number;
	total_input_tokens: number;
	total_output_tokens: number;
	duration_ms: number;
	outcome: 'unknown' | 'accepted' | 'corrected' | 'abandoned' | 'shipped' | string;
	outcome_evidence: string | null;
	classified_at: string | null;
	created_at: string;
	updated_at: string;
	stop_response?: string | null;
	stop_reason?: string | null;
	stop_request_id?: number | null;
	interrupted?: boolean;
	prompt_kind?: string | null;
	outcome_bundle_id?: number | null;
	reaction?: string | null;
	delivery?: string | null;
	outcome_source?: 'judge' | 'operator' | 'frustration_close' | 'no_verdict' | string | null;
	counts_as_correction?: boolean;
	live_verdict_id?: number | null;
	outcome_projection_version?: number | null;
	legacy?: boolean;
	pending?: boolean;
}

export interface LessonDto {
	id: number;
	project_id: number | null;
	project_path: string | null;
	kind: string;
	text: string;
	source_turn_id: number | null;
	created_at: string;
	expires_at: string | null;
	topic: string | null;
}

export interface RecallProjectContextDto {
	project_path: string;
	recent_corrected: OperatorTurnDto[];
	recent_shipped: OperatorTurnDto[];
	lessons: LessonDto[];
	knowledge?: KnowledgeSearchResponse;
}

export interface KnowledgeSearchItem {
	reference: string;
	source_kind: 'artifact' | 'memory' | 'skill' | 'lesson' | 'note' | 'turn' | 'thread';
	title: string;
	status: string;
	path: string | null;
	updated_at: string;
	rank: number;
	lexical_rank?: number;
	semantic_rank?: number;
	provenance: Record<string, unknown>;
}

export interface KnowledgeSearchResponse {
	project_uuid?: string;
	retrieval_path: 'lexical' | 'hybrid';
	items: KnowledgeSearchItem[];
	has_more: boolean;
	next_offset: number | null;
	read_tool: string;
	embedding?: {
		coverage?: { total: number; ready: number; pending: number; failed: number };
		model?: string;
		fallback?: string | null;
	};
}

export interface KnowledgeReadResponse extends Record<string, unknown> {
	reference: string;
	body: string;
	title?: string;
	path?: string;
	status?: string;
	kind?: string;
	current?: boolean;
	content_hash?: string;
	freshness?: { state: 'fresh' | 'stale'; reasons: string[] };
}

export interface OperatorTurnOutcomeSummary {
	counts: Array<{ outcome: string; count: number }>;
	total: number;
}

export interface ToolUsesFeedRow {
	use_id: number;
	wire_tool_use_id: string;
	request_id: number;
	session_id: string;
	project: string | null;
	tool_name: string;
	created_at: string;
	ord: number;
	input: unknown;
	file_path: string | null;
	is_error?: boolean;
	result_id?: number | null;
	error_content?: unknown;
	error_content_bytes?: number | null;
	error_content_sha256?: string | null;
}

export interface ToolUsesFeedResponse {
	total_count: number;
	total_count_capped?: boolean;
	offset: number;
	limit: number;
	next_cursor?: string | null;
	rows: ToolUsesFeedRow[];
}

export interface ToolUseFacets {
	projects: string[];
	tool_names: string[];
}

export interface AgentDispatchDto {
	parent_request_id: number;
	session_id: string;
	project_id: number;
	created_at: string;
	tool_id: string;
	subagent_type: string | null;
	description: string | null;
	brief_char_len: number;
	brief_sha256: string | null;
	ord: number;
}

export interface WorkflowAgentDto {
	agent_id: string;
	agent_type: string | null;
	status: 'started' | 'result';
	summary: string | null;
	gaps_count: number;
	result_json: unknown | null;
	request_ids: number[];
}

export interface WorkflowRunDto {
	run_id: number;
	run_uid: string | null;
	name: string | null;
	description: string | null;
	phases: string[];
	parent_request_id: number;
	agent_count: number;
	result_count: number;
	agents: WorkflowAgentDto[];
}

/** POST /api/steering/evaluations/{uuid}/label (api/steer_questions/actions.rs). */
export interface SteerEvaluationLabelDto {
	label_id: number;
	cancelled_wakes: number[];
}

export interface ApprovedLessonDto {
	lesson_id: number;
	project_id: number;
}

/** POST /api/directive-proposals/{id}/approve (api/cross_session_approve.rs). */
export interface DirectiveProposalApprovedDto {
	id: number;
	status: string;
	lessons: ApprovedLessonDto[];
}

/** POST /api/directive-proposals/{id}/reject (api/cross_session.rs). */
export interface DirectiveProposalRejectedDto {
	id: number;
	status: string;
}

/** POST /api/semantic/patterns/candidates/{id}/name (api/semantic/patterns.rs). */
export interface PatternCandidateNamedDto {
	candidate_id: number;
	status: string;
	sort_order: number;
	anchor_ids: number[];
}
