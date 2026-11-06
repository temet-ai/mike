import { ApiError } from '$lib/api';

export interface ReconcileUsage {
	input_tokens: number;
	output_tokens: number;
	cache_read_input_tokens: number;
	cache_creation_input_tokens: number;
}

/** A Claude Code step (a model call in the transcript) that matched no captured request. */
export interface ReconcileStep {
	turn: string;
	index: number;
	model: string;
	agent: string | null;
	usage: ReconcileUsage | null;
}

/** A captured request that no Claude Code step explains. */
export interface ReconcileWireRequest {
	request_id: string;
	origin: string;
	model: string | null;
	usage: ReconcileUsage | null;
}

export interface ReconcileReport {
	session: string;
	continues: string | null;
	mod_steps: number;
	wire_requests: number;
	matched: number;
	only_in_mod: ReconcileStep[];
	only_on_wire: ReconcileWireRequest[];
	only_on_wire_by_origin: Record<string, number>;
}

export interface SessionReconcile {
	available: boolean;
	reason: string | null;
	report: ReconcileReport | null;
}

export async function getSessionReconcile(sessionId: string): Promise<SessionReconcile> {
	const path = `/api/sessions/${encodeURIComponent(sessionId)}/reconcile`;
	const res = await fetch(path, { headers: { accept: 'application/json' } });
	if (!res.ok) {
		throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	return (await res.json()) as SessionReconcile;
}
