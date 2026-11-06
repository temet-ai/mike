import type { LegendEntry } from '$lib/components/shared/LegendFilter.svelte';

export const LEGEND_ENTRIES: readonly LegendEntry[] = [
	{
		key: 'who-session',
		label: 'SESSION · TERMINAL',
		kind: 'fill',
		colorToken: '--ident-session-fg'
	},
	{ key: 'who-agent', label: 'AGENT', kind: 'fill', colorToken: '--ident-agent-fg' },
	{ key: 'who-tool', label: 'TOOL · MCP', kind: 'fill', colorToken: '--ident-tool-fg' },
	{ key: 'who-mike', label: 'MIKE', kind: 'fill', colorToken: '--ident-mike-fg' },
	{ key: 'who-anthropic', label: 'ANTHROPIC', kind: 'fill', colorToken: '--vendor-anthropic' },
	{
		key: 'capture',
		label: 'MODEL REQUEST',
		kind: 'line',
		colorToken: '--ident-agent-fg',
		pattern: ''
	},
	{
		key: 'upstream',
		label: 'THROUGH THE BOUNDARY',
		kind: 'line',
		colorToken: '--signal-active',
		pattern: ''
	},
	{
		key: 'response',
		label: 'MODEL RESPONSE',
		kind: 'line',
		colorToken: '--vendor-anthropic',
		pattern: ''
	},
	{
		key: 'toolcall',
		label: 'TOOL CALL',
		kind: 'line',
		colorToken: '--ident-agent-fg',
		pattern: '8 3'
	},
	{
		key: 'return',
		label: 'TOOL RESULT',
		kind: 'line',
		colorToken: '--ident-tool-fg',
		pattern: '8 3'
	},
	{
		key: 'internet',
		label: 'LEAVES THE BOUNDARY',
		kind: 'line',
		colorToken: '--ident-tool-fg',
		pattern: '2 3'
	},
	{
		key: 'mikeout',
		label: 'MIKE OUTBOUND',
		kind: 'line',
		colorToken: '--ident-mike-fg',
		pattern: ''
	},
	{
		key: 'audit',
		label: 'MIKE EVENT → SESSION',
		kind: 'line',
		colorToken: '--ident-mike-fg',
		pattern: '1 4'
	},
	{
		key: 'dispatch',
		label: 'DISPATCH',
		kind: 'line',
		colorToken: '--ident-session-fg',
		pattern: '3 5'
	},
	{
		key: 'message',
		label: 'MAILBOX · OPERATOR',
		kind: 'line',
		colorToken: '--ident-terminal-fg',
		pattern: ''
	},
	{ key: 'error', label: 'ERROR', kind: 'ring', colorToken: '--signal-error' },
	{ key: 'idle', label: 'IDLE', kind: 'ring', colorToken: '--border-strong' }
];

export const ALL_LEGEND_KEYS: readonly string[] = LEGEND_ENTRIES.map((entry) => entry.key);

export function defaultLegend(): Set<string> {
	return new Set(ALL_LEGEND_KEYS);
}
