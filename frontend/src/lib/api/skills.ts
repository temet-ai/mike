import { ApiError } from '$lib/api';

/**
 * Client for the Skills page: the skills mike writes into a project's `.claude/skills/` from its
 * accepted lessons, the reviewer's evaluation of each draft, and the user's decisions on them.
 * Wire shapes follow crates/mike-features/src/api/skills/*.rs.
 */

export type SkillMode = 'auto' | 'ask';

export type SkillState =
	| 'retired_by_you'
	| 'shipped'
	| 'retired_unloaded'
	| 'retired_corrected'
	| 'removed'
	| 'rejected_by_you'
	| 'waiting_on_you'
	| 'approved_not_yet_shipped'
	| 'ready_to_ship'
	| 'failed_review'
	| 'being_reviewed'
	| 'waiting_for_reviewer'
	| 'generated_only';

export type DraftStatus = 'draft' | 'evaluating' | 'accepted' | 'rejected';
export type DraftReview = 'approved' | 'rejected';

export interface SkillProject {
	project_uuid: string;
	project_path: string;
	display_name: string;
	mode: SkillMode;
	mode_set: boolean;
	folders: number;
	shipped: number;
	/** The most skills mike writes into one project (setting lessons.skills.max_per_project). */
	skill_cap: number;
	waiting: number;
}

export interface SkillProjectsResponse {
	projects: SkillProject[];
	waiting: number;
}

export interface SkillInstall {
	machine_name: string;
	path: string;
	draft_id: number | null;
	first_written: string;
	retired_at: string | null;
	retired_reason: 'unloaded' | 'corrected_above_base' | 'user' | 'policy' | null;
	reported_at: string;
}

export type ReviewDecision = 'approve' | 'patch' | 'nothing_to_save' | 'wrong_place' | 'unusable';
export type ReviewPlace = 'rule' | 'existing_skill' | 'agent_persona' | 'memory_file' | 'nowhere';

/** The reviewer model's answer on a draft. */
export interface ReviewerVerdict {
	decision: ReviewDecision;
	reason: string;
	overlap: string | null;
	belongs_in: ReviewPlace | null;
}

/** How a shipped skill has done in real use, measured by the promoter on each run. */
export interface SkillLiveUse {
	first_shipped_on: string;
	window_start: string;
	window_end: string;
	loads: number;
	load_sessions: number;
	loaded_turns: number;
	judged_turns: number;
	corrected_turns: number;
	base_judged_turns: number;
	base_corrected_turns: number;
	retired_at: string | null;
	retired_reason: 'unloaded' | 'corrected_above_base' | null;
	measured_at: string;
}

export interface SkillFolder {
	folder: string;
	draft_id: number | null;
	status: DraftStatus | null;
	review: DraftReview | null;
	revision: number | null;
	lesson_watermark: number | null;
	lesson_count: number | null;
	description: string | null;
	drafted_at: string | null;
	evaluated_at: string | null;
	edited_at: string | null;
	trigger_score: number | null;
	patched_trigger_score: number | null;
	reviewer: ReviewerVerdict | null;
	verdict_reasons: string[] | null;
	approved_draft_id: number | null;
	first_written: string | null;
	live: boolean;
	installs: SkillInstall[];
	retired_by_user_at: string | null;
	live_use: SkillLiveUse | null;
	state: SkillState;
	needs_you: boolean;
}

export interface SkillFoldersResponse {
	project: string;
	mode: SkillMode;
	mode_set: boolean;
	skills: SkillFolder[];
}

export interface ReferenceFile {
	name: string;
	when_to_read?: string;
	content: string;
}

export interface TriggerResult {
	query: string;
	should_trigger: boolean;
	triggered: boolean;
	correct: boolean;
	loaded: string[];
}

export interface TriggerTest {
	score: number;
	correct: number;
	total: number;
	results: TriggerResult[];
}

export interface SkillEval {
	evaluator_version?: number;
	models?: string[];
	calls?: number;
	verdict?: {
		accepted: boolean;
		reasons: string[];
		min_trigger_score: number;
	};
	trigger?: TriggerTest;
	review?: ReviewerVerdict & {
		original_description?: string | null;
		original_skill_md?: string | null;
	};
	recheck?: TriggerTest | null;
	error?: string;
}

export interface SkillDraft {
	id: number;
	project_id: number;
	project_uuid: string;
	project_path: string;
	folder: string;
	agent_name: string | null;
	status: DraftStatus;
	review: DraftReview | null;
	reviewed_at: string | null;
	edited_at: string | null;
	revision: number;
	lesson_watermark: number;
	lesson_ids: number[];
	model: string;
	description: string;
	skill_md: string;
	authored_skill_md: string | null;
	reference_files: ReferenceFile[];
	trigger_queries: { query: string; should_trigger: boolean }[];
	eval: SkillEval | null;
	created_at: string;
	evaluated_at: string | null;
}

export interface SkillLesson {
	id: number;
	kind: string;
	stage: string;
	text: string;
	topic: string | null;
	archived_at: string | null;
	user_prompt: string | null;
}

export interface SkillRevision {
	id: number;
	revision: number;
	lesson_watermark: number;
	status: DraftStatus;
	review: DraftReview | null;
	created_at: string;
}

export interface SkillDraftResponse {
	draft: SkillDraft;
	lessons: SkillLesson[];
	revisions: SkillRevision[];
	folder: SkillFolder | null;
	mode: SkillMode;
}

async function send<T>(method: string, path: string, body?: unknown): Promise<T> {
	const res = await fetch(path, {
		method,
		headers: { accept: 'application/json', 'content-type': 'application/json' },
		body: body === undefined ? undefined : JSON.stringify(body)
	});
	if (!res.ok) {
		let message = `${method} ${path} failed: ${res.status} ${res.statusText}`;
		try {
			const parsed = (await res.json()) as { error?: string };
			if (parsed.error) message = parsed.error;
		} catch {
			message = `${method} ${path} failed: ${res.status}`;
		}
		throw new ApiError(message, res.status, path);
	}
	return (await res.json()) as T;
}

export function getSkillProjects(): Promise<SkillProjectsResponse> {
	return send('GET', '/api/skills/projects');
}

export function getProjectSkills(projectUuid: string): Promise<SkillFoldersResponse> {
	return send('GET', `/api/skills?project=${encodeURIComponent(projectUuid)}`);
}

export function getSkillDraft(id: number): Promise<SkillDraftResponse> {
	return send('GET', `/api/skills/drafts/${id}`);
}

export function reviewSkillDraft(
	id: number,
	decision: 'approve' | 'reject' | 'clear'
): Promise<unknown> {
	return send('POST', `/api/skills/drafts/${id}/review`, { decision });
}

export function editSkillDraft(id: number, skillMd: string): Promise<unknown> {
	return send('PATCH', `/api/skills/drafts/${id}/content`, { skill_md: skillMd });
}

export function setSkillMode(projectUuid: string, mode: SkillMode): Promise<unknown> {
	return send('POST', '/api/skills/policy', { project: projectUuid, mode });
}

export function retireSkill(projectUuid: string, folder: string): Promise<unknown> {
	return send('POST', '/api/skills/retire', { project: projectUuid, folder });
}

export function restoreSkill(projectUuid: string, folder: string): Promise<unknown> {
	return send('POST', '/api/skills/restore', { project: projectUuid, folder });
}

/** The words a user sees for each state, and the badge that carries it. */
export const STATE_TEXT: Record<SkillState, string> = {
	waiting_on_you: 'Waiting on you',
	shipped: 'Shipped',
	ready_to_ship: 'Ships on the next run',
	approved_not_yet_shipped: 'Approved, ships on the next run',
	being_reviewed: 'Being reviewed',
	waiting_for_reviewer: 'Waiting for the reviewer',
	failed_review: 'Did not pass review',
	rejected_by_you: 'Rejected by you',
	retired_by_you: 'Retired by you',
	retired_unloaded: 'Retired, not loaded for 30 days',
	retired_corrected: 'Retired, did not help',
	removed: 'Removed',
	generated_only: 'Written from the lessons, not drafted'
};

export function percent(rate: number | null | undefined): string {
	return rate === null || rate === undefined ? '—' : `${Math.round(rate * 100)}%`;
}
