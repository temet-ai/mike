import { ApiError } from '$lib/api';
import { cachedGet, invalidateApiCache } from '$lib/stores/cache/apiCache.svelte';
import type { LessonDto } from '$lib/types';

export type RunKind = 'session' | 'nightly' | 'gc';
export type RunTrigger = 'schedule' | 'startup_catchup' | 'manual';
export type RunStatus = 'running' | 'completed' | 'partial' | 'failed';
export type ActionKind =
	| 'promoted_to_daily'
	| 'promoted_to_long_term'
	| 'archived_duplicate'
	| 'archived_trimmed'
	| 'archived_expired'
	| 'deleted_expired'
	| 'evaluated';
export type ActionOutcome = 'applied' | 'failed' | 'skipped';
export type DecidedBy = 'embedding_distance' | 'rank_order' | 'expiry';
export type InjectionLane = 'primary' | 'subagent';

export interface ConsolidationRun {
	id: number;
	uuid: string;
	kind: RunKind;
	trigger: RunTrigger;
	status: RunStatus;
	started_at: string;
	finished_at: string | null;
	error: string | null;
	action_count: number;
}

export interface RunnerUp {
	lesson_id: number;
	distance: number;
}

export interface ConsolidationAction {
	id: number;
	run_id: number;
	run_kind: RunKind;

	lesson_id: number | null;
	lesson_uuid: string;
	project_id: number | null;
	project_path: string | null;
	session_id: number | null;
	session_uuid: string | null;
	source_turn_id: number | null;
	files_touched: string[] | null;
	agent_id: number | null;

	lesson_text: string;

	action_kind: ActionKind;
	outcome: ActionOutcome;
	error: string | null;

	before_stage: string | null;
	after_stage: string | null;
	decided_by: DecidedBy | null;

	distance: number | null;
	threshold: number | null;
	embedding_model: string | null;

	counterpart_lesson_id: number | null;
	counterpart_uuid: string | null;
	counterpart_text: string | null;

	runners_up: RunnerUp[];
	rank_detail: Record<string, unknown> | null;

	injection_count: number | null;

	created_at: string;
}

export interface LessonInjection {
	id: number;
	lesson_id: number;
	request_id: number;
	session_id: number | null;
	session_uuid: string | null;
	injected_at: string;
	lane: InjectionLane;
	persona: string | null;
}

export interface LessonHistory {
	lesson_id: number;
	lesson_uuid: string;
	lesson_text: string;
	kind: string;
	stage: string;
	created_at: string;
	session_uuid: string | null;
	source_turn_id: number | null;
	actions: ConsolidationAction[];
	injections: LessonInjection[];
}

export interface Paged<T> {
	items: T[];
	total_count: number;
	limit: number;
	offset: number;
}

export interface ConsolidationRunsFilters {
	kind?: RunKind;
	status?: RunStatus;
	project?: string;
	session?: string;
	file?: string;
	limit?: number;
	offset?: number;
	[key: string]: string | number | undefined;
}

export interface MemoryDecisionsFilters {
	project?: string;
	session?: string;
	file?: string;
	action_kind?: ActionKind;
	limit?: number;
	offset?: number;
	[key: string]: string | number | undefined;
}

function toQuery(params: Record<string, string | number | undefined>): string {
	const search = new URLSearchParams();
	for (const [key, value] of Object.entries(params)) {
		if (value === undefined || value === '') continue;
		search.set(key, String(value));
	}
	const qs = search.toString();
	return qs ? `?${qs}` : '';
}

export async function getConsolidationRuns(
	filters: ConsolidationRunsFilters = {}
): Promise<Paged<ConsolidationRun>> {
	const path = `/api/memory/consolidation/runs${toQuery(filters)}`;
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as Paged<ConsolidationRun>;
	});
}

export async function getConsolidationRun(
	id: number
): Promise<{ run: ConsolidationRun; actions: ConsolidationAction[] }> {
	const path = `/api/memory/consolidation/runs/${id}`;
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as { run: ConsolidationRun; actions: ConsolidationAction[] };
	});
}

export async function getMemoryDecisions(
	filters: MemoryDecisionsFilters = {}
): Promise<Paged<ConsolidationAction>> {
	const path = `/api/memory/decisions${toQuery(filters)}`;
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as Paged<ConsolidationAction>;
	});
}

export async function getLessonHistory(id: number): Promise<LessonHistory> {
	const path = `/api/lessons/${id}/history`;
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as LessonHistory;
	});
}

export async function getSessionMemoryActions(sessionId: string): Promise<ConsolidationAction[]> {
	const path = `/api/sessions/${encodeURIComponent(sessionId)}/memory-actions`;
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as ConsolidationAction[];
	});
}

export type ArchivedReason = 'duplicate' | 'expired' | 'trimmed' | 'superseded' | 'operator';

export interface ArchivedLesson {
	id: number;
	uuid: string;
	project_id: number | null;
	project_path: string | null;
	kind: string;
	text: string;
	topic: string | null;
	session_id: number | null;
	session_uuid: string | null;
	source_turn_id: number | null;
	agent_id: number | null;
	created_at: string;
	archived_at: string | null;
	archived_reason: ArchivedReason | string | null;
	duplicate_of_id: number | null;
}

export async function getArchivedLessons(projectPath: string): Promise<ArchivedLesson[]> {
	const path = `/api/lessons/archived${toQuery({ project: projectPath })}`;
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as ArchivedLesson[];
	});
}

export async function restoreLesson(id: number): Promise<LessonDto> {
	const path = `/api/lessons/${id}/restore`;
	const res = await fetch(path, {
		method: 'POST',
		headers: { accept: 'application/json' }
	});
	if (!res.ok) {
		let message = `POST ${path} failed: ${res.status} ${res.statusText}`;
		try {
			const errBody = (await res.json()) as { error?: string };
			if (errBody && typeof errBody.error === 'string' && errBody.error.length > 0) {
				message = errBody.error;
			}
		} catch {}
		throw new ApiError(message, res.status, path);
	}
	void invalidateApiCache('/api/memory/decisions');
	void invalidateApiCache('/api/memory/consolidation/runs');
	void invalidateApiCache('/api/lessons');
	return (await res.json()) as LessonDto;
}

export interface ActionKindDescription {
	label: string;
	tone: 'active' | 'warning' | 'error' | 'muted';
}

const ACTION_KIND_DESCRIPTIONS: Record<ActionKind, ActionKindDescription> = {
	promoted_to_daily: { label: 'Promoted to daily', tone: 'active' },
	promoted_to_long_term: { label: 'Promoted to long-term', tone: 'active' },
	archived_duplicate: { label: 'Archived — merged as a duplicate', tone: 'warning' },
	archived_trimmed: { label: 'Archived — trimmed for capacity', tone: 'warning' },
	archived_expired: { label: 'Archived — expired', tone: 'muted' },
	deleted_expired: { label: 'Deleted — expired', tone: 'error' },
	evaluated: { label: 'Evaluated — no action taken', tone: 'muted' }
};

export function describeActionKind(kind: ActionKind): ActionKindDescription {
	return ACTION_KIND_DESCRIPTIONS[kind] ?? { label: kind, tone: 'muted' };
}

const OUTCOME_TONES: Record<ActionOutcome, ActionKindDescription['tone']> = {
	applied: 'active',
	failed: 'error',
	skipped: 'warning'
};

export function outcomeTone(outcome: ActionOutcome): ActionKindDescription['tone'] {
	return OUTCOME_TONES[outcome] ?? 'muted';
}

export function isRestorableAction(
	action: Pick<ConsolidationAction, 'action_kind' | 'lesson_id'>
): boolean {
	return action.action_kind.startsWith('archived_') && action.lesson_id !== null;
}

const ARCHIVED_REASON_DESCRIPTIONS: Record<ArchivedReason, ActionKindDescription> = {
	duplicate: { label: 'Archived — merged as a duplicate', tone: 'warning' },
	trimmed: { label: 'Archived — trimmed for capacity', tone: 'warning' },
	expired: { label: 'Archived — expired', tone: 'muted' },
	superseded: { label: 'Archived — superseded', tone: 'muted' },
	operator: { label: 'Archived — by operator', tone: 'muted' }
};

export function describeArchivedReason(reason: string | null): ActionKindDescription {
	if (reason === null) return { label: 'Archived', tone: 'muted' };
	return (
		ARCHIVED_REASON_DESCRIPTIONS[reason as ArchivedReason] ?? {
			label: `Archived — ${reason}`,
			tone: 'muted'
		}
	);
}
