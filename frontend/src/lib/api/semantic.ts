import { ApiError } from '$lib/api';

export interface CandidateMember {
	candidate_id?: number;
	item_id: number;
	ord?: number;
	text?: string | null;
	session_id?: number | null;
	operator_turn_id?: number | null;
	effective_at?: string | null;
	similarity_to_medoid?: number | null;
	status?: 'gone';
}

export interface PatternCandidate {
	id: number;
	status: 'open' | 'named' | 'dismissed' | 'superseded' | string;
	window_start: string;
	window_end: string;
	member_item_ids: number[];
	member_count: number;
	session_count: number;
	medoid_item_id: number | null;
	created_at: string;
	decided_at: string | null;
	decision_note: string | null;
	named_pattern: string | null;
}

export interface CandidateEntry {
	candidate: PatternCandidate;
	members: CandidateMember[];
}

export interface CandidatesResponse {
	candidates: CandidateEntry[];
	total: number;
	limit: number;
	offset: number;
}

export const SLUG_PATTERN = /^[a-z0-9-]{3,64}$/;

export function validSlug(slug: string): boolean {
	return SLUG_PATTERN.test(slug);
}

export function medoidText(entry: CandidateEntry): string {
	const medoid = entry.members.find((m) => m.item_id === entry.candidate.medoid_item_id);
	return medoid?.text ?? '';
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

export async function listCandidates(status = 'open'): Promise<CandidatesResponse> {
	const body = await send<CandidatesResponse>(
		'GET',
		`/api/semantic/patterns/candidates?status=${status}`
	);
	if (!Array.isArray(body?.candidates)) {
		throw new Error('the pattern candidates response has no candidates list');
	}
	return body;
}

export function nameCandidate(id: number, pattern: string, description: string): Promise<unknown> {
	return send('POST', `/api/semantic/patterns/candidates/${id}/name`, { pattern, description });
}

export function dismissCandidate(id: number, note: string): Promise<unknown> {
	return send('POST', `/api/semantic/patterns/candidates/${id}/dismiss`, { note });
}
