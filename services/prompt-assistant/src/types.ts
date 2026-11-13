// Shared types for the prompt-assistant service. Kept dependency-free so both
// the HTTP layer and the MCP tool layer can import from here without cycles.

export type MayApply = 'suggest' | 'apply';
export type Effort = 'low' | 'medium' | 'high';
export type AssistantProvider = 'anthropic' | 'codex';

export interface PromptAssistantSettings {
	provider: AssistantProvider;
	model: string;
	codex_model: string;
	effort: Effort;
	instructions: string;
	read_prompt: boolean;
	read_edits: boolean;
	read_project_instructions: boolean;
	read_versions: boolean;
	read_memory: boolean;
	/** Gates the `list_projects` and `compare_across_projects` tools. */
	read_other_projects: boolean;
	may_apply: MayApply;
	capture: boolean;
	max_turns: number;
	daily_budget_usd: number;
	keep_days: number;
	/** The `prompt_assistant` feature toggle — off means the whole service refuses new turns. */
	enabled: boolean;
}

export type ProposalAction = 'replace' | 'append' | 'drop';

/** What the assistant checks a message against: the request's own project, or every known project. */
export type AssistantScope = 'project' | 'all';

export interface Proposal {
	id: string;
	title: string;
	edit_id: string | null;
	action: ProposalAction;
	match: string;
	/**
	 * The raw text to save on the filter rule — exactly `args.text` from
	 * `propose_edit` for `replace`/`append`, `null` for `drop`. This is what a
	 * caller should PATCH/POST as the rule's `text`; it is never the
	 * match-concatenated `after` used for display.
	 */
	rule_text: string | null;
	before: string;
	after: string;
	reach: string;
	/** When set, the proposal is scoped to this project's canonical path rather than global. */
	project_path: string | null;
	created_at: string;
	applied: boolean;
	applied_rule_id?: string;
}

export interface ConversationMessage {
	id: string;
	role: 'user' | 'assistant';
	text: string;
	quote?: string;
	proposals?: Proposal[];
	cost_usd?: number;
	created_at: string;
}

export interface Conversation {
	id: string;
	request_id: number;
	created_at: string;
	updated_at: string;
	messages: ConversationMessage[];
}

export interface SseTextEvent {
	event: 'text';
	data: { delta: string };
}
export interface SseProposalEvent {
	event: 'proposal';
	data: {
		id: string;
		title: string;
		edit_id: string | null;
		action: ProposalAction;
		match: string;
		/** Raw rule text, exactly as it should be saved. `null` for `drop`. See `Proposal.rule_text`. */
		rule_text: string | null;
		before: string;
		after: string;
		reach: string;
		project_path: string | null;
	};
}
export interface SseStatusEvent {
	event: 'status';
	data: { phase: string };
}
export interface SseErrorEvent {
	event: 'error';
	data: { message: string };
}
export interface SseDoneEvent {
	event: 'done';
	data: { cost_usd: number };
}
export interface SseAppliedEvent {
	event: 'applied';
	data: { proposal_id: string; rule_id: string };
}

export type SseEvent =
	| SseTextEvent
	| SseProposalEvent
	| SseStatusEvent
	| SseErrorEvent
	| SseDoneEvent
	| SseAppliedEvent;

export interface FilterRuleDto {
	id: string;
	kind: 'text_substring' | 'xml_tag' | 'lazy_section' | 'inject_reminder';
	value: string;
	action: 'drop' | 'replace' | 'append';
	text: string | null;
	enabled: boolean;
	source: 'toml' | 'ui';
	updated_at: string;
	trigger_token: string | null;
	sort_order: number;
	fires_24h: number;
	fires_7d: number;
	last_fired_at: string | null;
	title?: string | null;
	/** Absent/omitted means the rule is global. See `crates/mike-features/src/filter/mod.rs::project_scope_matches`. */
	project_path?: string | null;
}

/** One project mike has captured traffic for, as `GET /api/projects` reports it. */
export interface ProjectSummary {
	project_id: number;
	project_uuid: string;
	project_path: string;
	custom_name: string | null;
	session_count: number;
	request_count: number;
	first_seen: string;
	last_seen: string;
}

/** One matching turn found by `GET /api/content/search` for a candidate edit's match text. */
export interface ContentSearchExcerpt {
	request_id: number;
	session_id: string;
	before: string;
	matched: string;
	after: string;
}

/** `compare_across_projects`' per-project verdict material — the model, not this service, judges agreement/conflict. */
export interface CrossProjectFinding {
	project_path: string;
	project_name: string | null;
	last_seen: string;
	match_found: boolean;
	match_excerpts: ContentSearchExcerpt[];
	instructions: ProjectFileSummary[];
}

/** Mirrors `mcp/projectFiles.ts`'s `ProjectFile`, restated here so `types.ts` stays dependency-free. */
export interface ProjectFileSummary {
	path: string;
	content: string;
	truncated: boolean;
}

export interface SystemBlock {
	index: number;
	text: string;
	cache_control: { type: string; ttl?: string } | null;
	composition_match: 'matched' | 'unknown' | 'extra' | null;
}

export interface SystemBlocksProjection {
	prompt_sha256: string | null;
	composition_id: string | null;
	composition_version: number | null;
	blocks: SystemBlock[];
}

export interface RequestDetail {
	id: number;
	session_id: string;
	project: string | null;
	human_prompt: string;
	last_user_message: string;
	model: string | null;
	system_blocks?: SystemBlocksProjection;
}

export interface FilterDryRunResponse {
	request_id: number;
	changed: boolean;
	project_path: string;
	firing_rule_ids: number[];
	original_len: number;
	filtered_len: number;
	filtered_system: Array<{ type: string; text: string }> | null;
	filtered_messages: Array<{ type: string; text: string }> | null;
}

export interface PromptClassDto {
	lane: 'main' | 'subagent';
	subagent_persona: string | null;
	observation_count: number;
	first_seen: string | null;
	last_seen: string | null;
	block_count: number;
	model_breakdown: Array<{ model: string | null; count: number }>;
}
