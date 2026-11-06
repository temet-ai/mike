import type { SessionFlowActor } from '$lib/api/generated/SessionFlowActor';
import { IDENTITY_OF, type IdentityFamily, type NodeKind } from '$lib/board/types';

export type FlowRole =
	| 'coordinator'
	| 'agent'
	| 'peer'
	| 'sidecars'
	| 'local-tools'
	| 'mike-mcp'
	| `other-mcp:${string}`
	| `service:${string}`
	| 'upstream'
	| 'terminal';

const MCP_TOOL_PATTERN = /^mcp__([a-zA-Z0-9-]+)__/;

function slugify(label: string): string {
	const slug = label
		.trim()
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, '-')
		.replace(/^-+|-+$/g, '');
	return slug === '' ? 'unknown' : slug;
}

function toolServiceRole(actor: SessionFlowActor): FlowRole {
	if (actor.id.startsWith('mailbox:')) return 'service:mailbox';
	const match = MCP_TOOL_PATTERN.exec(actor.label);
	if (match) {
		const server = match[1]!.toLowerCase();
		return server === 'mike' ? 'mike-mcp' : `other-mcp:${server}`;
	}
	return 'local-tools';
}

export function roleOf(actor: SessionFlowActor, sessionId: string): FlowRole {
	switch (actor.kind) {
		case 'session':
			return actor.session_id === sessionId ? 'coordinator' : 'service:session';
		case 'peer':
			return 'peer';
		case 'sidecar':
			return 'sidecars';
		case 'worker':
		case 'unresolved_dispatch':
			return 'agent';
		case 'upstream':
			return 'upstream';
		case 'tool_service':
			return toolServiceRole(actor);
		case 'mike':
			return `service:${slugify(actor.label)}`;
		default:
			return 'service:unknown';
	}
}

export function rolesOf(
	actors: Iterable<SessionFlowActor>,
	sessionId: string
): ReadonlyMap<string, FlowRole> {
	const roles = new Map<string, FlowRole>();
	for (const actor of actors) roles.set(actor.id, roleOf(actor, sessionId));
	return roles;
}

export function nodeKindOf(role: FlowRole): NodeKind {
	if (role === 'coordinator') return 'session';
	if (role === 'peer') return 'peer';
	if (role === 'agent') return 'agent';
	if (role === 'sidecars') return 'group';
	if (role === 'local-tools' || role === 'mike-mcp' || role.startsWith('other-mcp:')) return 'tool';
	if (role.startsWith('service:')) return 'service';
	if (role === 'upstream') return 'outside';
	return 'terminal';
}

export function identityOf(role: FlowRole): IdentityFamily {
	return IDENTITY_OF[nodeKindOf(role)];
}
