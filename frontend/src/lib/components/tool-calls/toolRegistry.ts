import type { Component } from 'svelte';
import {
	Terminal,
	FileText,
	FilePen,
	FilePlus,
	Download,
	Search,
	Bot,
	ListTodo,
	Blocks,
	Wrench
} from '@lucide/svelte';

export type ToolKind =
	| 'bash'
	| 'read'
	| 'edit'
	| 'write'
	| 'fetch'
	| 'search'
	| 'task'
	| 'websearch'
	| 'todo'
	| 'mcp'
	| 'generic';

export interface KindPresentation {
	icon: Component;
	accentVar: string;
	label: string;
}

export const KIND_PRESENTATION: Record<ToolKind, KindPresentation> = {
	bash: { icon: Terminal, accentVar: '--status-info-fg', label: 'Bash' },
	read: { icon: FileText, accentVar: '--data-teal-fg', label: 'Read' },
	edit: { icon: FilePen, accentVar: '--role-tool-fg', label: 'Edit' },
	write: { icon: FilePlus, accentVar: '--status-success-fg', label: 'Write' },
	fetch: { icon: Download, accentVar: '--agent-4-fg', label: 'Fetch' },
	search: { icon: Search, accentVar: '--agent-3-fg', label: 'Search' },
	task: { icon: Bot, accentVar: '--data-teal-fg', label: 'Task' },
	websearch: { icon: Search, accentVar: '--status-info-fg', label: 'Web Search' },
	todo: { icon: ListTodo, accentVar: '--agent-2-fg', label: 'Todo' },
	mcp: { icon: Blocks, accentVar: '--data-viz-5-fg', label: 'MCP' },
	generic: { icon: Wrench, accentVar: '--text-3', label: 'Tool' }
};

const EDIT_TOOLS: ReadonlySet<string> = new Set([
	'Edit',
	'MultiEdit',
	'NotebookEdit',
	'NotebookEditCell'
]);

const WRITE_TOOLS: ReadonlySet<string> = new Set(['Write']);

const READ_TOOLS: ReadonlySet<string> = new Set(['Read', 'ReadFile', 'View']);

const FETCH_TOOLS: ReadonlySet<string> = new Set(['WebFetch', 'curl', 'fetch']);

const SEARCH_TOOLS: ReadonlySet<string> = new Set(['Grep', 'Glob']);

const TASK_TOOLS: ReadonlySet<string> = new Set(['Task', 'Agent']);

export function classify(toolName: string): ToolKind {
	if (typeof toolName !== 'string' || toolName.length === 0) return 'generic';
	if (toolName.startsWith('mcp__')) return 'mcp';
	if (toolName === 'Bash') return 'bash';
	if (READ_TOOLS.has(toolName)) return 'read';
	if (EDIT_TOOLS.has(toolName)) return 'edit';
	if (WRITE_TOOLS.has(toolName)) return 'write';
	if (FETCH_TOOLS.has(toolName)) return 'fetch';
	if (SEARCH_TOOLS.has(toolName)) return 'search';
	if (TASK_TOOLS.has(toolName)) return 'task';
	if (toolName === 'WebSearch') return 'websearch';
	if (toolName === 'TodoWrite') return 'todo';
	return 'generic';
}

export function presentationFor(toolName: string): KindPresentation {
	return KIND_PRESENTATION[classify(toolName)];
}

export interface McpName {
	server: string;
	method: string;
}

export function parseMcpName(toolName: string): McpName | null {
	if (typeof toolName !== 'string' || !toolName.startsWith('mcp__')) return null;
	const segments = toolName.split('__');
	const [, server = '', ...methodParts] = segments;
	return { server, method: methodParts.join('__') };
}

export function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function strField(input: unknown, key: string): string | null {
	if (!isRecord(input)) return null;
	const v = input[key];
	return typeof v === 'string' ? v : null;
}

export function numField(input: unknown, key: string): number | null {
	if (!isRecord(input)) return null;
	const v = input[key];
	return typeof v === 'number' && Number.isFinite(v) ? v : null;
}
