import { ApiError } from '$lib/api';

export type EmbeddingSource = 'overview' | 'centroid';

export interface EmbeddingStatusPresent {
	present: true;
	source: EmbeddingSource;
	model: string;
	embedded_at: string;
	chunk_count: number;
}

export type EmbeddingAbsentReason = 'no_embedded_turns' | 'embedding_disabled';

export interface EmbeddingStatusAbsent {
	present: false;
	reason: EmbeddingAbsentReason;
}

export type EmbeddingStatus = EmbeddingStatusPresent | EmbeddingStatusAbsent;

export interface GraphEdge {
	from: string;
	to: string;
	similarity: number;
}

export interface SessionGraphNode {
	session_id: string;
	title: string;
	project_id: number;
	project_path: string;
	outcome: string | null;
	last_seen: string;
	last_status?: string;
	similarity: number;
	source: EmbeddingSource;
	topics: string[];
	is_centre: boolean;
}

export interface SessionEmbeddingGraph {
	session_id: string;
	embedding: EmbeddingStatus;
	nodes: SessionGraphNode[];
	edges: GraphEdge[];
}

export interface ProjectCentroidStatus {
	present: boolean;
	session_count: number;
	embedded_at: string | null;
}

export interface ProjectGraphNode {
	session_id: string;
	title: string;
	outcome: string | null;
	last_seen: string;
	last_status?: string;
	similarity_to_centroid: number;
	source: EmbeddingSource;
	topics: string[];
}

export interface SiblingProject {
	project_id: number;
	project_path: string;
	similarity: number;
}

export interface ProjectEmbeddingGraph {
	project_id: number;
	project_path: string;
	centroid: ProjectCentroidStatus;
	nodes: ProjectGraphNode[];
	edges: GraphEdge[];
	sibling_projects: SiblingProject[];
}

export interface GetSessionEmbeddingGraphParams {
	k?: number;
	scope?: 'project' | 'all';
	minSimilarity?: number;
}

export interface GetProjectEmbeddingGraphParams {
	limit?: number;
	minSimilarity?: number;
}

async function fetchGraphJson<T>(path: string): Promise<T> {
	const res = await fetch(path, { headers: { accept: 'application/json' } });
	if (!res.ok) {
		let message = `GET ${path} failed: ${res.status} ${res.statusText}`;
		try {
			const errBody = (await res.json()) as { error?: string };
			if (errBody && typeof errBody.error === 'string' && errBody.error.length > 0) {
				message = errBody.error;
			}
		} catch {}
		throw new ApiError(message, res.status, path);
	}
	return (await res.json()) as T;
}

export function getSessionEmbeddingGraph(
	sessionId: string,
	params: GetSessionEmbeddingGraphParams = {}
): Promise<SessionEmbeddingGraph> {
	const query = new URLSearchParams();
	if (params.k !== undefined) query.set('k', String(params.k));
	if (params.scope !== undefined) query.set('scope', params.scope);
	if (params.minSimilarity !== undefined) query.set('min_similarity', String(params.minSimilarity));
	const suffix = query.size > 0 ? `?${query.toString()}` : '';
	return fetchGraphJson<SessionEmbeddingGraph>(
		`/api/sessions/${encodeURIComponent(sessionId)}/embedding-graph${suffix}`
	);
}

export function getProjectEmbeddingGraph(
	projectId: number,
	params: GetProjectEmbeddingGraphParams = {}
): Promise<ProjectEmbeddingGraph> {
	const query = new URLSearchParams();
	if (params.limit !== undefined) query.set('limit', String(params.limit));
	if (params.minSimilarity !== undefined) query.set('min_similarity', String(params.minSimilarity));
	const suffix = query.size > 0 ? `?${query.toString()}` : '';
	return fetchGraphJson<ProjectEmbeddingGraph>(
		`/api/projects/${encodeURIComponent(String(projectId))}/embedding-graph${suffix}`
	);
}
