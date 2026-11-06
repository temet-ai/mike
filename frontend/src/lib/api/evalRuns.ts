import { ApiError } from '$lib/api';
import type { EvalLabelDto } from '$lib/api/eval';

export interface ClassScoreDto {
	tp: number;
	fp: number;
	fn: number;
	precision: number | null;
	recall: number | null;
	f1: number | null;
}

export interface AxisReportDto {
	n: number;
	agreement: number;
	kappa: number | null;
	confusion?: Record<string, Record<string, number>>;
	per_class?: Record<string, ClassScoreDto>;
}

export interface EvalRunReportDto {
	n?: number;
	reaction?: AxisReportDto;
	delivery?: AxisReportDto;
	corrected?: ClassScoreDto;
	items_skipped?: number;
	items_failed?: number;
	items_refused?: number;
	[key: string]: unknown;
}

export interface EvalRunDto {
	id: number;
	created_at: string;
	finished_at: string | null;
	purpose: string;
	status: string;
	bundle_id: number;
	rubric_version: string;
	input_builder_version: string;
	slices: string[];
	model: string;
	split: string;
	tiers: string[];
	holdout_batch: string | null;
	labels_as_of: string;
	label_count: number;
	label_ids_hash: string;
	repeat_of_run_id: number | null;
	max_usd: number;
	cost_usd: number;
	items_judged: number;
	items_refused: number;
	items_failed: number;
	report: EvalRunReportDto | null;
	argv: Record<string, unknown>;
}

export interface EvalVerdictDto {
	id: number;
	reaction: string | null;
	delivery: string | null;
	confidence: number;
	rationale: string;
	unmet_ask: string;
	evidence_quality: string;
	input_text?: string | null;
	severity: string | null;
	model: string;
	bundle_id: number | null;
	[key: string]: unknown;
}

export interface EvalRunItemDto {
	run_id: number;
	operator_turn_id: number;
	reaction_label_id: number | null;
	delivery_label_id: number | null;
	turn_verdict_id: number | null;
	outcome: string;
	stratum: string;
	inclusion_prob: number | null;
	reaction_correct: boolean | null;
	delivery_correct: boolean | null;
	created_at: string;
	gold_reaction: string | null;
	gold_delivery: string | null;
	gold_control: boolean | null;
	verdict_reaction: string | null;
	verdict_delivery: string | null;
	verdict_legacy_label: string | null;
	next_state: string | null;
	verdict: EvalVerdictDto | null;
	gold_severity?: string | null;
	current_reaction?: string | null;
	current_delivery?: string | null;
	reviewed_at?: string | null;
	session_uuid?: string | null;
	session_title?: string | null;
}

export interface EvalRunItemsPage {
	items: EvalRunItemDto[];
	total: number;
	reviewed: number;
	limit: number;
	offset: number;
}

export interface EvalRunsPage {
	items: EvalRunDto[];
	total: number;
	limit: number;
	offset: number;
}

export type TextState = 'present' | 'empty_turn' | 'missing';
export type NextMessageState = 'present' | 'none' | 'pending' | 'missing' | 'unknown';

export interface EvalReviewContextDto {
	operator_turn_id: number;
	session_uuid: string;
	closed_at: string | null;
	user_prompt: string | null;
	final_assistant_text: string | null;
	facts_present: boolean;
	text_chars: number | null;
	text_cut_at: number | null;
	tool_call_count: number | null;
	tool_error_count: number | null;
	next_state: string | null;
	next_genuine_turn_id: number | null;
	next_genuine_request_id: number | null;
	next_genuine_at: string | null;
	next_genuine_text: string | null;
	prompt_state: TextState;
	assistant_state: TextState;
	next_message_state: NextMessageState;
}

export interface EvalReviewDetailDto {
	item: EvalRunItemDto;
	context: EvalReviewContextDto | null;
	labels: {
		run_gold: EvalLabelDto[];
		current: EvalLabelDto[];
		reviews: EvalLabelDto[];
	};
}

export interface BundleComponentsDto {
	model?: string;
	reasoning_effort?: string;
	rubric_version?: string | number;
	[key: string]: unknown;
}

export interface BundleDto {
	id: number;
	role: string;
	origin: string;
	status: string;
	content_sha256: string;
	created_at?: string;
	promoted_at?: string | null;
	override_reason?: string | null;
	components?: BundleComponentsDto;
	[key: string]: unknown;
}

export type Disagreement = 'any' | 'reaction' | 'delivery';

export interface RunItemsQuery {
	outcome?: string;
	reaction_correct?: boolean;
	delivery_correct?: boolean;
	disagreement?: Disagreement;
	gold_reaction?: string;
	gold_delivery?: string;
	verdict_reaction?: string;
	verdict_delivery?: string;
	severity?: string;
	reviewed?: boolean;
	include?: Array<'verdict' | 'review'>;
	limit?: number;
	offset?: number;
}

export function itemsQueryString(q: RunItemsQuery): string {
	const params = new URLSearchParams();
	const entries = Object.entries(q).sort(([a], [b]) => a.localeCompare(b));
	for (const [key, value] of entries) {
		if (value === undefined || value === null || value === '') continue;
		if (Array.isArray(value)) {
			if (value.length > 0) params.set(key, value.join(','));
			continue;
		}
		params.set(key, String(value));
	}
	const text = params.toString();
	return text ? `?${text}` : '';
}

async function getJson<T>(path: string): Promise<T> {
	const res = await fetch(path, { headers: { accept: 'application/json' } });
	if (!res.ok) {
		let message = `${res.status} ${res.statusText}`;
		try {
			const body = (await res.json()) as { error?: string };
			if (body.error) message = body.error;
		} catch {
			message = `${res.status} ${res.statusText}`;
		}
		throw new ApiError(`${path}: ${message}`, res.status, path);
	}
	return (await res.json()) as T;
}

export function listEvalRuns(
	q: { role?: string; status?: string; limit?: number } = {}
): Promise<EvalRunsPage> {
	const params = new URLSearchParams();
	if (q.role) params.set('role', q.role);
	if (q.status) params.set('status', q.status);
	if (q.limit) params.set('limit', String(q.limit));
	const text = params.toString();
	return getJson(`/api/eval/runs${text ? `?${text}` : ''}`);
}

export function getEvalRun(id: number): Promise<EvalRunDto> {
	return getJson(`/api/eval/runs/${id}`);
}

export function listEvalRunItems(runId: number, q: RunItemsQuery): Promise<EvalRunItemsPage> {
	return getJson(`/api/eval/runs/${runId}/items${itemsQueryString(q)}`);
}

export function getEvalReviewDetail(runId: number, turnId: number): Promise<EvalReviewDetailDto> {
	return getJson(`/api/eval/runs/${runId}/items/${turnId}/review`);
}

export function getEvalBundle(id: number): Promise<BundleDto> {
	return getJson(`/api/eval/bundles/${id}`);
}

export function listTurnJudgeBundles(): Promise<{ bundles: BundleDto[] }> {
	return getJson('/api/eval/bundles?role=turn_judge&limit=200');
}

export function listEvalBundles(role?: string): Promise<{ bundles: BundleDto[] }> {
	const params = new URLSearchParams({ limit: '200' });
	if (role) params.set('role', role);
	return getJson(`/api/eval/bundles?${params.toString()}`);
}

export interface EvalGateDto {
	id: number;
	created_at: string;
	candidate_run_id: number;
	baseline_run_id: number | null;
	noise_run_id: number | null;
	verdict: 'pass' | 'fail' | string;
	floors: { precision?: number; recall?: number; [key: string]: unknown } | null;
	report: {
		corrected?: { precision?: number | null; recall?: number | null };
		regression?: { corrected?: { gained?: number; lost?: number } };
		[key: string]: unknown;
	} | null;
	override_reason: string | null;
}

export function listEvalGates(): Promise<{ items: EvalGateDto[]; total: number }> {
	return getJson('/api/eval/gates?limit=200');
}

export function accuracy(axis: AxisReportDto | undefined): number | null {
	if (!axis || axis.n === 0) return null;
	return axis.agreement / axis.n;
}

export function isDisagreement(item: EvalRunItemDto): boolean {
	return item.reaction_correct === false || item.delivery_correct === false;
}

export interface OperatorTurnFactsDto {
	turn_id: number;
	session_id: string;
	start_request_id: number | null;
	end_request_id: number | null;
	start_request_number: number | null;
	end_request_number: number | null;
	stop_request_id: number | null;
	created_at: string | null;
	duration_ms: number | null;
	tool_call_count: number | null;
	tool_error_count: number | null;
	files_touched: string[];
	tools_used: string[];
	skills_used: string[];
	agents_dispatched: string[];
	stop_response: string | null;
	stop_reason: string | null;
	interrupted: boolean | null;
	[key: string]: unknown;
}

export function getOperatorTurnFacts(turnId: number): Promise<OperatorTurnFactsDto> {
	return getJson(`/api/operator-turns/${turnId}`);
}
