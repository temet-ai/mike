import type { Component } from 'svelte';
import {
	Bot,
	Copy,
	FileText,
	Globe,
	Layers,
	Monitor,
	Plug,
	Server,
	Terminal,
	Wrench
} from '@lucide/svelte';
import { anthropicMarkSvg } from '$lib/icons/vendorMarks';

export type IconKey =
	| 'session'
	| 'peer'
	| 'agent'
	| 'group'
	| 'tool'
	| 'service'
	| 'outside'
	| 'terminal'
	| 'browser'
	| 'mcp'
	| 'shell'
	| 'files'
	| 'internet'
	| 'model'
	| 'filters'
	| 'lessons'
	| 'judge'
	| 'embed'
	| 'verifier'
	| 'mailbox'
	| 'analyzers'
	| 'upkeep'
	| 'capture'
	| 'composer'
	| 'inject'
	| 'compress'
	| 'intercept'
	| 'extract'
	| 'persist'
	| 'router';

export interface IconEntry {
	readonly component?: Component<{ size?: number | string; class?: string }>;
	readonly svg?: string;
}

const LUCIDE: Partial<Record<IconKey, Component<{ size?: number | string; class?: string }>>> = {
	session: Monitor,
	peer: Copy,
	agent: Bot,
	group: Layers,
	tool: Wrench,
	service: Server,
	outside: Globe,
	terminal: Terminal,
	browser: Globe,
	mcp: Plug,
	shell: Terminal,
	files: FileText,
	internet: Globe
};

function svg(inner: string, fill = 'none'): string {
	return `<svg viewBox="0 0 16 16" width="14" height="14" fill="${fill}" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">${inner}</svg>`;
}

const RAW: Partial<Record<IconKey, string>> = {
	model: anthropicMarkSvg(14),
	filters: svg('<path d="M2 3h12l-4.5 5v4l-3 1.5V8z"></path>'),
	lessons: svg('<path d="M4 2h8v12l-4-3-4 3z"></path>'),
	judge: svg(
		'<path d="M8 2v12"></path><path d="M3 5h10"></path><path d="M3 12h10"></path><path d="M5 5l-2 5h4z"></path><path d="M11 5l-2 5h4z"></path>'
	),
	embed: svg(
		'<circle cx="4" cy="4" r="1.2"></circle><circle cx="12" cy="4" r="1.2"></circle><circle cx="4" cy="12" r="1.2"></circle><circle cx="12" cy="12" r="1.2"></circle><circle cx="8" cy="8" r="1.2"></circle>'
	),
	verifier: svg('<circle cx="8" cy="8" r="6"></circle><path d="M5 8l2 2 4-4"></path>'),
	mailbox: svg(
		'<path d="M2 9h3.5l1 2h3l1-2H14"></path><path d="M2 9V4h12v5"></path><path d="M2 9v3h12V9"></path>'
	),
	analyzers: svg('<path d="M2 8h2.5l2-4 2.5 8 2-4H14"></path>'),
	upkeep: svg('<path d="M13 8a5 5 0 1 1-1.5-3.5"></path><path d="M13 3v3h-3"></path>'),
	capture: svg(
		'<ellipse cx="8" cy="4" rx="5" ry="2"></ellipse><path d="M3 4v8c0 1.1 2.2 2 5 2s5-.9 5-2V4"></path><path d="M3 8c0 1.1 2.2 2 5 2s5-.9 5-2"></path>'
	),
	composer: svg(
		'<path d="M8 2l6 3-6 3-6-3z"></path><path d="M2 8l6 3 6-3"></path><path d="M2 11l6 3 6-3"></path>'
	),
	inject: svg('<path d="M8 2v8"></path><path d="M5 7l3 3 3-3"></path><path d="M3 13h10"></path>'),
	compress: svg(
		'<path d="M2 8h4M10 8h4"></path><path d="M4 5.5 6.5 8 4 10.5M12 5.5 9.5 8l2.5 2.5"></path>'
	),
	intercept: svg('<path d="M8 2l5 2v4c0 3-2.2 5-5 6-2.8-1-5-3-5-6V4z"></path>'),
	extract: svg(
		'<path d="M3 7v6h10V7"></path><path d="M8 2v7"></path><path d="M5.5 4.5 8 2l2.5 2.5"></path>'
	),
	persist: svg(
		'<rect x="2" y="3" width="12" height="10" rx="1"></rect><path d="M2 9h12"></path><circle cx="11.5" cy="11" r="0.7" fill="currentColor"></circle>'
	),
	router: svg(
		'<rect x="2" y="9" width="12" height="5" rx="1"></rect><path d="M5 9V6.5a3 3 0 0 1 6 0V9"></path><circle cx="5" cy="11.5" r="0.6" fill="currentColor"></circle><circle cx="8" cy="11.5" r="0.6" fill="currentColor"></circle>'
	)
};

export function resolveIcon(key: IconKey): IconEntry {
	const component = LUCIDE[key];
	if (component) return { component };
	return { svg: RAW[key] };
}
