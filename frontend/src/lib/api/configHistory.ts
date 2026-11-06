import { ApiError } from '$lib/api';

export type ChangeSubjectKind = 'setting' | 'toggle' | 'steerer' | 'build' | 'promotion';
export type ChangeVia = 'spa' | 'api' | 'startup' | 'migration' | 'promotion' | 'mike' | 'sql';

export interface ChangeLogRow {
	id: number;
	changed_at: string;
	xact_id: string;
	subject_kind: ChangeSubjectKind;
	subject_key: string;
	op: 'insert' | 'update' | 'delete' | 'soft_delete' | 'restore' | 'observe';
	old_value: unknown;
	new_value: unknown;
	redacted: boolean;
	via: ChangeVia;
	actor: string | null;
	request_id: string | null;
	build_sha: string | null;
	db_user: string;
	application_name: string | null;
	detail: Record<string, unknown>;
}

export interface ChangeLogPage {
	changes: ChangeLogRow[];
	next_before_id: number | null;
}

export interface ChangeLogQuery {
	subject_kind?: ChangeSubjectKind;
	subject_key_prefix?: string;
	via?: ChangeVia;
	before_id?: number;
	limit?: number;
}

export interface SnapshotEntry {
	subject_kind: 'setting' | 'toggle' | 'steerer';
	subject_key: string;
	value: unknown;
	source: 'stored' | 'code_default' | 'unknown';
	redacted: boolean;
	changed_at: string | null;
	change_id: number | null;
	via: ChangeVia | null;
}

export interface ConfigSnapshot {
	at: string;
	baseline_at: string | null;
	build: { change_id: number; sha: string | null; changed_at: string } | null;
	entries: SnapshotEntry[];
}

async function getJson<T>(path: string): Promise<T> {
	const res = await fetch(path, { headers: { accept: 'application/json' } });
	if (!res.ok) {
		throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	return (await res.json()) as T;
}

export function changeLogPath(query: ChangeLogQuery): string {
	const params = new URLSearchParams();
	if (query.subject_kind) params.set('subject_kind', query.subject_kind);
	if (query.subject_key_prefix) params.set('subject_key_prefix', query.subject_key_prefix);
	if (query.via) params.set('via', query.via);
	if (query.before_id !== undefined) params.set('before_id', String(query.before_id));
	if (query.limit !== undefined) params.set('limit', String(query.limit));
	const qs = params.toString();
	return qs ? `/api/config/change-log?${qs}` : '/api/config/change-log';
}

export function listConfigChanges(query: ChangeLogQuery = {}): Promise<ChangeLogPage> {
	return getJson<ChangeLogPage>(changeLogPath(query));
}

export function getConfigSnapshot(at: string): Promise<ConfigSnapshot> {
	return getJson<ConfigSnapshot>(`/api/config/snapshot?at=${encodeURIComponent(at)}`);
}

export const VIA_WORDS: Record<ChangeVia, string> = {
	spa: 'changed in the mike UI',
	api: 'changed through the REST API',
	startup: 'recorded when mike started',
	migration: 'written by a database migration',
	promotion: 'a promotion',
	mike: 'written by mike itself',
	sql: 'changed by a direct database edit (psql)'
};
