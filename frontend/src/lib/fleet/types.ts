export type FleetNodeState = 'working' | 'idle' | 'stale' | 'done' | 'error' | 'unknown';

export type FleetErrorKind = 'agent' | 'infra' | 'ended_early';

export interface FleetNodeQuality {
	outcome: string;
	rationale: string;
}

export interface FleetGraphNode {
	session_id: string;
	title: string | null;
	agent_type: string | null;
	role: string | null;
	model: string | null;
	workspace: string | null;
	cwd: string | null;
	worktree: string | null;
	goal: string | null;
	next_steps: string | null;
	state: FleetNodeState;
	state_detail?: string | null;
	error_kind?: FleetErrorKind | null;
	quality?: FleetNodeQuality | null;
	unconsumed_mail?: number;
	last_activity_at: string;
	request_count: number;
}

export type FleetGraphEdgeKind = 'messaged';

export interface FleetGraphEdge {
	from_session_id: string;
	to_session_id: string;
	kind: FleetGraphEdgeKind;
	first_ts: string;
	last_ts: string;
	count: number;
}

export interface FleetGraphResponse {
	project_path: string;
	generated_at: string;
	nodes: FleetGraphNode[];
	edges: FleetGraphEdge[];
}

export interface FleetProjectSummary {
	project_path: string;
	session_count?: number;
	request_count?: number;
	first_seen: string;
	last_seen: string;
	pinned: boolean;
	custom_name: string | null;
	project_id?: number | null;
	project_uuid?: string;
}

export interface FleetProjectWithCounts extends FleetProjectSummary {
	session_count: number;
	request_count: number;
}

export interface FleetErrorBody {
	error: string;
	message: string;
}

export interface FleetAcceptanceObligation {
	note_id: string;
	obligation_id: string;
	title: string;
	body: string;
	data: unknown;
	tasks: string[];
	criteria: string[];
	outcome: string;
	required: boolean;
	issue: string | null;
	at: string;
	updated_at?: string;
	session_uuid: string;
}

export interface FleetTaskStatus {
	id: string;
	name: string;
	state: string;
	criteria: string[];
	passed_criteria: string[];
	ready?: boolean;
}

export interface FleetCriterionStatus {
	id: string;
	text: string;
	tasks: number;
	passed: number;
	ready?: number;
}

export interface EffortCell {
	requests: number;
	tokens: number;
	share_percent: number | null;
}

export interface EffortCells {
	tokens: number;
	labels: Record<string, EffortCell>;
}

export interface FleetEffortSplit {
	basis?: string;
	all_seats?: EffortCells;
	by_seat?: Record<string, EffortCells>;
	error?: string;
}

export interface FleetMailWakeCounts {
	basis?: string;
	by_seat?: Record<string, Record<string, number>>;
}

export interface FleetRecord {
	live?: boolean;
	note?: string;
	effort_split?: FleetEffortSplit;
	mail_wake_counts?: FleetMailWakeCounts;
}

export type FleetDecisionMode = 'off' | 'shadow' | 'on';

export interface FleetDecisionModes {
	stop_check: FleetDecisionMode;
	wake_check: FleetDecisionMode;
	triage: FleetDecisionMode;
	commit_message: FleetDecisionMode;
	effort: FleetDecisionMode;
}

export interface FleetAcceptanceStatus {
	fleet: string;
	slug: string;
	state: string;
	run_id: string | null;
	goal: string;
	now: string;
	accepted?: boolean;
	tasks: FleetTaskStatus[];
	criteria: FleetCriterionStatus[];
	obligations?: FleetAcceptanceObligation[];
	blocked: { seat: string; title: string }[];
	needs_operator: { at: string; seat: string; title: string }[];
}
