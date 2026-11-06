import { ApiError } from '$lib/api';

export type LabelAxis = 'reaction' | 'delivery' | 'session_end';

export interface VocabularyLabel {
	label: string;
	sort_order: number;
	description: string;
}

export interface VocabularyAxis {
	axis: string;
	subject_kinds: string[];
	labels: VocabularyLabel[];
}

export interface EvalLabelDto {
	id: number;
	subject_kind: string;
	subject_id: number;
	operator_turn_id: number;
	session_uuid: string;
	axis: string;
	label: string;
	action: 'assert' | 'withdraw';
	tier: 'gold' | 'relayed' | 'auto_shadow' | 'seed';
	method: string;
	split: 'dev' | 'holdout';
	tags: string[];
	severity: string | null;
	confidence: string | null;
	evidence: string | null;
	note: string | null;
	source: string;
	provenance: Record<string, unknown>;
	verdict_snapshot: Record<string, unknown> | null;
	supersedes_id: number | null;
	created_at: string;
	split_reason?: string | null;
	is_current?: boolean;
	subject_present?: boolean;
}

export interface LabelListResponse {
	labels: EvalLabelDto[];
	next_before_id: number | null;
	verdict_now: Record<string, unknown> | null;
}

export type LabelSubject = { kind: 'turn'; turnId: number } | { kind: 'session_end'; sessionId: string };

export class LabelWriteError extends Error {
	constructor(
		message: string,
		public readonly status: number
	) {
		super(message);
	}
}

async function readJson<T>(res: Response, path: string): Promise<T> {
	if (!res.ok) {
		let message = `${res.status} ${res.statusText}`;
		try {
			const body = (await res.json()) as { error?: string };
			if (body.error) message = body.error;
		} catch {
			message = `${res.status} ${res.statusText}`;
		}
		if (res.status >= 400 && res.status < 500) throw new LabelWriteError(message, res.status);
		throw new ApiError(`${path}: ${message}`, res.status, path);
	}
	return (await res.json()) as T;
}

export async function getEvalVocabulary(): Promise<VocabularyAxis[]> {
	const path = '/api/eval/vocabulary';
	const res = await fetch(path, { headers: { accept: 'application/json' } });
	return (await readJson<{ axes: VocabularyAxis[] }>(res, path)).axes;
}

export function labelsPath(subject: LabelSubject): string {
	return subject.kind === 'turn'
		? `/api/eval/labels?subject_kind=turn&turn_id=${subject.turnId}`
		: `/api/eval/labels?subject_kind=session_end&session=${encodeURIComponent(subject.sessionId)}`;
}

export async function listEvalLabels(subject: LabelSubject): Promise<LabelListResponse> {
	const path = labelsPath(subject);
	const res = await fetch(path, { headers: { accept: 'application/json' } });
	return readJson<LabelListResponse>(res, path);
}

function subjectBody(subject: LabelSubject) {
	return subject.kind === 'turn'
		? { kind: 'turn', turn_id: subject.turnId }
		: { kind: 'session_end', session_id: subject.sessionId };
}

async function post<T>(path: string, body: unknown): Promise<T> {
	const res = await fetch(path, {
		method: 'POST',
		headers: { 'content-type': 'application/json', accept: 'application/json' },
		body: JSON.stringify(body)
	});
	return readJson<T>(res, path);
}

export function postGoldLabels(
	subject: LabelSubject,
	labels: Array<{ axis: string; label: string }>,
	note: string | null,
	context: string,
	evalRunId?: number
): Promise<{ labels: EvalLabelDto[] }> {
	const body: Record<string, unknown> = { subject: subjectBody(subject), labels, note, context };
	if (evalRunId !== undefined) body['eval_run_id'] = evalRunId;
	return post('/api/eval/labels', body);
}

export function withdrawLabel(id: number): Promise<{ label: EvalLabelDto }> {
	return post(`/api/eval/labels/${id}/withdraw`, {});
}

export function confirmRelayedLabel(id: number): Promise<{ label: EvalLabelDto }> {
	return post(`/api/eval/labels/${id}/confirm`, {});
}
