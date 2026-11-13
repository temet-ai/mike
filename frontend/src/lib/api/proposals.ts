import { ApiError } from '$lib/api';

/**
 * Client for the Proposals page: lessons mike placed as a rule or an agent statement, which mike
 * never writes on its own, and the user's decision on each. Wire shapes follow
 * crates/mike-features/src/api/proposals.rs.
 */

export type ProposalPlacement = 'rule' | 'agent';
export type ProposalDecision = 'accepted' | 'declined';

export interface ProposalProject {
	project_uuid: string;
	project_path: string;
	display_name: string;
	proposals: number;
	undecided: number;
}

export interface ProposalProjectsResponse {
	projects: ProposalProject[];
	undecided: number;
}

export interface Proposal {
	lesson_id: number;
	placement: ProposalPlacement;
	kind: string;
	stage: string;
	text: string;
	topic: string | null;
	covered_by: string | null;
	reason: string | null;
	agent_name: string | null;
	user_prompt: string | null;
	created_at: string;
	placed_at: string | null;
	decision: ProposalDecision | null;
	decided_at: string | null;
}

export interface ProposalsResponse {
	project: string;
	proposals: Proposal[];
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

export function getProposalProjects(): Promise<ProposalProjectsResponse> {
	return send('GET', '/api/proposals/projects');
}

export function getProposals(projectUuid: string): Promise<ProposalsResponse> {
	return send('GET', `/api/proposals?project=${encodeURIComponent(projectUuid)}`);
}

export function decideProposal(
	lessonId: number,
	decision: 'accept' | 'decline' | 'clear'
): Promise<unknown> {
	return send('POST', `/api/proposals/${lessonId}/decision`, { decision });
}
