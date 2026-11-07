import { ApiError } from '$lib/api';
import { cachedGet, invalidateApiCache } from '$lib/stores/cache/apiCache.svelte';
import type { Paged } from './memoryDecisions';

export type RelationKind = 'supersedes' | 'refines' | 'recurs' | 'contradicts';
export type RelationMethod =
	'declared' | 'artifact_reference' | 'adjudicated' | 'operator' | 'write_time' | 'rewritten';
export type RelationStatus = 'proposed' | 'confirmed' | 'rejected' | 'contested';

export interface LessonRelation {
	id: number;
	uuid: string;
	from_lesson_id: number;
	from_lesson_text: string;
	from_lesson_created_at: string;
	to_lesson_id: number;
	to_lesson_text: string;
	to_lesson_created_at: string;
	kind: RelationKind;
	method: RelationMethod;
	status: RelationStatus;
	candidate_distance: number | null;
	evidence_json: Record<string, unknown>;
	proposer_model: string | null;
	proposer_rationale: string | null;
	adjudicator_model: string | null;
	adjudicator_agrees: boolean | null;
	adjudicator_rationale: string | null;
	detector_version: string;
	proposed_at: string;
	decided_at: string | null;
	decided_by: string | null;
	created_at: string;
}

export interface LessonRelationsFilters {
	status?: RelationStatus;
	kind?: RelationKind;
	limit?: number;
	offset?: number;
	[key: string]: string | number | undefined;
}

export type InjectionOutcomeCode = 'unknown' | 'success' | 'partial' | 'failure';

export interface LessonOutcomeBucket {
	outcome: InjectionOutcomeCode;
	count: number;
	avg_quality_score: number | null;
}

export interface LessonOutcomeSummary {
	lesson_id: number;
	kind: string;
	lesson_text?: string;
	injection_count: number;
	distinct_session_count: number;
	outcomes_observed_after_injection: LessonOutcomeBucket[];
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

export async function getLessonRelations(
	filters: LessonRelationsFilters = {}
): Promise<Paged<LessonRelation>> {
	const path = `/api/memory/relations${toQuery(filters)}`;
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as Paged<LessonRelation>;
	});
}

export async function getLessonOutcomes(): Promise<LessonOutcomeSummary[]> {
	const path = '/api/memory/lesson-outcomes';
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as LessonOutcomeSummary[];
	});
}

async function postRelationDecision(
	id: number,
	action: 'confirm' | 'reject'
): Promise<LessonRelation> {
	const path = `/api/memory/relations/${id}/${action}`;
	const res = await fetch(path, { method: 'POST', headers: { accept: 'application/json' } });
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
	void invalidateApiCache('/api/memory/relations');
	return (await res.json()) as LessonRelation;
}

export async function confirmLessonRelation(id: number): Promise<LessonRelation> {
	return postRelationDecision(id, 'confirm');
}

export async function rejectLessonRelation(id: number): Promise<LessonRelation> {
	return postRelationDecision(id, 'reject');
}

const RELATION_KIND_LABELS: Record<RelationKind, string> = {
	supersedes: 'Supersedes',
	refines: 'Refines',
	recurs: 'Recurs',
	contradicts: 'Contradicts'
};

export function describeRelationKind(kind: RelationKind): string {
	return RELATION_KIND_LABELS[kind] ?? kind;
}

const RELATION_METHOD_LABELS: Record<RelationMethod, string> = {
	declared: 'Declared by the model',
	artifact_reference: 'Artifact reference',
	adjudicated: 'Adjudicated',
	operator: 'Operator-authored',
	write_time: 'Decided when the lesson was written',
	rewritten: 'Rewritten into a rule'
};

export function describeRelationMethod(method: RelationMethod): string {
	return RELATION_METHOD_LABELS[method] ?? method;
}

export function relationStatusTone(
	status: RelationStatus
): 'active' | 'warning' | 'error' | 'muted' {
	switch (status) {
		case 'confirmed':
			return 'active';
		case 'rejected':
			return 'error';
		case 'contested':
			return 'warning';
		case 'proposed':
		default:
			return 'muted';
	}
}

export function isDecidableRelation(status: RelationStatus): boolean {
	return status === 'proposed' || status === 'contested';
}

export function outcomeBucketTone(outcome: string): 'active' | 'warning' | 'error' | 'muted' {
	switch (outcome) {
		case 'success':
			return 'active';
		case 'partial':
			return 'warning';
		case 'failure':
			return 'error';
		case 'unknown':
		default:
			return 'muted';
	}
}
