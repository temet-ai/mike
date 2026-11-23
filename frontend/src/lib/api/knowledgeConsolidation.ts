import { ApiError } from '$lib/api';
import type { CapabilityRow } from '$lib/api/capabilities';

/**
 * Client for knowledge consolidation: what the worker changed in a project's lessons and
 * guidance files, its per-run measures, and the subjects it groups statements into.
 * Shapes follow the Rust DTOs in crates/mike-lessons/src/api/knowledge_consolidation*.rs.
 * Reads go straight to fetch: the section polls, so a cached answer would hide a change.
 */

export type Verdict =
	'kept' | 'covered' | 'superseded' | 'merged' | 'tightened' | 'misgrouped' | 'conflict_open';

export type Application = 'applied' | 'proposed' | 'failed' | 'undone' | 'none';

export type StatementState = 'current' | 'covered' | 'superseded' | 'merged' | 'gone';

export interface Carrier {
	reference: string;
	kind: 'lesson' | 'file';
	label: string;
	lesson_id: number | null;
	source_name: string | null;
	relative_path: string | null;
	scope: 'project' | 'machine';
	stated_at: string | null;
}

export interface EvidenceItem {
	label: string;
	note: string;
	at: string | null;
	operator: boolean;
}

export interface DecisionRow {
	id: number;
	run_id: number;
	project_id: number | null;
	subject_id: number | null;
	subject_title: string | null;
	statement_id: number | null;
	item_id: number | null;
	carrier_reference: string;
	verdict: Verdict;
	counterpart_statement_id: number | null;
	counterpart_reference: string | null;
	text_before: string;
	text_after: string | null;
	reason: string;
	evidence: EvidenceItem[];
	model: string;
	application: Application;
	applied_at: string | null;
	error: string | null;
	created_at: string;
	carrier: Carrier | null;
	counterpart: Carrier | null;
	counterpart_text: string | null;
}

export interface SubjectSummary {
	id: number;
	project_id: number | null;
	title: string;
	text: string;
	members: number;
	carriers: number;
	changed_at: string | null;
	judged_at: string | null;
	newest_decision_at: string | null;
	newest_verdict: Verdict | null;
	newest_reason: string | null;
}

export interface SubjectStatement {
	statement_id: number;
	text: string;
	carrier: Carrier;
	state: StatementState;
	resolved_by_statement_id: number | null;
	stated_at: string;
}

export interface SubjectDetail extends SubjectSummary {
	statements: SubjectStatement[];
}

export interface MeasureSeriesRow {
	id: number;
	run_id: number;
	project_id: number | null;
	measured_at: string;
	statements_current: number;
	subjects: number;
	subjects_shared: number;
	conflicts_open: number;
	guidance_chars: number;
	misgrouped_share: number;
	split_subject_share: number;
	grouping_distance: number;
}

export interface DecisionPage {
	decisions: DecisionRow[];
	/** Id of the last row when the page came back full; pass it as `before` for the next page. */
	next_before: number | null;
}

export interface DecisionsQuery {
	project: string;
	subject?: number;
	before?: number;
	limit?: number;
	verdicts?: Verdict[];
	open?: boolean;
}

export interface SettleResult {
	decision: DecisionRow;
	conflict: DecisionRow;
}

const BASE = '/api/knowledge/consolidation';

function query(params: Record<string, string | number | boolean | undefined>): string {
	const search = new URLSearchParams();
	for (const [key, value] of Object.entries(params)) {
		if (value === undefined || value === '') continue;
		search.set(key, String(value));
	}
	const text = search.toString();
	return text ? `?${text}` : '';
}

async function failure(method: string, path: string, res: Response): Promise<ApiError> {
	let message = `${method} ${path} failed: ${res.status} ${res.statusText}`;
	try {
		const body = (await res.json()) as { error?: unknown };
		if (typeof body.error === 'string' && body.error !== '') message = body.error;
	} catch {
		// the status line stands in for a body that is not JSON
	}
	return new ApiError(message, res.status, path);
}

async function getJson<T>(path: string): Promise<T> {
	const res = await fetch(path, { headers: { accept: 'application/json' } });
	if (!res.ok) throw await failure('GET', path, res);
	return (await res.json()) as T;
}

async function postJson<T>(path: string, body?: unknown): Promise<T> {
	const res = await fetch(path, {
		method: 'POST',
		headers: { accept: 'application/json', 'content-type': 'application/json' },
		body: body === undefined ? undefined : JSON.stringify(body)
	});
	if (!res.ok) throw await failure('POST', path, res);
	return (await res.json()) as T;
}

export async function getConsolidationMeasures(
	project: string,
	opts: { since?: string; limit?: number } = {}
): Promise<MeasureSeriesRow[]> {
	const data = await getJson<{ measures: MeasureSeriesRow[] }>(
		`${BASE}/measures${query({ project, since: opts.since, limit: opts.limit })}`
	);
	return data.measures;
}

export async function getConsolidationSubjects(
	project: string,
	opts: { limit?: number; offset?: number } = {}
): Promise<SubjectSummary[]> {
	const data = await getJson<{ subjects: SubjectSummary[] }>(
		`${BASE}/subjects${query({ project, limit: opts.limit, offset: opts.offset })}`
	);
	return data.subjects;
}

export async function getConsolidationDecisions(q: DecisionsQuery): Promise<DecisionPage> {
	const data = await getJson<DecisionPage>(
		`${BASE}/decisions${query({
			project: q.project,
			subject: q.subject,
			before: q.before,
			limit: q.limit,
			verdict: q.verdicts && q.verdicts.length > 0 ? q.verdicts.join(',') : undefined,
			open: q.open ? true : undefined
		})}`
	);
	return { decisions: data.decisions, next_before: data.next_before ?? null };
}

export function getConsolidationSubject(id: number): Promise<SubjectDetail> {
	return getJson<SubjectDetail>(`${BASE}/subjects/${id}`);
}

export async function undoConsolidationDecision(id: number): Promise<DecisionRow> {
	return (await postJson<{ decision: DecisionRow }>(`${BASE}/decisions/${id}/undo`)).decision;
}

export async function applyConsolidationDecision(id: number): Promise<DecisionRow> {
	return (await postJson<{ decision: DecisionRow }>(`${BASE}/decisions/${id}/apply`)).decision;
}

export function settleConsolidationConflict(
	id: number,
	keepStatementId: number
): Promise<SettleResult> {
	return postJson<SettleResult>(`${BASE}/decisions/${id}/settle`, {
		keep_statement_id: keepStatementId
	});
}

/** The worker's row of `/api/capabilities`, or null when this mike does not report it. */
export async function getConsolidationCapability(): Promise<CapabilityRow | null> {
	const data = await getJson<{ capabilities?: CapabilityRow[] }>('/api/capabilities');
	return (data.capabilities ?? []).find((row) => row.id === 'knowledge_consolidation') ?? null;
}
