import { shortSessionId } from '$lib/format';
import { roleTokens, type RoleTokens } from '$lib/fleet/roleTokens';
import type { ParticipantIdentityDto, ThreadSummaryDto } from './types';

export function buildThreadIdentityIndex(
	thread: ThreadSummaryDto | null | undefined
): Map<string, ParticipantIdentityDto> {
	return new Map((thread?.participant_identities ?? []).map((p) => [p.session_uuid, p]));
}

export interface FallbackIdentity {
	role: string | null;
	agentType: string | null;
}

export function resolveParticipantLabel(
	uuid: string | null,
	index: Map<string, ParticipantIdentityDto>,
	fallback?: FallbackIdentity
): string {
	if (!uuid) return 'mike';
	const identity = index.get(uuid);
	if (identity) return identity.label;
	return fallback?.role ?? fallback?.agentType ?? shortSessionId(uuid);
}

export function resolveParticipantTokens(
	uuid: string | null,
	index: Map<string, ParticipantIdentityDto>,
	fallback?: FallbackIdentity
): RoleTokens {
	const identity = uuid ? index.get(uuid) : undefined;
	if (identity) return roleTokens(identity.role);
	return roleTokens(fallback?.role ?? null);
}

export function labelCarriesShortId(label: string, uuid: string | null): boolean {
	return uuid === null || label.includes(shortSessionId(uuid));
}
