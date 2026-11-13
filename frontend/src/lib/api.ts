import { cachedGet, invalidateApiCache } from '$lib/stores/cache/apiCache.svelte';
import type {
	Agent,
	AgentDetail,
	AgentDispatchDto,
	AgentListItem,
	AgentPatch,
	CompositionUsageStats,
	BodyPayload,
	CompositionRevision,
	ContentSearchResponse,
	DiagnosticCategory,
	DiagnosticStats,
	FilterDryRunRequest,
	FilterDryRunResponse,
	FilterRuleCreate,
	FilterRuleDeleteResponse,
	FilterRuleDto,
	FilterRuleMutationEnvelope,
	FilterRulePatch,
	Finding,
	FindTextUnifiedResponse,
	HealthResponse,
	OperatorTurnDto,
	OperatorTurnOutcomeSummary,
	PromptAnomalyGroup,
	PromptClassDetailDto,
	PromptClassDto,
	PromptClassVersionsDto,
	PromptClassInstructionsListDto,
	SuggestPromptNameDto,
	PromptComposition,
	PromptCompositionCreate,
	PromptCompositionListItem,
	PromptCompositionMode,
	PromptCompositionMutationEnvelope,
	PromptCompositionPatch,
	ProjectSummaryDto,
	RecallProjectContextDto,
	KnowledgeReadResponse,
	KnowledgeSearchResponse,
	RequestDetail,
	RequestDeltasPayload,
	RequestStreamBlocksPayload,
	RequestSummary,
	SessionDetail,
	SessionHead,
	SessionProjectAssignmentResponse,
	SessionSummary,
	StatsResponse,
	TimeSpentDto,
	ToolErrorFacets,
	ToolErrorsResponse,
	ToolUseFacets,
	ToolUsesFeedResponse,
	ToolUsesResponse,
	TransformationManifestPayload,
	VolatileLinePattern,
	VolatileLinePatternCreate,
	WorkflowRunDto
} from './types';

export class ApiError extends Error {
	constructor(
		message: string,
		readonly status: number,
		readonly url: string
	) {
		super(message);
		this.name = 'ApiError';
	}
}

const RETRY_DELAYS_DEFAULT = [500, 1500, 4500] as const;
let _retryDelaysMs: readonly number[] = RETRY_DELAYS_DEFAULT;

export function __setRetryDelaysForTesting(delays: readonly number[]): void {
	_retryDelaysMs = delays;
}

export function __resetRetryDelaysForTesting(): void {
	_retryDelaysMs = RETRY_DELAYS_DEFAULT;
}

export function isRetryableError(err: unknown): boolean {
	if (err instanceof TypeError) return true;
	if (err instanceof ApiError) {
		return err.status >= 500 || err.status === 429 || err.status === 408;
	}
	return false;
}

export async function withRetry<T>(fn: () => Promise<T>): Promise<T> {
	let lastErr: unknown;
	for (let attempt = 0; attempt <= _retryDelaysMs.length; attempt++) {
		try {
			return await fn();
		} catch (err) {
			lastErr = err;
			if (!isRetryableError(err)) throw err;
			const delay = _retryDelaysMs[attempt];
			if (delay === undefined) break;
			await new Promise<void>((resolve) => setTimeout(resolve, delay));
		}
	}
	throw lastErr;
}

async function rawGetJson<T>(path: string): Promise<T> {
	return withRetry(async () => {
		const res = await fetch(path, {
			headers: { accept: 'application/json' }
		});
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as T;
	});
}

function getJson<T>(path: string): Promise<T> {
	return cachedGet<T>(path, () => rawGetJson<T>(path));
}

async function postJson<Req, Res>(path: string, body: Req): Promise<Res> {
	const res = await fetch(path, {
		method: 'POST',
		headers: {
			accept: 'application/json',
			'content-type': 'application/json'
		},
		body: JSON.stringify(body)
	});
	if (!res.ok) {
		let message = `POST ${path} failed: ${res.status} ${res.statusText}`;
		try {
			const errBody = (await res.json()) as { error?: string };
			if (errBody && typeof errBody.error === 'string' && errBody.error.length > 0) {
				message = errBody.error;
			}
		} catch {
			// Keep the HTTP status fallback when the response body is not valid JSON.
		}
		throw new ApiError(message, res.status, path);
	}
	return (await res.json()) as Res;
}

async function patchJson<Req, Res>(path: string, body: Req): Promise<Res> {
	const res = await fetch(path, {
		method: 'PATCH',
		headers: {
			accept: 'application/json',
			'content-type': 'application/json'
		},
		body: JSON.stringify(body)
	});
	if (!res.ok) {
		let message = `PATCH ${path} failed: ${res.status} ${res.statusText}`;
		try {
			const errBody = (await res.json()) as { error?: string };
			if (errBody && typeof errBody.error === 'string' && errBody.error.length > 0) {
				message = errBody.error;
			}
		} catch {
			// Keep the HTTP status fallback when the response body is not valid JSON.
		}
		throw new ApiError(message, res.status, path);
	}
	return (await res.json()) as Res;
}

export function getHealth(): Promise<HealthResponse> {
	// Reachability must come from this request, never a prior successful cache entry.
	return cachedGet('/api/health', () => rawGetJson<HealthResponse>('/api/health'), 'passthrough');
}

export type SortOrder = 'asc' | 'desc';

export interface ListRequestsParams {
	offset?: number;
	limit?: number;
	order?: SortOrder;
	prompt_sha256?: string;
	cursor?: string;
}

export interface RequestsPage {
	rows: RequestSummary[];
	nextCursor: string | null;
}

function listRequestsPath(params: ListRequestsParams): string {
	const query = new URLSearchParams();
	if (params.offset !== undefined) {
		query.set('offset', String(params.offset));
	}
	if (params.limit !== undefined) {
		query.set('limit', String(params.limit));
	}
	if (params.order !== undefined) {
		query.set('order', params.order);
	}
	if (params.prompt_sha256 !== undefined && params.prompt_sha256.length > 0) {
		query.set('prompt_sha256', params.prompt_sha256);
	}
	if (params.cursor !== undefined && params.cursor.length > 0) {
		query.set('cursor', params.cursor);
	}
	const suffix = query.size > 0 ? `?${query.toString()}` : '';
	return `/api/requests${suffix}`;
}

export function listRequests(params: ListRequestsParams = {}): Promise<RequestSummary[]> {
	return getJson<RequestSummary[]>(listRequestsPath(params));
}

export async function listRequestsPage(params: ListRequestsParams = {}): Promise<RequestsPage> {
	const path = listRequestsPath(params);
	const res = await fetch(path, { headers: { accept: 'application/json' } });
	if (!res.ok) {
		throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	return {
		rows: (await res.json()) as RequestSummary[],
		nextCursor: res.headers.get('x-next-cursor')
	};
}

export function getRequest(id: number): Promise<RequestDetail> {
	return getJson<RequestDetail>(`/api/requests/${id}`);
}

export function getRequestDetail(id: number): Promise<RequestDetail> {
	return getRequest(id);
}

export function getRequestSummary(id: number): Promise<RequestSummary> {
	return getJson<RequestSummary>(`/api/requests/${id}/summary`);
}

export function getRequestBody(id: number): Promise<BodyPayload> {
	return getJson<BodyPayload>(`/api/requests/${id}/body/request`);
}

export function getResponseBody(id: number): Promise<BodyPayload> {
	return getJson<BodyPayload>(`/api/requests/${id}/body/response`);
}

export function getStreamChunks(id: number): Promise<RequestStreamBlocksPayload> {
	return getJson<RequestStreamBlocksPayload>(`/api/requests/${id}/stream_chunks`);
}

export function getRequestDeltas(sessionId: string, id: number): Promise<RequestDeltasPayload> {
	return getJson<RequestDeltasPayload>(`/api/sessions/${sessionId}/requests/${id}/deltas`);
}

export function getAccumulatedText(id: number): Promise<string> {
	return getStreamChunks(id).then((payload) =>
		payload.stream_blocks
			.filter((b) => b.block_type === 'text')
			.sort((a, b) => a.seq_ord - b.seq_ord)
			.map((b) => b.content)
			.join('')
	);
}

export function getRequestSentBody(id: number): Promise<BodyPayload> {
	return getJson<BodyPayload>(`/api/requests/${id}/body/request_sent`);
}

export function getRequestTransformationManifest(
	id: number
): Promise<TransformationManifestPayload> {
	return getJson<TransformationManifestPayload>(`/api/requests/${id}/manifest`);
}

export function getStats(): Promise<StatsResponse> {
	return getJson<StatsResponse>('/api/stats');
}

export interface FindTextParams {
	needle: string;
	source?: 'request' | 'request_sent' | 'response';
	request_id?: number;
	session_id?: string;
	case_sensitive?: boolean;
	context_bytes?: number;
	max_matches?: number;
	date_from?: string;
	date_to?: string;
}

export function findText(params: FindTextParams): Promise<FindTextUnifiedResponse> {
	const query = new URLSearchParams();
	query.set('needle', params.needle);
	if (params.source !== undefined) query.set('source', params.source);
	if (params.request_id !== undefined) query.set('request_id', String(params.request_id));
	if (params.session_id !== undefined) query.set('session_id', params.session_id);
	if (params.case_sensitive !== undefined) {
		query.set('case_sensitive', String(params.case_sensitive));
	}
	if (params.context_bytes !== undefined) query.set('context_bytes', String(params.context_bytes));
	if (params.max_matches !== undefined) query.set('max_matches', String(params.max_matches));
	if (params.date_from !== undefined) query.set('date_from', params.date_from);
	if (params.date_to !== undefined) query.set('date_to', params.date_to);
	return rawGetJson<FindTextUnifiedResponse>(`/api/body/find?${query.toString()}`);
}

export interface ContentSearchParams {
	q: string;
	project?: string;
	limit?: number;
	offset?: number;
}

export function searchContent(params: ContentSearchParams): Promise<ContentSearchResponse> {
	const query = new URLSearchParams();
	query.set('q', params.q);
	if (params.project !== undefined) query.set('project', params.project);
	if (params.limit !== undefined) query.set('limit', String(params.limit));
	if (params.offset !== undefined) query.set('offset', String(params.offset));
	return rawGetJson<ContentSearchResponse>(`/api/content/search?${query.toString()}`);
}

export interface ListSessionsParams {
	offset?: number;
	limit?: number;
	order?: SortOrder;
	search?: string;
	project?: string;
	ids?: string[];
	date_from?: string;
	date_to?: string;
}

export function listSessions(params: ListSessionsParams = {}): Promise<SessionSummary[]> {
	const query = new URLSearchParams();
	if (params.offset !== undefined) {
		query.set('offset', String(params.offset));
	}
	if (params.limit !== undefined) {
		query.set('limit', String(params.limit));
	}
	if (params.order !== undefined) {
		query.set('order', params.order);
	}
	if (params.search !== undefined) {
		const trimmed = params.search.trim();
		if (trimmed.length > 0) {
			query.set('search', trimmed);
		}
	}
	if (params.project !== undefined) {
		const trimmed = params.project.trim();
		if (trimmed.length > 0) {
			query.set('project', trimmed);
		}
	}
	if (params.ids !== undefined && params.ids.length > 0) {
		query.set('ids', params.ids.join(','));
	}
	if (params.date_from !== undefined) {
		query.set('date_from', params.date_from);
	}
	if (params.date_to !== undefined) {
		query.set('date_to', params.date_to);
	}
	const suffix = query.size > 0 ? `?${query.toString()}` : '';
	return getJson<SessionSummary[]>(`/api/sessions${suffix}`);
}

export function patchSessionPin(
	id: string,
	pinned: boolean
): Promise<{ session_id: string; pinned: boolean }> {
	return patchJson<{ pinned: boolean }, { session_id: string; pinned: boolean }>(
		`/api/sessions/${encodeURIComponent(id)}/pin`,
		{ pinned }
	).then((res) => {
		void invalidateApiCache('/api/sessions');
		return res;
	});
}

export function assignSessionProject(
	id: string,
	projectPath: string
): Promise<SessionProjectAssignmentResponse> {
	return patchJson<{ project_path: string }, SessionProjectAssignmentResponse>(
		`/api/sessions/${encodeURIComponent(id)}/project`,
		{ project_path: projectPath }
	).then((res) => {
		void invalidateApiCache('/api/sessions');
		void invalidateApiCache('/api/projects');
		return res;
	});
}

export function patchProjectPin(
	path: string,
	pinned: boolean
): Promise<{ project_path: string; pinned: boolean }> {
	return patchJson<{ pinned: boolean }, { project_path: string; pinned: boolean }>(
		`/api/projects/${encodeURIComponent(path)}/pin`,
		{ pinned }
	).then((res) => {
		void invalidateApiCache('/api/sessions');
		return res;
	});
}

export function listProjects(opts: { counts?: boolean } = {}): Promise<ProjectSummaryDto[]> {
	return getJson<ProjectSummaryDto[]>(opts.counts ? '/api/projects?counts=true' : '/api/projects');
}

export interface TimeSpentParams {
	days?: number;
	project?: string;
	project_path?: string;
	from?: string;
	to?: string;
	session_id?: string;
	include_subagents?: boolean;
	kind?: string;
	limit?: number;
	offset?: number;
}

export function getTimeSpent(params: TimeSpentParams = {}): Promise<TimeSpentDto> {
	const query = new URLSearchParams();
	if (params.days !== undefined) query.set('days', String(params.days));
	if (params.project !== undefined && params.project.trim().length > 0) {
		query.set('project', params.project.trim());
	}
	if (params.project_path !== undefined && params.project_path.length > 0) {
		query.set('project_path', params.project_path);
	}
	if (params.from !== undefined) query.set('from', params.from);
	if (params.to !== undefined) query.set('to', params.to);
	if (params.session_id !== undefined) query.set('session_id', params.session_id);
	if (params.include_subagents === true) query.set('include_subagents', 'true');
	if (params.kind !== undefined && params.kind.length > 0) query.set('kind', params.kind);
	if (params.limit !== undefined) query.set('limit', String(params.limit));
	if (params.offset !== undefined && params.offset > 0) query.set('offset', String(params.offset));
	const suffix = query.size > 0 ? `?${query.toString()}` : '';
	return rawGetJson<TimeSpentDto>(`/api/time_spent${suffix}`);
}

export function patchProjectName(
	path: string,
	customName: string | null
): Promise<{ project_path: string; custom_name: string | null }> {
	return patchJson<
		{ custom_name: string | null },
		{ project_path: string; custom_name: string | null }
	>(`/api/projects/${encodeURIComponent(path)}/name`, { custom_name: customName }).then((res) => {
		void invalidateApiCache('/api/sessions');
		return res;
	});
}

export interface ProjectMergeResponse {
	source_path: string;
	target_uuid: string;
	target_path: string;
	sessions_moved: number;
}

export function mergeProjectInto(
	path: string,
	intoProjectUuid: string
): Promise<ProjectMergeResponse> {
	return postJson<{ into_project_uuid: string }, ProjectMergeResponse>(
		`/api/projects/${encodeURIComponent(path)}/merge`,
		{ into_project_uuid: intoProjectUuid }
	).then((res) => {
		void invalidateApiCache('/api/sessions');
		void invalidateApiCache('/api/projects');
		return res;
	});
}

export interface CreatedProject {
	id: number;
	project_uuid: string;
	project_path: string;
}

export function createProject(name: string): Promise<CreatedProject> {
	return postJson<{ name: string }, CreatedProject>('/api/knowledge/projects', { name }).then(
		(res) => {
			void invalidateApiCache('/api/projects');
			return res;
		}
	);
}

export interface GetSessionParams {
	offset?: number;
	limit?: number;
	order?: SortOrder;
}

export function getSession(id: string, params: GetSessionParams = {}): Promise<SessionDetail> {
	const query = new URLSearchParams();
	if (params.offset !== undefined) {
		query.set('offset', String(params.offset));
	}
	if (params.limit !== undefined) {
		query.set('limit', String(params.limit));
	}
	if (params.order !== undefined) {
		query.set('order', params.order);
	}
	const suffix = query.size > 0 ? `?${query.toString()}` : '';
	return getJson<SessionDetail>(`/api/sessions/${encodeURIComponent(id)}${suffix}`);
}

export function getSessionHead(id: string): Promise<SessionHead> {
	return getJson<SessionHead>(`/api/sessions/${encodeURIComponent(id)}/head`);
}

export interface SessionValidationStatusDto {
	uuid: string;
	operator_turn_id: number;
	status: 'dispatched' | 'resolved_pass' | 'resolved_correction';
	intervention_uuid: string | null;
	dispatched_at: string;
	resolved_at: string | null;
}

export function getSessionValidationStatus(id: string): Promise<SessionValidationStatusDto> {
	return getJson<SessionValidationStatusDto>(
		`/api/sessions/${encodeURIComponent(id)}/validation-status`
	);
}

export interface ListSessionToolUsesParams {
	offset?: number;
	limit?: number;
	order?: SortOrder;
}

export function listSessionToolUses(
	id: string,
	params: ListSessionToolUsesParams = {}
): Promise<ToolUsesResponse> {
	const query = new URLSearchParams();
	if (params.offset !== undefined) {
		query.set('offset', String(params.offset));
	}
	if (params.limit !== undefined) {
		query.set('limit', String(params.limit));
	}
	if (params.order !== undefined) {
		query.set('order', params.order);
	}
	const suffix = query.size > 0 ? `?${query.toString()}` : '';
	return getJson<ToolUsesResponse>(`/api/sessions/${encodeURIComponent(id)}/tool_uses${suffix}`);
}

export interface ListSessionOperatorTurnsParams {
	order?: SortOrder;
	promptKind?: string;
}

export function listSessionOperatorTurns(
	id: string,
	params: ListSessionOperatorTurnsParams = {}
): Promise<OperatorTurnDto[]> {
	const query = new URLSearchParams();
	if (params.order !== undefined) {
		query.set('order', params.order);
	}
	if (params.promptKind !== undefined) {
		query.set('prompt_kind', params.promptKind);
	}
	const suffix = query.size > 0 ? `?${query.toString()}` : '';
	return getJson<OperatorTurnDto[]>(
		`/api/sessions/${encodeURIComponent(id)}/operator-turns${suffix}`
	);
}

export interface PairedToolCall {
	tool_id: string;
	name: string;
	input_bytes: number;
	ord: number;
	request_id: number;
	has_result: boolean;
	result_is_error: boolean | null;
	result_content_bytes: number | null;
	operation: string | null;
}

export interface TurnToolCallsResponse {
	turn_id: number;
	total_count: number;
	calls: PairedToolCall[];
}

export interface TurnToolCallContent {
	tool_id: string;
	input: unknown;
	result: {
		tool_use_id: string;
		is_error: boolean;
		content_sha256: string;
		content_bytes: number;
		content: unknown;
	} | null;
}

export function listTurnToolCalls(
	sessionId: string,
	turnId: number
): Promise<TurnToolCallsResponse> {
	return rawGetJson<TurnToolCallsResponse>(
		`/api/sessions/${encodeURIComponent(sessionId)}/turns/${turnId}/tool-calls`
	);
}

export function getTurnToolCallContent(
	sessionId: string,
	turnId: number,
	toolId: string
): Promise<TurnToolCallContent> {
	return rawGetJson<TurnToolCallContent>(
		`/api/sessions/${encodeURIComponent(sessionId)}/turns/${turnId}/tool-calls/${encodeURIComponent(toolId)}/content`
	);
}

export interface OperatorTurnsListParams {
	project?: string;
	session?: string;
	since?: string;
	outcome?: 'unknown' | 'accepted' | 'corrected' | 'abandoned' | 'shipped';
	offset?: number;
	limit?: number;
	order?: SortOrder;
}

export function listOperatorTurns(
	params: OperatorTurnsListParams = {}
): Promise<OperatorTurnDto[]> {
	const query = new URLSearchParams();
	if (params.project !== undefined) query.set('project', params.project);
	if (params.session !== undefined) query.set('session', params.session);
	if (params.since !== undefined) query.set('since', params.since);
	if (params.outcome !== undefined) query.set('outcome', params.outcome);
	if (params.offset !== undefined) query.set('offset', String(params.offset));
	if (params.limit !== undefined) query.set('limit', String(params.limit));
	if (params.order !== undefined) query.set('order', params.order);
	const suffix = query.size > 0 ? `?${query.toString()}` : '';
	return getJson<OperatorTurnDto[]>(`/api/operator-turns${suffix}`);
}

export function getOperatorTurnsOutcomeSummary(
	params: { project?: string; since?: string } = {}
): Promise<OperatorTurnOutcomeSummary> {
	const query = new URLSearchParams();
	if (params.project !== undefined) query.set('project', params.project);
	if (params.since !== undefined) query.set('since', params.since);
	const suffix = query.size > 0 ? `?${query.toString()}` : '';
	return getJson<OperatorTurnOutcomeSummary>(`/api/operator-turns/outcome-summary${suffix}`);
}

export function getRecallProjectContext(
	projectPath: string,
	queryText?: string
): Promise<RecallProjectContextDto> {
	const query = new URLSearchParams({ project: projectPath });
	if (queryText?.trim()) query.set('query', queryText.trim());
	return getJson<RecallProjectContextDto>(`/api/recall/project-context?${query.toString()}`);
}

export function searchProjectKnowledge(
	project: string,
	params: {
		query?: string;
		kind?: string;
		includeHistorical?: boolean;
		offset?: number;
		limit?: number;
	} = {}
): Promise<KnowledgeSearchResponse> {
	const query = new URLSearchParams({ project });
	if (params.query?.trim()) query.set('query', params.query.trim());
	if (params.kind) query.set('kind', params.kind);
	if (params.includeHistorical) query.set('include_historical', 'true');
	if (params.offset !== undefined) query.set('offset', String(params.offset));
	if (params.limit !== undefined) query.set('limit', String(params.limit));
	return getJson<KnowledgeSearchResponse>(`/api/knowledge/search?${query.toString()}`);
}

export function readProjectKnowledge(
	project: string,
	reference: string
): Promise<KnowledgeReadResponse> {
	const query = new URLSearchParams({ project, reference });
	return getJson<KnowledgeReadResponse>(`/api/knowledge/read?${query.toString()}`);
}

export interface ToolErrorsListParams {
	project?: string;
	tool_name?: string;
	since?: string;
	offset?: number;
	limit?: number;
	order?: SortOrder;
	cursor?: string;
}

export function listToolErrors(params: ToolErrorsListParams = {}): Promise<ToolErrorsResponse> {
	const query = new URLSearchParams();
	if (params.project !== undefined) query.set('project', params.project);
	if (params.tool_name !== undefined) query.set('tool_name', params.tool_name);
	if (params.since !== undefined) query.set('since', params.since);
	if (params.offset !== undefined) query.set('offset', String(params.offset));
	if (params.limit !== undefined) query.set('limit', String(params.limit));
	if (params.order !== undefined) query.set('order', params.order);
	if (params.cursor !== undefined) query.set('cursor', params.cursor);
	const suffix = query.size > 0 ? `?${query.toString()}` : '';
	return getJson<ToolErrorsResponse>(`/api/tool-results/errors${suffix}`);
}

export function listToolErrorFacets(): Promise<ToolErrorFacets> {
	return getJson<ToolErrorFacets>('/api/tool-results/errors/facets');
}

export interface AgentDispatchesListParams {
	project?: string;
	since?: string;
	offset?: number;
	limit?: number;
	order?: SortOrder;
}

export function listAgentDispatches(
	params: AgentDispatchesListParams = {}
): Promise<AgentDispatchDto[]> {
	const query = new URLSearchParams();
	if (params.project !== undefined) query.set('project', params.project);
	if (params.since !== undefined) query.set('since', params.since);
	if (params.offset !== undefined) query.set('offset', String(params.offset));
	if (params.limit !== undefined) query.set('limit', String(params.limit));
	if (params.order !== undefined) query.set('order', params.order);
	const suffix = query.size > 0 ? `?${query.toString()}` : '';
	return getJson<AgentDispatchDto[]>(`/api/agent-dispatches${suffix}`);
}

export function listSessionWorkflowRuns(sessionId: string): Promise<WorkflowRunDto[]> {
	return getJson<WorkflowRunDto[]>(`/api/sessions/${encodeURIComponent(sessionId)}/workflow-runs`);
}

export function getDiagnosticsStats(): Promise<DiagnosticStats> {
	return getJson<DiagnosticStats>('/api/diagnostics/stats');
}

export interface ListFindingsParams {
	category?: DiagnosticCategory;
	analyzer?: string;
	session_id?: string;
	offset?: number;
	limit?: number;
	order?: SortOrder;
}

export interface PaginatedFindings {
	findings: Finding[];
	total: number;
	offset: number;
	limit: number;
}

export function listFindings(params: ListFindingsParams = {}): Promise<Finding[]> {
	const query = buildFindingsQuery(params);
	const suffix = query.size > 0 ? `?${query.toString()}` : '';
	return getJson<Finding[]>(`/api/diagnostics/findings${suffix}`);
}

export function listFindingsPaginated(params: ListFindingsParams = {}): Promise<PaginatedFindings> {
	const query = buildFindingsQuery(params);
	query.set('total', 'true');
	return getJson<PaginatedFindings>(`/api/diagnostics/findings?${query.toString()}`);
}

function buildFindingsQuery(params: ListFindingsParams): URLSearchParams {
	const query = new URLSearchParams();
	if (params.category !== undefined) {
		query.set('category', params.category);
	}
	if (params.analyzer !== undefined) {
		query.set('analyzer', params.analyzer);
	}
	if (params.session_id !== undefined) {
		query.set('session_id', params.session_id);
	}
	if (params.offset !== undefined) {
		query.set('offset', String(params.offset));
	}
	if (params.limit !== undefined) {
		query.set('limit', String(params.limit));
	}
	if (params.order !== undefined) {
		query.set('order', params.order);
	}
	return query;
}

const FILTER_RULES_PATH = '/api/filter/rules';

export function listFilterRules(): Promise<FilterRuleDto[]> {
	// Deliberately NOT cachedGet: /api/filter/rules is edited from the very page that reads it
	// (create/update/delete DO invalidate the cache below), but it can also change out of band —
	// a migration backfill, or an MCP `update_filter_rule` call — with no invalidation hook of
	// its own. The workbench's SWR/IndexedDB copy then serves a stale `title` (or a stale
	// anything else) for up to five minutes in-tab, or indefinitely across a reload, since
	// `revalidatedAt` resets on reload while the IndexedDB entry does not. Rules are a short
	// list; an always-fresh fetch is cheap, and this is the one place where staleness reads as a
	// visible product bug (a saved title silently replaced by the derived fallback).
	return rawGetJson<FilterRuleDto[]>(FILTER_RULES_PATH);
}

export function createFilterRule(body: FilterRuleCreate): Promise<FilterRuleMutationEnvelope> {
	return postJson<FilterRuleCreate, FilterRuleMutationEnvelope>(FILTER_RULES_PATH, body).then(
		(res) => {
			void invalidateApiCache(FILTER_RULES_PATH);
			return res;
		}
	);
}

export function patchFilterRule(
	id: string,
	patch: FilterRulePatch
): Promise<FilterRuleMutationEnvelope> {
	return patchJson<FilterRulePatch, FilterRuleMutationEnvelope>(
		`${FILTER_RULES_PATH}/${encodeURIComponent(id)}`,
		patch
	).then((res) => {
		void invalidateApiCache(FILTER_RULES_PATH);
		return res;
	});
}

export async function deleteFilterRule(id: string): Promise<FilterRuleDeleteResponse> {
	const path = `${FILTER_RULES_PATH}/${encodeURIComponent(id)}`;
	const res = await fetch(path, {
		method: 'DELETE',
		headers: { accept: 'application/json' }
	});
	if (res.status === 204) {
		void invalidateApiCache(FILTER_RULES_PATH);
		return { warnings: [] };
	}
	if (!res.ok) {
		let message = `DELETE ${path} failed: ${res.status} ${res.statusText}`;
		try {
			const errBody = (await res.json()) as { error?: string };
			if (errBody && typeof errBody.error === 'string' && errBody.error.length > 0) {
				message = errBody.error;
			}
		} catch {
			// Keep the HTTP status fallback when the response body is not valid JSON.
		}
		throw new ApiError(message, res.status, path);
	}
	try {
		return (await res.json()) as FilterRuleDeleteResponse;
	} catch {
		return { warnings: [] };
	}
}

export function dryRunFilterRules(body: FilterDryRunRequest): Promise<FilterDryRunResponse> {
	return postJson<FilterDryRunRequest, FilterDryRunResponse>(`${FILTER_RULES_PATH}/dry-run`, body);
}

const PROMPT_COMPOSITIONS_PATH = '/api/prompts/compositions';

export function listPromptCompositions(): Promise<PromptCompositionListItem[]> {
	return getJson<PromptCompositionListItem[]>(PROMPT_COMPOSITIONS_PATH);
}

export interface ListPromptClassesParams {
	date_from?: string;
	date_to?: string;
	limit?: number;
	offset?: number;
	/** Only classes observed while the request's project pointed at this path. */
	project_path?: string;
}

export function listPromptClasses(params: ListPromptClassesParams = {}): Promise<PromptClassDto[]> {
	const query = new URLSearchParams();
	if (params.date_from !== undefined) query.set('date_from', params.date_from);
	if (params.date_to !== undefined) query.set('date_to', params.date_to);
	if (params.limit !== undefined) query.set('limit', String(params.limit));
	if (params.offset !== undefined) query.set('offset', String(params.offset));
	if (params.project_path !== undefined) query.set('project_path', params.project_path);
	const suffix = query.size > 0 ? `?${query.toString()}` : '';
	return getJson<PromptClassDto[]>(`/api/prompts/classes${suffix}`);
}

export const UNRESOLVED_PERSONA_SEGMENT = 'unresolved';

export interface GetPromptClassObservationsParams {
	limit?: number;
	offset?: number;
	model?: string;
	/** Only observations captured while the request's project pointed at this path. */
	project_path?: string;
}

function appendPromptClassObservationsParams(
	query: URLSearchParams,
	params: GetPromptClassObservationsParams
): void {
	if (params.limit !== undefined) query.set('limit', String(params.limit));
	if (params.offset !== undefined) query.set('offset', String(params.offset));
	if (params.model !== undefined && params.model.length > 0) query.set('model', params.model);
	if (params.project_path !== undefined && params.project_path.length > 0) {
		query.set('project_path', params.project_path);
	}
}

export function getPromptClassMain(
	params: GetPromptClassObservationsParams = {}
): Promise<PromptClassDetailDto> {
	const query = new URLSearchParams();
	appendPromptClassObservationsParams(query, params);
	const suffix = query.size > 0 ? `?${query.toString()}` : '';
	return getJson<PromptClassDetailDto>(`/api/prompts/classes/main${suffix}`);
}

export function getPromptClassSubagent(
	persona: string,
	params: GetPromptClassObservationsParams = {}
): Promise<PromptClassDetailDto> {
	const query = new URLSearchParams();
	appendPromptClassObservationsParams(query, params);
	const suffix = query.size > 0 ? `?${query.toString()}` : '';
	return getJson<PromptClassDetailDto>(
		`/api/prompts/classes/subagent/${encodeURIComponent(persona)}${suffix}`
	);
}

export function getPromptClassVersions(
	lane: 'main' | 'subagent',
	persona: string | null
): Promise<PromptClassVersionsDto> {
	if (lane === 'main') {
		return getJson<PromptClassVersionsDto>('/api/prompts/classes/main/versions');
	}
	const segment = persona !== null && persona.length > 0 ? persona : UNRESOLVED_PERSONA_SEGMENT;
	return getJson<PromptClassVersionsDto>(
		`/api/prompts/classes/subagent/${encodeURIComponent(segment)}/versions`
	);
}

/** The distinct agent instructions observed under one subagent class, most used first. */
export function getPromptClassInstructions(
	persona: string | null
): Promise<PromptClassInstructionsListDto> {
	const segment = persona !== null && persona.length > 0 ? persona : UNRESOLVED_PERSONA_SEGMENT;
	return getJson<PromptClassInstructionsListDto>(
		`/api/prompts/classes/subagent/${encodeURIComponent(segment)}/instructions`
	);
}

/** Asks mike's configured LLM for a short agent name for these instructions. */
export function suggestPromptName(instructions: string): Promise<SuggestPromptNameDto> {
	return postJson<{ instructions: string }, SuggestPromptNameDto>('/api/prompts/suggest-name', {
		instructions
	});
}

export function getPromptCompositionBySha256(promptSha256: string): Promise<PromptComposition> {
	return getJson<PromptComposition>(
		`${PROMPT_COMPOSITIONS_PATH}/by-sha256/${encodeURIComponent(promptSha256)}`
	);
}

export function createPromptComposition(
	body: PromptCompositionCreate
): Promise<PromptCompositionMutationEnvelope> {
	return postJson<PromptCompositionCreate, PromptCompositionMutationEnvelope>(
		PROMPT_COMPOSITIONS_PATH,
		body
	);
}

export function patchPromptComposition(
	id: string,
	patch: PromptCompositionPatch
): Promise<PromptCompositionMutationEnvelope> {
	return patchJson<PromptCompositionPatch, PromptCompositionMutationEnvelope>(
		`${PROMPT_COMPOSITIONS_PATH}/${encodeURIComponent(id)}`,
		patch
	);
}

export async function deletePromptComposition(id: string): Promise<{ warnings: string[] }> {
	const path = `${PROMPT_COMPOSITIONS_PATH}/${encodeURIComponent(id)}`;
	const res = await fetch(path, {
		method: 'DELETE',
		headers: { accept: 'application/json' }
	});
	if (res.status === 204) {
		void invalidateApiCache(PROMPT_COMPOSITIONS_PATH);
		return { warnings: [] };
	}
	if (!res.ok) {
		let message = `DELETE ${path} failed: ${res.status} ${res.statusText}`;
		try {
			const errBody = (await res.json()) as { error?: string };
			if (errBody && typeof errBody.error === 'string' && errBody.error.length > 0) {
				message = errBody.error;
			}
		} catch {
			// Keep the HTTP status fallback when the response body is not valid JSON.
		}
		throw new ApiError(message, res.status, path);
	}
	try {
		return (await res.json()) as { warnings: string[] };
	} catch {
		return { warnings: [] };
	}
}

export interface ListPromptAnomaliesParams {
	date_from?: string;
	date_to?: string;
	limit?: number;
	offset?: number;
}

export function listPromptAnomalies(
	params: ListPromptAnomaliesParams = {}
): Promise<PromptAnomalyGroup[]> {
	const query = new URLSearchParams();
	if (params.date_from !== undefined) query.set('date_from', params.date_from);
	if (params.date_to !== undefined) query.set('date_to', params.date_to);
	if (params.limit !== undefined) query.set('limit', String(params.limit));
	if (params.offset !== undefined) query.set('offset', String(params.offset));
	const suffix = query.size > 0 ? `?${query.toString()}` : '';
	return getJson<PromptAnomalyGroup[]>(`/api/prompts/anomalies${suffix}`);
}

export function listAgents(): Promise<AgentListItem[]> {
	return getJson<AgentListItem[]>('/api/agents');
}

export function getAgent(id: string): Promise<AgentDetail> {
	return getJson<AgentDetail>(`/api/agents/${encodeURIComponent(id)}`);
}

export function getCompositionStats(promptSha256: string): Promise<CompositionUsageStats> {
	return getJson<CompositionUsageStats>(
		`/api/prompts/compositions/by-sha256/${encodeURIComponent(promptSha256)}/stats`
	);
}

export async function patchAgent(id: string, patch: AgentPatch): Promise<Agent> {
	const res = await fetch(`/api/agents/${encodeURIComponent(id)}`, {
		method: 'PATCH',
		headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
		body: JSON.stringify(patch)
	});
	if (!res.ok) {
		const text = await res.text().catch(() => '');
		throw new ApiError(
			text || `PATCH /api/agents/${id} failed: ${res.status}`,
			res.status,
			res.url
		);
	}
	return res.json();
}

export interface ListToolUsesParams {
	tool_name?: string;
	project?: string;
	session_id?: string;
	since?: string;
	offset?: number;
	limit?: number;
	order?: SortOrder;
	errors_only?: boolean;
	cursor?: string;
}

export function listToolUses(params: ListToolUsesParams = {}): Promise<ToolUsesFeedResponse> {
	const query = new URLSearchParams();
	if (params.tool_name !== undefined) query.set('tool_name', params.tool_name);
	if (params.project !== undefined) query.set('project', params.project);
	if (params.session_id !== undefined) query.set('session_id', params.session_id);
	if (params.since !== undefined) query.set('since', params.since);
	if (params.offset !== undefined) query.set('offset', String(params.offset));
	if (params.limit !== undefined) query.set('limit', String(params.limit));
	if (params.order !== undefined) query.set('order', params.order);
	if (params.errors_only !== undefined) query.set('errors_only', String(params.errors_only));
	if (params.cursor !== undefined) query.set('cursor', params.cursor);
	const suffix = query.size > 0 ? `?${query.toString()}` : '';
	return getJson<ToolUsesFeedResponse>(`/api/tool-uses${suffix}`);
}

export function listToolUseFacets(): Promise<ToolUseFacets> {
	return getJson<ToolUseFacets>('/api/tool-uses/facets');
}

export async function deleteAgent(id: string): Promise<void> {
	const res = await fetch(`/api/agents/${encodeURIComponent(id)}`, {
		method: 'DELETE'
	});
	if (!res.ok && res.status !== 204) {
		const text = await res.text().catch(() => '');
		throw new ApiError(
			text || `DELETE /api/agents/${id} failed: ${res.status}`,
			res.status,
			res.url
		);
	}
}

export interface PromptCompositionExportEntry {
	prompt_sha256: string;
	label: string;
	blocks: Array<{
		text: string;
		cache_control?: { type: string; ttl?: string } | null;
	}>;
	mode: PromptCompositionMode;
	notes?: string | null;
}

export interface PromptCompositionExport {
	version: number;
	compositions: PromptCompositionExportEntry[];
}

export interface PromptCompositionImportRowResult {
	prompt_sha256: string;
	status: 'imported' | 'skipped' | 'error';
	reason: string | null;
}

export interface PromptCompositionImportResult {
	imported: number;
	skipped: number;
	errors: number;
	rows: PromptCompositionImportRowResult[];
}

export function exportPromptCompositions(): Promise<PromptCompositionExport> {
	return getJson<PromptCompositionExport>(`${PROMPT_COMPOSITIONS_PATH}/export`);
}

export function getCompositionRevisions(id: string): Promise<CompositionRevision[]> {
	return getJson<CompositionRevision[]>(
		`${PROMPT_COMPOSITIONS_PATH}/${encodeURIComponent(id)}/revisions`
	);
}

const VOLATILE_LINES_PATH = '/api/volatile-lines';

export function listVolatileLines(): Promise<VolatileLinePattern[]> {
	return getJson<VolatileLinePattern[]>(VOLATILE_LINES_PATH);
}

export function createVolatileLine(body: VolatileLinePatternCreate): Promise<VolatileLinePattern> {
	return postJson<VolatileLinePatternCreate, VolatileLinePattern>(VOLATILE_LINES_PATH, body);
}

export function patchVolatileLine(
	id: string,
	patch: Partial<VolatileLinePatternCreate>
): Promise<VolatileLinePattern> {
	return patchJson<Partial<VolatileLinePatternCreate>, VolatileLinePattern>(
		`${VOLATILE_LINES_PATH}/${encodeURIComponent(id)}`,
		patch
	);
}

export async function deleteVolatileLine(id: string): Promise<void> {
	const path = `${VOLATILE_LINES_PATH}/${encodeURIComponent(id)}`;
	const res = await fetch(path, {
		method: 'DELETE',
		headers: { accept: 'application/json' }
	});
	if (res.status === 204) return;
	if (!res.ok) {
		let message = `DELETE ${path} failed: ${res.status} ${res.statusText}`;
		try {
			const errBody = (await res.json()) as { error?: string };
			if (errBody && typeof errBody.error === 'string' && errBody.error.length > 0) {
				message = errBody.error;
			}
		} catch {
			// Keep the HTTP status fallback when the response body is not valid JSON.
		}
		throw new ApiError(message, res.status, path);
	}
}

export async function importPromptCompositions(
	payload: PromptCompositionExport
): Promise<PromptCompositionImportResult> {
	const path = `${PROMPT_COMPOSITIONS_PATH}/import`;
	const res = await fetch(path, {
		method: 'POST',
		headers: {
			'content-type': 'application/json',
			accept: 'application/json'
		},
		body: JSON.stringify(payload)
	});
	if (!res.ok) {
		let message = `POST ${path} failed: ${res.status} ${res.statusText}`;
		try {
			const errBody = (await res.json()) as { error?: string };
			if (errBody && typeof errBody.error === 'string' && errBody.error.length > 0) {
				message = errBody.error;
			}
		} catch {
			// Keep the HTTP status fallback when the response body is not valid JSON.
		}
		throw new ApiError(message, res.status, path);
	}
	return (await res.json()) as PromptCompositionImportResult;
}
