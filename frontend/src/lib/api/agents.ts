import { ApiError } from '$lib/api';
import { cachedGet, invalidateApiCache } from '$lib/stores/cache/apiCache.svelte';
import type { PromptComposition } from '$lib/types';

export type AgentSource = 'observed' | 'file_backed' | 'dynamic' | 'builtin';

export interface AgentAlias {
	alias: string;
	kind: 'persona' | 'name_variant';
}

export interface AgentCore {
	id: number;
	name: string;
	definition_path: string | null;
	description: string | null;
	source: AgentSource;
	created_at: string;
	updated_at: string;
}

export interface AgentListItem extends AgentCore {
	composition_count: number;
	dispatch_count: number;
	instance_count: number;
	prompt_version_count: number;
	last_dispatched_at: string | null;
	aliases: AgentAlias[];
}

export interface AgentDispatchRow {
	tool_use_id: number;
	dispatched_at: string;
	prompt: string;
	session_uuid: string;
	request_id: number;
}

export interface AgentDispatchesPage {
	total_count: number;
	rows: AgentDispatchRow[];
}

export interface AgentPromptVersion {
	content_sha256: string;
	first_seen_at: string;
	last_seen_at: string;
	request_count: number;
}

export interface AgentInstance {
	id: number;
	session_uuid: string;
	subagent_persona: string | null;
	started_at: string | null;
	first_request_id: number | null;
}

export interface AgentDetail extends AgentListItem {
	dispatches: AgentDispatchesPage;
	prompt_versions: AgentPromptVersion[];
	instances: AgentInstance[];
	compositions: PromptComposition[];
}

export type AgentPatch = Partial<{
	name: string;
	definition_path: string | null;
	description: string | null;
	source: AgentSource;
}>;

export interface GetAgentParams {
	dispatch_limit?: number;
	dispatch_offset?: number;
}

export async function listAgents(): Promise<AgentListItem[]> {
	const path = '/api/agents';
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as AgentListItem[];
	});
}

export async function getAgent(
	id: number | string,
	params: GetAgentParams = {}
): Promise<AgentDetail> {
	const query = new URLSearchParams();
	if (params.dispatch_limit !== undefined) {
		query.set('dispatch_limit', String(params.dispatch_limit));
	}
	if (params.dispatch_offset !== undefined) {
		query.set('dispatch_offset', String(params.dispatch_offset));
	}
	const suffix = query.size > 0 ? `?${query.toString()}` : '';
	const path = `/api/agents/${encodeURIComponent(id)}${suffix}`;
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as AgentDetail;
	});
}

export async function patchAgent(id: number, patch: AgentPatch): Promise<AgentCore> {
	const path = `/api/agents/${encodeURIComponent(id)}`;
	const res = await fetch(path, {
		method: 'PATCH',
		headers: {
			'content-type': 'application/json',
			accept: 'application/json'
		},
		body: JSON.stringify(patch)
	});
	if (!res.ok) {
		let message = `PATCH ${path} failed: ${res.status} ${res.statusText}`;
		try {
			const errBody = (await res.json()) as { error?: string };
			if (errBody && typeof errBody.error === 'string' && errBody.error.length > 0) {
				message = errBody.error;
			}
		} catch {}
		throw new ApiError(message, res.status, path);
	}
	void invalidateApiCache('/api/agents');
	return (await res.json()) as AgentCore;
}

export async function deleteAgent(id: number): Promise<void> {
	const path = `/api/agents/${encodeURIComponent(id)}`;
	const res = await fetch(path, { method: 'DELETE' });
	if (!res.ok && res.status !== 204) {
		let message = `DELETE ${path} failed: ${res.status} ${res.statusText}`;
		try {
			const errBody = (await res.json()) as { error?: string };
			if (errBody && typeof errBody.error === 'string' && errBody.error.length > 0) {
				message = errBody.error;
			}
		} catch {}
		throw new ApiError(message, res.status, path);
	}
	void invalidateApiCache('/api/agents');
}
