import type {
	ContentSearchExcerpt,
	FilterDryRunResponse,
	FilterRuleDto,
	ProjectSummary,
	PromptClassDto,
	RequestDetail
} from './types.js';
import {
	SERVICE_TOKEN_HEADER,
	fileServiceTokenReader,
	type ServiceTokenReader
} from './serviceToken.js';

interface ContentSearchApiSession {
	session_id: string;
	project_path: string;
	matches: Array<{
		request_id: number;
		before: string;
		matched: string;
		after: string;
	}>;
}

interface ContentSearchApiResponse {
	sessions: ContentSearchApiSession[];
}

export type FetchFn = typeof fetch;

export interface SettingMeta {
	key: string;
	kind: string;
	value: boolean | number | string | string[];
	[extra: string]: unknown;
}

export interface FeatureDto {
	key: string;
	enabled: boolean;
	[extra: string]: unknown;
}

export interface AnthropicCredentials {
	mode: 'claude_code_login' | 'api_key';
	api_key: string | null;
}

export class InspectApiError extends Error {
	constructor(
		message: string,
		readonly status: number,
		readonly url: string
	) {
		super(message);
		this.name = 'InspectApiError';
	}
}

/**
 * Thin client over mike's inspect REST API (`GET /api/*`). Every method takes
 * no hidden state beyond the base URL, so tests can construct one against a
 * `fetchImpl` stub instead of a live proxy.
 */
export class InspectClient {
	constructor(
		private readonly baseUrl: string,
		private readonly fetchImpl: FetchFn = fetch,
		private readonly readServiceToken: ServiceTokenReader = fileServiceTokenReader(baseUrl)
	) {}

	private async getJson<T>(
		pathAndQuery: string,
		extraHeaders: Record<string, string> = {}
	): Promise<T> {
		const url = `${this.baseUrl}${pathAndQuery}`;
		const res = await this.fetchImpl(url, {
			headers: { accept: 'application/json', ...extraHeaders },
			redirect: Object.hasOwn(extraHeaders, SERVICE_TOKEN_HEADER) ? 'error' : 'follow'
		});
		if (!res.ok) {
			throw new InspectApiError(`GET ${pathAndQuery} failed: ${res.status}`, res.status, url);
		}
		return (await res.json()) as T;
	}

	private async sendJson<T>(pathAndQuery: string, method: string, body: unknown): Promise<T> {
		const url = `${this.baseUrl}${pathAndQuery}`;
		const res = await this.fetchImpl(url, {
			method,
			headers: {
				accept: 'application/json',
				'content-type': 'application/json'
			},
			body: JSON.stringify(body)
		});
		if (!res.ok) {
			let message = `${method} ${pathAndQuery} failed: ${res.status}`;
			try {
				const errBody = (await res.json()) as { error?: string };
				if (typeof errBody.error === 'string' && errBody.error.length > 0) {
					message = errBody.error;
				}
			} catch {
				// non-JSON error body — keep the generic message
			}
			throw new InspectApiError(message, res.status, url);
		}
		return (await res.json()) as T;
	}

	getRequest(id: number): Promise<RequestDetail> {
		return this.getJson<RequestDetail>(`/api/requests/${id}`);
	}

	listFilterRules(): Promise<FilterRuleDto[]> {
		return this.getJson<FilterRuleDto[]>('/api/filter/rules');
	}

	dryRunFilterRules(requestId: number, rules: FilterRuleDto[]): Promise<FilterDryRunResponse> {
		return this.sendJson<FilterDryRunResponse>('/api/filter/rules/dry-run', 'POST', {
			request_id: requestId,
			rules
		});
	}

	createFilterRule(
		body: Record<string, unknown>
	): Promise<{ rule: FilterRuleDto; warnings: string[] }> {
		return this.sendJson('/api/filter/rules', 'POST', body);
	}

	updateFilterRule(
		id: string,
		patch: Record<string, unknown>
	): Promise<{ rule: FilterRuleDto; warnings: string[] }> {
		return this.sendJson(`/api/filter/rules/${encodeURIComponent(id)}`, 'PATCH', patch);
	}

	listPromptClasses(): Promise<PromptClassDto[]> {
		return this.getJson<PromptClassDto[]>('/api/prompts/classes');
	}

	listProjects(): Promise<ProjectSummary[]> {
		return this.getJson<ProjectSummary[]>('/api/projects');
	}

	/**
	 * Wraps `GET /api/content/search`, flattening its per-session match groups
	 * into one excerpt list scoped to a single project. `limit` bounds sessions
	 * returned by the API, not matches — each session already caps at 50
	 * matches server-side.
	 */
	async searchContentInProject(
		query: string,
		projectPath: string,
		limit = 20
	): Promise<ContentSearchExcerpt[]> {
		const params = new URLSearchParams({
			q: query,
			project: projectPath,
			limit: String(limit)
		});
		const body = await this.getJson<ContentSearchApiResponse>(
			`/api/content/search?${params.toString()}`
		);
		return body.sessions.flatMap((session) =>
			session.matches.map((m) => ({
				request_id: m.request_id,
				session_id: session.session_id,
				before: m.before,
				matched: m.matched,
				after: m.after
			}))
		);
	}

	async getSettings(): Promise<SettingMeta[]> {
		const body = await this.getJson<{ settings: SettingMeta[] }>('/api/settings');
		return body.settings ?? [];
	}

	async getFeatures(): Promise<FeatureDto[]> {
		const body = await this.getJson<FeatureDto[] | { features: FeatureDto[] }>(
			'/api/settings/features'
		);
		return Array.isArray(body) ? body : (body.features ?? []);
	}

	/**
	 * The Anthropic auth mode and, when `mode` is `'api_key'` and one is saved,
	 * the real unmasked key (`null` otherwise, including whenever `mode` is
	 * `'claude_code_login'`). Callers degrade this the same way as
	 * `getSettings()`/`getFeatures()`: this route may 404 on an older/not-yet-
	 * restarted mike, and the caller catches that and falls back to
	 * `{ mode: 'claude_code_login', api_key: null }`.
	 *
	 * mike refuses this route (401) without the service token it publishes
	 * owner-only in its state directory, so the token is read and sent here
	 * and on no other request.
	 */
	async getAnthropicCredentials(): Promise<AnthropicCredentials> {
		const token = await this.readServiceToken();
		return this.getJson<AnthropicCredentials>(
			'/api/settings/assistant/anthropic/credentials',
			token === null ? {} : { [SERVICE_TOKEN_HEADER]: token }
		);
	}
}
