import { ApiError } from '$lib/api';

/** The definition of a custom kind; null for a built-in kind. */
export interface KnowledgeKindDefinition {
	source_table: string;
	key_column: string;
	title_column: string;
	body_columns: string[];
	project_column: string | null;
}

/** One table a kind watches, and the columns whose change re-embeds an item. */
export interface KnowledgeKindWatch {
	table: string;
	key_column: string;
	watch_columns: string[];
}

export interface KnowledgeKindStats {
	items: number;
	embedded: number;
	waiting: number;
	failed: number;
	newest_error: string | null;
	newest_error_at: string | null;
	oldest_waiting_ready_at: string | null;
}

/** One row of `GET /api/knowledge/kinds`. */
export interface KnowledgeKind {
	kind: string;
	label: string;
	description: string;
	built_in: boolean;
	enabled: boolean;
	settle_secs: number;
	body_view: string | null;
	generated_view: string | null;
	definition: KnowledgeKindDefinition | null;
	watches: KnowledgeKindWatch[];
	stats: KnowledgeKindStats;
	/** The oldest waiting item has been ready for more than five minutes. */
	behind: boolean;
}

/** The fields a 400 can name. */
export type KnowledgeKindField =
	| 'kind'
	| 'label'
	| 'description'
	| 'source_table'
	| 'key_column'
	| 'title_column'
	| 'body_columns'
	| 'project_column'
	| 'watch_columns'
	| 'settle_secs'
	| 'view';

/** The body of a preview and of a create. A view kind sends `view` and one watch's fields. */
export interface KnowledgeKindDraft {
	kind: string;
	label: string;
	description: string;
	source_table: string;
	key_column: string;
	title_column: string | null;
	body_columns: string[];
	project_column: string | null;
	watch_columns: string[];
	settle_secs: number | null;
	view: string | null;
}

export interface KnowledgeKindPreviewRow {
	reference: string;
	header: string;
	body: string;
}

/** The fields `PATCH /api/knowledge/kinds/{kind}` accepts; send only what changed. */
export interface KnowledgeKindPatch {
	enabled?: boolean;
	settle_secs?: number;
	watch_columns?: string[];
	label?: string;
	description?: string;
}

export interface KnowledgeTable {
	name: string;
	columns: { name: string; type: string }[];
}

/** An API refusal that may name the field it is about. */
export class KnowledgeKindError extends ApiError {
	constructor(
		message: string,
		status: number,
		url: string,
		readonly field: KnowledgeKindField | null,
		readonly watchIndex: number | null
	) {
		super(message, status, url);
		this.name = 'KnowledgeKindError';
	}
}

const FIELDS: readonly string[] = [
	'kind',
	'label',
	'description',
	'source_table',
	'key_column',
	'title_column',
	'body_columns',
	'project_column',
	'watch_columns',
	'settle_secs',
	'view'
];

async function failure(method: string, path: string, res: Response): Promise<KnowledgeKindError> {
	let message = `${method} ${path} failed: ${res.status} ${res.statusText}`;
	let field: KnowledgeKindField | null = null;
	let watchIndex: number | null = null;
	const body = (await res.json().catch(() => null)) as {
		error?: unknown;
		field?: unknown;
		watch_index?: unknown;
	} | null;
	if (typeof body?.error === 'string' && body.error.length > 0) message = body.error;
	if (typeof body?.field === 'string' && FIELDS.includes(body.field)) {
		field = body.field as KnowledgeKindField;
	}
	if (typeof body?.watch_index === 'number') watchIndex = body.watch_index;
	return new KnowledgeKindError(message, res.status, path, field, watchIndex);
}

const JSON_HEADERS = { 'content-type': 'application/json', accept: 'application/json' };
const BASE = '/api/knowledge/kinds';

export async function getKnowledgeKinds(): Promise<KnowledgeKind[]> {
	const res = await fetch(BASE, { headers: { accept: 'application/json' } });
	if (!res.ok) throw await failure('GET', BASE, res);
	return ((await res.json()) as { kinds: KnowledgeKind[] }).kinds;
}

/** The tables and columns the form can suggest. */
export async function getKnowledgeTables(): Promise<KnowledgeTable[]> {
	const path = `${BASE}/tables`;
	const res = await fetch(path, { headers: { accept: 'application/json' } });
	if (!res.ok) throw await failure('GET', path, res);
	return ((await res.json()) as { tables: KnowledgeTable[] }).tables;
}

/** A real row of the draft's table as it would be embedded; null when the table has no row yet. */
export async function previewKnowledgeKind(
	draft: KnowledgeKindDraft
): Promise<KnowledgeKindPreviewRow | null> {
	const path = `${BASE}/preview`;
	const res = await fetch(path, {
		method: 'POST',
		headers: JSON_HEADERS,
		body: JSON.stringify(draft)
	});
	if (!res.ok) throw await failure('POST', path, res);
	return ((await res.json()) as { preview: KnowledgeKindPreviewRow | null }).preview;
}

export async function createKnowledgeKind(draft: KnowledgeKindDraft): Promise<string> {
	const res = await fetch(BASE, {
		method: 'POST',
		headers: JSON_HEADERS,
		body: JSON.stringify(draft)
	});
	if (!res.ok) throw await failure('POST', BASE, res);
	return ((await res.json()) as { kind: string }).kind;
}

export async function patchKnowledgeKind(
	kind: string,
	patch: KnowledgeKindPatch
): Promise<KnowledgeKind> {
	const path = `${BASE}/${encodeURIComponent(kind)}`;
	const res = await fetch(path, {
		method: 'PATCH',
		headers: JSON_HEADERS,
		body: JSON.stringify(patch)
	});
	if (!res.ok) throw await failure('PATCH', path, res);
	return (await res.json()) as KnowledgeKind;
}

export async function deleteKnowledgeKind(kind: string): Promise<void> {
	const path = `${BASE}/${encodeURIComponent(kind)}`;
	const res = await fetch(path, { method: 'DELETE', headers: { accept: 'application/json' } });
	if (!res.ok) throw await failure('DELETE', path, res);
}

/** Kinds in the order the table draws them: plain code-point comparison on `kind`. */
export function sortKinds(kinds: readonly KnowledgeKind[]): KnowledgeKind[] {
	return [...kinds].sort((a, b) => (a.kind < b.kind ? -1 : a.kind > b.kind ? 1 : 0));
}

/** What the Source column shows: the view's name when the kind reads from one, else its table. */
export function kindSource(k: KnowledgeKind): string {
	if (k.body_view !== null && k.body_view !== '') return k.body_view;
	if (k.definition !== null) return k.definition.source_table;
	return k.watches[0]?.table ?? '—';
}
