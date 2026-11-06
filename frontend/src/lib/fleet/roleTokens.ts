import {
	Bot,
	CircleCheckBig,
	CircleQuestionMark,
	Code,
	Compass,
	Database,
	Eye,
	Network,
	Paintbrush,
	Search
} from '@lucide/svelte';
import type { Component } from 'svelte';
import { agentBgVar, agentBorderVar, agentFgVar } from './agentIdentity';

export interface RoleTokens {
	fg: string;
	bg: string;
	border: string;
	label: string;
	icon: Component;
}

const UNKNOWN_ROLE_TOKENS: RoleTokens = {
	fg: 'var(--status-muted-fg)',
	bg: 'var(--status-muted-bg)',
	border: 'var(--status-muted-border)',
	label: 'role unknown',
	icon: CircleQuestionMark
};

const ICON_KEYWORDS: ReadonlyArray<[RegExp, Component]> = [
	[/rust|forge/i, Code],
	[/frontend|window|svelte/i, Code],
	[/db-dev|database/i, Database],
	[/ui-designer|design|paint/i, Paintbrush],
	[/coordinat/i, Network],
	[/observ|watchdog/i, Eye],
	[/review|verif/i, CircleCheckBig],
	[/align/i, Compass],
	[/research/i, Search]
];

function iconFor(text: string): Component {
	for (const [re, icon] of ICON_KEYWORDS) {
		if (re.test(text)) return icon;
	}
	return Bot;
}

export function roleTokens(role: string | null): RoleTokens {
	if (role === null) return UNKNOWN_ROLE_TOKENS;
	return {
		fg: agentFgVar(role),
		bg: agentBgVar(role),
		border: agentBorderVar(role),
		label: role,
		icon: iconFor(role)
	};
}
