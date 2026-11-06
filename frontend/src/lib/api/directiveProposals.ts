import { ApiError } from '$lib/api';

export interface DirectiveProposal {
	id: number;
	uuid: string;
	source: 'repeated_directive' | 'confirmed_handback' | string;
	scope: 'project' | 'no_project' | 'operator' | string;
	project_id: number | null;
	class: string;
	polarity: string;
	status: string;
	proposed_kind: string;
	proposed_text: string;
	evidence_count: number;
	session_count: number;
	first_evidence_at: string;
	last_evidence_at: string;
	created_at: string;
	decided_at: string | null;
	decision_note: string | null;
}

export interface ProposalEvidence {
	kind: string;
	ref_id: number;
	operator_turn_id: number | null;
	session_id: number | null;
	observed_at: string;
	detail: Record<string, unknown>;
	message_text: string | null;
	gone: boolean;
}

export interface ProposalDetail {
	proposal: DirectiveProposal;
	evidence: ProposalEvidence[];
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

export function listProposals(status = 'open'): Promise<{ proposals: DirectiveProposal[]; next_cursor: number | null }> {
	return send('GET', `/api/directive-proposals?status=${status}`);
}

export function getProposal(id: number): Promise<ProposalDetail> {
	return send('GET', `/api/directive-proposals/${id}`);
}

export function rejectProposal(id: number, note: string): Promise<unknown> {
	return send('POST', `/api/directive-proposals/${id}/reject`, { note });
}

export interface ApproveProposalBody {
	text: string;
	kind: string;
	target_project_ids: number[];
	expires_at?: string | null;
}

export function approveProposal(id: number, body: ApproveProposalBody): Promise<unknown> {
	return send('POST', `/api/directive-proposals/${id}/approve`, body);
}

export function sourceLabel(source: string): string {
	if (source === 'repeated_directive') return 'You said this more than once';
	if (source === 'confirmed_handback') return 'You confirmed this was already authorised';
	return source.replaceAll('_', ' ');
}
