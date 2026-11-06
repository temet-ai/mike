const PINNED_AGENT_SLOTS: Record<string, number> = {
	'rust-developer': 1,
	'frontend-developer': 2,
	'ui-designer': 3,
	'db-dev': 4,
	watchdog: 5
};

const HASH_POOL_START = 6;
const HASH_POOL_SIZE = 6;

function stableHash(value: string): number {
	let hash = 0x811c9dc5;
	for (let i = 0; i < value.length; i++) {
		hash ^= value.charCodeAt(i);
		hash = Math.imul(hash, 0x01000193);
	}
	return hash >>> 0;
}

export function agentSlotForRole(role: string): number {
	const pinned = PINNED_AGENT_SLOTS[role];
	if (pinned !== undefined) return pinned;
	return HASH_POOL_START + (stableHash(role) % HASH_POOL_SIZE);
}

export function agentFgVar(role: string): string {
	return `var(--agent-${agentSlotForRole(role)}-fg)`;
}

export function agentBgVar(role: string): string {
	return `var(--agent-${agentSlotForRole(role)}-bg)`;
}

export function agentBorderVar(role: string): string {
	return `var(--agent-${agentSlotForRole(role)}-border)`;
}
