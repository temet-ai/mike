import { ApiError } from '$lib/api';
import { cachedGet, invalidateApiCache } from '$lib/stores/cache/apiCache.svelte';
import type { Authority, SteeringMode, Verdict } from '$lib/steering';
import type {
	SteererDto,
	SteererEnvelope,
	SteererCreate,
	SteererPatch,
	EnrolSessionBody,
	EnrolSessionResponse,
	RevertInterventionBody,
	ModelsResponse,
	AssistRequest,
	AssistResponse
} from '$lib/types';

export interface SteeredRequestRow {
	request_id: number;
	response_id: number | null;
	session_id: string;
	session_uuid: string;
	operator_turn_id: number | null;
	steerer_name: string | null;
	steerer_display_name: string | null;
	authority: Authority | null;
	mode: SteeringMode | null;
	verdict: Verdict | null;
	off_thread_topic: string | null;
	confidence: number | null;
	payload_text: string | null;
	anchor_text: string | null;
	verdict_json: Record<string, unknown> | null;
	steerer_model: string | null;
	sent_at: string | null;
	processed_at: string | null;
	expires_at: string | null;
	reverted_at: string | null;
	revert_reason: string | null;
	trigger_request_id: number | null;
	created_at: string;
}

export interface SessionSteeringRow {
	intervention_id: number;
	session_id: string;
	session_uuid: string;
	project_path: string | null;
	steerer_name: string | null;
	steerer_display_name: string | null;
	verdict: Verdict | null;
	authority: Authority | null;
	mode: SteeringMode | null;
	trigger_request_id: number | null;
	sent_request_id: number | null;
	sent_response_id: number | null;
	operator_turn_id: number | null;
	gate_reason: string | null;
	off_thread_topic: string | null;
	confidence: number | null;
	anchor_text: string | null;
	verdict_json: Record<string, unknown> | null;
	payload_text: string | null;
	steerer_model: string | null;
	sent_at: string | null;
	processed_at: string | null;
	expires_at: string | null;
	reverted_at: string | null;
	revert_reason: string | null;
	created_at: string;
	delivery_channel?: 'append' | 'wake' | null;
	deliver_after?: string | null;
	wake_state?: string | null;
	delivered_request_id?: number | null;
	evaluation_uuid?: string | null;
}

export interface SessionEnrolmentRow {
	enrolment_id: number;
	session_id: string;
	session_uuid: string;
	steerer_id: number;
	steerer_name: string | null;
	steerer_display_name: string | null;
	steerer_enabled: boolean;
	mode: SteeringMode | null;
	authority: Authority | null;
	enrolled_at: string;
	enrolled_by: string | null;
	disenrolled_at: string | null;
}

export interface SessionSteeringEnvelope {
	rows: SessionSteeringRow[];
	total_count: number;
	offset: number;
	limit: number;
}

export interface SessionSteeringParams {
	offset?: number;
	limit?: number;
}

const SESSION_STEERING_MAX_LIMIT = 500;

export interface PrecheckVerdict {
	send: boolean;
	score: number;
	why: string;
}

export interface Precheck {
	record_id: number | null;
	question_set: string;
	answers: Record<string, number>;
	refusals: number;
	verdict: PrecheckVerdict;
	fail_open: boolean;
	error: string | null;
}

export interface ChangedFileFact {
	path: string;
	kind: string;
	added: number;
	removed: number;
	committed: boolean | null;
}

export interface SessionEvaluationRow {
	id: number;
	uuid: string;
	steerer_id: number;
	steerer_name: string;
	steerer_kind: string;
	operator_turn_id: number;
	status: string;
	mode: string;
	question: string | null;
	trigger_point: string | null;
	trigger_request_id: number | null;
	bundle_id: number | null;
	intervention_uuid: string | null;
	dispatched_at: string;
	resolved_at: string | null;
	verdict_json: Record<string, unknown> | null;
	confidence: number | null;
	reason: string | null;
	outcome_label: string | null;
	label_source: string | null;
	labelled_at: string | null;
	label_evidence: unknown;
}

export interface SessionEvaluationsEnvelope {
	session_uuid: string;
	total_count: number;
	evaluations: SessionEvaluationRow[];
	next_before_id: number | null;
}

export async function getSessionEvaluations(
	sessionId: string,
	params: { limit?: number; before_id?: number } = {}
): Promise<SessionEvaluationsEnvelope> {
	const query = new URLSearchParams();
	if (params.limit !== undefined) query.set('limit', String(params.limit));
	if (params.before_id !== undefined) query.set('before_id', String(params.before_id));
	const suffix = query.size > 0 ? `?${query.toString()}` : '';
	const path = `/api/sessions/${encodeURIComponent(sessionId)}/evaluations${suffix}`;
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as SessionEvaluationsEnvelope;
	});
}

export function precheckOf(evaluation: SessionEvaluationRow | null): Precheck | null {
	const raw = evaluation?.verdict_json?.['precheck'];
	if (typeof raw !== 'object' || raw === null) return null;
	return raw as Precheck;
}

export function changedFilesOf(evaluation: SessionEvaluationRow | null): ChangedFileFact[] {
	const facts = evaluation?.verdict_json?.['facts'] as
		{ observation?: { changed?: ChangedFileFact[] } } | undefined;
	return facts?.observation?.changed ?? [];
}

export function isHeldEvaluation(evaluation: SessionEvaluationRow): boolean {
	return precheckOf(evaluation)?.verdict.send === false;
}

export async function getSteeredRequest(requestId: number): Promise<SteeredRequestRow | null> {
	const path = `/api/requests/${requestId}/steering`;
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (res.status === 404) return null;
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as SteeredRequestRow;
	});
}

export async function getSessionSteeringPage(
	sessionId: string,
	params: SessionSteeringParams = {}
): Promise<SessionSteeringEnvelope> {
	const query = new URLSearchParams();
	if (params.offset !== undefined) query.set('offset', String(params.offset));
	if (params.limit !== undefined) query.set('limit', String(params.limit));
	const suffix = query.size > 0 ? `?${query.toString()}` : '';
	const path = `/api/sessions/${encodeURIComponent(sessionId)}/steering${suffix}`;
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (res.status === 404) {
			return { rows: [], total_count: 0, offset: params.offset ?? 0, limit: params.limit ?? 0 };
		}
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as SessionSteeringEnvelope;
	});
}

export async function getSessionSteering(sessionId: string): Promise<SessionSteeringRow[]> {
	let rows: SessionSteeringRow[] = [];
	let offset = 0;
	for (;;) {
		const page = await getSessionSteeringPage(sessionId, {
			offset,
			limit: SESSION_STEERING_MAX_LIMIT
		});
		rows = rows.concat(page.rows);
		if (page.rows.length === 0 || rows.length >= page.total_count) break;
		offset = rows.length;
	}
	return rows;
}

export async function getSessionEnrolments(sessionId: string): Promise<SessionEnrolmentRow[]> {
	const path = `/api/sessions/${encodeURIComponent(sessionId)}/enrolments`;
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (res.status === 404) return [];
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		const data = (await res.json()) as SessionEnrolmentRow[] | { items: SessionEnrolmentRow[] };
		return Array.isArray(data) ? data : (data.items ?? []);
	});
}

export async function listSteerers(): Promise<SteererDto[]> {
	const path = '/api/steerers';
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as SteererDto[];
	});
}

export async function getSteerer(id: number): Promise<SteererDto> {
	const path = `/api/steerers/${id}`;
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as SteererDto;
	});
}

export async function createSteerer(body: SteererCreate): Promise<SteererEnvelope> {
	const path = '/api/steerers';
	const res = await fetch(path, {
		method: 'POST',
		headers: { 'content-type': 'application/json', accept: 'application/json' },
		body: JSON.stringify(body)
	});
	if (!res.ok) {
		throw new ApiError(`POST ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	void invalidateApiCache('/api/steerers');
	return (await res.json()) as SteererEnvelope;
}

export async function patchSteerer(id: number, patch: SteererPatch): Promise<SteererEnvelope> {
	const path = `/api/steerers/${id}`;
	const res = await fetch(path, {
		method: 'PATCH',
		headers: { 'content-type': 'application/json', accept: 'application/json' },
		body: JSON.stringify(patch)
	});
	if (!res.ok) {
		throw new ApiError(`PATCH ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	void invalidateApiCache('/api/steerers');
	return (await res.json()) as SteererEnvelope;
}

export interface SetAllSteererModelsResponse {
	count: number;
}

export async function setAllSteererModels(model: string): Promise<SetAllSteererModelsResponse> {
	const path = '/api/steerers/model';
	const res = await fetch(path, {
		method: 'PUT',
		headers: { 'content-type': 'application/json', accept: 'application/json' },
		body: JSON.stringify({ model })
	});
	if (!res.ok) {
		throw new ApiError(`PUT ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	void invalidateApiCache('/api/steerers');
	return (await res.json()) as SetAllSteererModelsResponse;
}

export async function deleteSteerer(id: number): Promise<void> {
	const path = `/api/steerers/${id}`;
	const res = await fetch(path, {
		method: 'DELETE',
		headers: { accept: 'application/json' }
	});
	if (!res.ok) {
		throw new ApiError(`DELETE ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	void invalidateApiCache('/api/steerers');
}

export async function enrolSession(
	sessionId: string,
	body: EnrolSessionBody
): Promise<EnrolSessionResponse> {
	const path = `/api/sessions/${encodeURIComponent(sessionId)}/enrolments`;
	const res = await fetch(path, {
		method: 'POST',
		headers: { 'content-type': 'application/json', accept: 'application/json' },
		body: JSON.stringify(body)
	});
	if (!res.ok) {
		throw new ApiError(`POST ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	void invalidateApiCache('/api/sessions');
	void invalidateApiCache('/api/steerers');
	return (await res.json()) as EnrolSessionResponse;
}

export async function disenrolSession(sessionId: string, enrolmentId: number): Promise<void> {
	const path = `/api/sessions/${encodeURIComponent(sessionId)}/enrolments/${enrolmentId}`;
	const res = await fetch(path, {
		method: 'DELETE',
		headers: { accept: 'application/json' }
	});
	if (!res.ok) {
		throw new ApiError(`DELETE ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	void invalidateApiCache('/api/sessions');
	void invalidateApiCache('/api/steerers');
}

export async function revertIntervention(
	interventionId: number,
	body: RevertInterventionBody
): Promise<void> {
	const path = `/api/interventions/${interventionId}/revert`;
	const res = await fetch(path, {
		method: 'PATCH',
		headers: { 'content-type': 'application/json', accept: 'application/json' },
		body: JSON.stringify(body)
	});
	if (!res.ok) {
		throw new ApiError(`PATCH ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	void invalidateApiCache('/api/sessions');
}

export interface VerifierCounts {
	fired: number;
	delivered: number;
}

export async function getVerifierCounts(sessionId: string): Promise<VerifierCounts | null> {
	const path = `/api/sessions/${encodeURIComponent(sessionId)}/verifier-counts`;
	try {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (res.status === 404) return null;
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as VerifierCounts;
	} catch (err) {
		if (err instanceof ApiError) throw err;
		return null;
	}
}

export async function listModels(): Promise<ModelsResponse | null> {
	const path = '/api/models';
	try {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (res.status === 502 || res.status === 503) return null;
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as ModelsResponse;
	} catch (err) {
		if (err instanceof ApiError) throw err;
		return null;
	}
}

export interface SteeringVocabularyItem {
	code: string;
	description: string;
}

export interface SteeringVocabulary {
	authorities: SteeringVocabularyItem[];
	modes: SteeringVocabularyItem[];
	verdicts: SteeringVocabularyItem[];
}

export async function getSteeringVocabulary(): Promise<SteeringVocabulary> {
	const path = '/api/steering/vocabulary';
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as SteeringVocabulary;
	});
}

export async function assistSteerer(body: AssistRequest): Promise<AssistResponse> {
	const path = '/api/steerers/assist';
	const res = await fetch(path, {
		method: 'POST',
		headers: { 'content-type': 'application/json', accept: 'application/json' },
		body: JSON.stringify(body)
	});
	if (!res.ok) {
		throw new ApiError(`POST ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	return (await res.json()) as AssistResponse;
}

export interface OpenRouterKeyResult {
	valid: boolean;
	label?: string;
	error?: string;
}

export async function setOpenRouterKey(key: string): Promise<OpenRouterKeyResult> {
	const path = '/api/settings/steerer/openrouter/key';
	const res = await fetch(path, {
		method: 'PUT',
		headers: { 'content-type': 'application/json', accept: 'application/json' },
		body: JSON.stringify({ key })
	});
	if (res.status !== 200 && res.status !== 422) {
		throw new ApiError(`PUT ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	const result = (await res.json()) as OpenRouterKeyResult;
	if (result.valid) await invalidateApiCache('/api/usage/openrouter');
	return result;
}
