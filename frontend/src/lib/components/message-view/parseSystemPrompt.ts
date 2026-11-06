export function estimateTokens(charCount: number): number {
	return Math.max(0, Math.round(charCount / 4));
}

export function formatTokens(tokens: number): string {
	if (tokens >= 1024) {
		const k = tokens / 1000;
		const rounded = k >= 10 ? Math.round(k) : Math.round(k * 10) / 10;
		return `~${rounded}k tokens`;
	}
	return `~${tokens} tokens`;
}

export type ReminderSubKind =
	| { family: 'hook'; name: string }
	| { family: 'boot'; catalogue: 'mcp' | 'skills' | 'context' }
	| {
			family: 'event';
			name: 'date-change' | 'file-modified' | 'stale-task-list' | 'task-notification';
			severity: 'info' | 'warning';
	  }
	| { family: 'catchall' };

export interface ReminderBlock {
	index: number;
	text: string;
	charCount: number;
	estTokens: number;
	preview: string;
	subKind: ReminderSubKind;
}

export type GroupKind =
	| 'hooks'
	| 'mcp'
	| 'tools'
	| 'skills'
	| 'claudeMd'
	| 'context-global'
	| 'context-project'
	| 'event'
	| 'other';

export interface ReminderGroup {
	kind: GroupKind;
	label: string;
	subtitle: string;
	hint: string;
	blocks: ReminderBlock[];
	charCount: number;
	estTokens: number;
}

export type ClaudeMdBucket = 'global' | 'project' | 'memory' | 'harness';

export type ClaudeMdSubsection = 'claudeMd' | 'rules' | 'memory' | 'harness';

export interface ClaudeMdSection {
	path: string;
	displayPath: string;
	tagline: string;
	bucket: ClaudeMdBucket;
	subsection: ClaudeMdSubsection;
	charCount: number;
	estTokens: number;
	body: string;
}

export interface ClaudeMdReminder {
	kind: 'claudeMd';
	raw: string;
	charCount: number;
	estTokens: number;
	sections: ClaudeMdSection[];
}

export interface ParsedSystemPrompt {
	groups: ReminderGroup[];
	totalChars: number;
	totalTokens: number;
	totalBlocks: number;
	claudeMd: ClaudeMdReminder | null;
	agentSystemPrompt: string | null;
}

const REMINDER_WRAPPER = /^<system-reminder>\n?([\s\S]*?)\n?<\/system-reminder>\s*$/;

function stripReminderWrapper(text: string): { inner: string; wrapped: boolean } {
	const match = REMINDER_WRAPPER.exec(text);
	if (match && typeof match[1] === 'string') {
		return { inner: match[1], wrapped: true };
	}
	return { inner: text, wrapped: false };
}

function looksLikeHooks(inner: string): boolean {
	if (/\bhook success:/i.test(inner)) return true;
	const trimmed = inner.trimStart();
	return /^(SessionStart|UserPromptSubmit|Stop|SubagentStop|PreToolUse|PostToolUse|PreCompact|Notification|SessionEnd)\b/.test(
		trimmed
	);
}

const HOOK_NAMES = [
	'SessionStart',
	'SessionEnd',
	'InstructionsLoaded',
	'ConfigChange',
	'UserPromptSubmit',
	'Notification',
	'PreToolUse',
	'PostToolUse',
	'PostToolUseFailure',
	'PermissionRequest',
	'SubagentStart',
	'SubagentStop',
	'TeammateIdle',
	'Stop',
	'PreCompact',
	'PostCompact',
	'TodoWrite',
	'PreSubmit'
] as const;

function extractHookName(inner: string): string {
	const trimmed = inner.trimStart();
	for (const name of HOOK_NAMES) {
		const re = new RegExp(`^${name}\\b`);
		if (re.test(trimmed)) return name;
	}
	const fallback = /^([A-Z][A-Za-z0-9]+)(?:[:\s])/.exec(trimmed);
	if (fallback && fallback[1]) return fallback[1];
	return 'Unknown';
}

function looksLikeTools(inner: string): boolean {
	if (/\bdeferred tools\b/i.test(inner)) return true;
	if (/\bToolSearch\b/.test(inner)) return true;
	if (/^#\s*Available Tools\b/im.test(inner)) return true;
	return /^<tools>/.test(inner.trimStart());
}

function looksLikeMcp(inner: string): boolean {
	if (/^#\s*MCP Server Instructions\b/im.test(inner)) return true;
	if (/^##\s+[\w.:-]+\s*$/m.test(inner) && /MCP\b/i.test(inner)) return true;
	return false;
}

function looksLikeSkills(inner: string): boolean {
	if (/^#\s*Skills\b/im.test(inner)) return true;
	if (/^Available skills\b/im.test(inner)) return true;
	return /^The following skills are available/im.test(inner);
}

function looksLikeClaudeMd(inner: string): boolean {
	if (/^#\s*claudeMd\b/im.test(inner)) return true;
	if (/^Contents of /m.test(inner)) return true;
	return /Codebase and user instructions are shown below/i.test(inner);
}

function looksLikeDateChange(inner: string): boolean {
	return /^The date has changed/m.test(inner.trimStart());
}

function looksLikeStaleTaskList(inner: string): boolean {
	return /^The task tools haven't been used recently/m.test(inner.trimStart());
}

function looksLikeFileModified(inner: string): boolean {
	const t = inner.trimStart();
	if (/^Note: .+ has been modified/m.test(t)) return true;
	if (/^The file .+ has been modified by the user/m.test(t)) return true;
	return false;
}

function looksLikeTaskNotification(inner: string): boolean {
	const t = inner.trimStart();
	if (/^<task-notification\b/.test(t)) return true;
	if (/^Background task\b/m.test(t)) return true;
	return false;
}

function classifyBlock(inner: string): GroupKind {
	if (looksLikeHooks(inner)) return 'hooks';
	if (looksLikeClaudeMd(inner)) return 'claudeMd';
	if (looksLikeMcp(inner)) return 'mcp';
	if (looksLikeTools(inner)) return 'tools';
	if (looksLikeSkills(inner)) return 'skills';
	if (
		looksLikeDateChange(inner) ||
		looksLikeStaleTaskList(inner) ||
		looksLikeFileModified(inner) ||
		looksLikeTaskNotification(inner)
	) {
		return 'event';
	}
	return 'other';
}

export function classifySubKind(inner: string): ReminderSubKind {
	if (looksLikeHooks(inner)) {
		return { family: 'hook', name: extractHookName(inner) };
	}
	if (looksLikeClaudeMd(inner)) {
		return { family: 'boot', catalogue: 'context' };
	}
	if (looksLikeMcp(inner)) {
		return { family: 'boot', catalogue: 'mcp' };
	}
	if (looksLikeSkills(inner)) {
		return { family: 'boot', catalogue: 'skills' };
	}
	if (looksLikeDateChange(inner)) {
		return { family: 'event', name: 'date-change', severity: 'info' };
	}
	if (looksLikeStaleTaskList(inner)) {
		return { family: 'event', name: 'stale-task-list', severity: 'warning' };
	}
	if (looksLikeFileModified(inner)) {
		return { family: 'event', name: 'file-modified', severity: 'info' };
	}
	if (looksLikeTaskNotification(inner)) {
		return { family: 'event', name: 'task-notification', severity: 'info' };
	}
	return { family: 'catchall' };
}

const CONTENTS_OF_RE = /^Contents of (.+?)(?: \((.*?)\))?:\s*$/gm;
const HARNESS_H1_RE = /^#\s+([a-zA-Z][a-zA-Z0-9_]*)\s*$/gm;

function bucketFromTagline(tagline: string, path: string): ClaudeMdBucket {
	const t = tagline.toLowerCase();
	if (/private\s+global\s+instructions/.test(t)) return 'global';
	if (/global\s+instructions/.test(t)) return 'global';
	if (/project\s+instructions/.test(t)) return 'project';
	if (/auto[-\s]?memory/.test(t)) return 'memory';
	if (/(^|\/)\.claude\//.test(path)) return 'global';
	return 'project';
}

function subsectionForSection(bucket: ClaudeMdBucket, path: string): ClaudeMdSubsection {
	if (bucket === 'memory') return 'memory';
	if (bucket === 'harness') return 'harness';
	const file = path.split('/').pop() ?? '';
	if (/^CLAUDE\.md$/i.test(file)) return 'claudeMd';
	if (/(^|\/)rules\//.test(path)) return 'rules';
	return 'claudeMd';
}

function abbreviatePath(path: string): string {
	const homeMatch = /^\/Users\/[^/]+\/(.*)$/.exec(path);
	if (homeMatch && homeMatch[1]) return `~/${homeMatch[1]}`;
	return path;
}

function parseClaudeMdSections(body: string): ClaudeMdSection[] {
	const sections: ClaudeMdSection[] = [];
	const matches: Array<{ path: string; tagline: string; start: number; headerEnd: number }> = [];
	CONTENTS_OF_RE.lastIndex = 0;
	for (;;) {
		const m = CONTENTS_OF_RE.exec(body);
		if (!m) break;
		matches.push({
			path: m[1] ?? '',
			tagline: m[2] ?? '',
			start: m.index,
			headerEnd: m.index + m[0].length
		});
	}

	for (let i = 0; i < matches.length; i++) {
		const cur = matches[i];
		if (!cur) continue;
		const next = matches[i + 1];
		let bodyEnd: number;
		if (next) {
			bodyEnd = next.start;
		} else {
			HARNESS_H1_RE.lastIndex = cur.headerEnd;
			const firstHarness = HARNESS_H1_RE.exec(body);
			bodyEnd = firstHarness ? firstHarness.index : body.length;
		}
		const fileBody = body.slice(cur.headerEnd, bodyEnd).trimStart();
		const bucket = bucketFromTagline(cur.tagline, cur.path);
		const subsection = subsectionForSection(bucket, cur.path);
		sections.push({
			path: cur.path,
			displayPath: abbreviatePath(cur.path),
			tagline: cur.tagline,
			bucket,
			subsection,
			charCount: fileBody.length,
			estTokens: estimateTokens(fileBody.length),
			body: fileBody
		});
	}

	const scanStart = matches.length > 0 ? (matches[matches.length - 1]?.headerEnd ?? 0) : 0;
	HARNESS_H1_RE.lastIndex = scanStart;
	const harness: Array<{ name: string; start: number; headerEnd: number }> = [];
	for (;;) {
		const m = HARNESS_H1_RE.exec(body);
		if (!m) break;
		if (m.index < scanStart) continue;
		harness.push({
			name: m[1] ?? '',
			start: m.index,
			headerEnd: m.index + m[0].length
		});
	}
	for (let j = 0; j < harness.length; j++) {
		const cur = harness[j];
		if (!cur) continue;
		const next = harness[j + 1];
		const bodyEnd = next ? next.start : body.length;
		const harnessBody = body.slice(cur.headerEnd, bodyEnd).trim();
		sections.push({
			path: cur.name,
			displayPath: cur.name,
			tagline: 'harness',
			bucket: 'harness',
			subsection: 'harness',
			charCount: harnessBody.length,
			estTokens: estimateTokens(harnessBody.length),
			body: harnessBody
		});
	}

	return sections;
}

function buildClaudeMdReminder(inner: string): ClaudeMdReminder {
	return {
		kind: 'claudeMd',
		raw: inner,
		charCount: inner.length,
		estTokens: estimateTokens(inner.length),
		sections: parseClaudeMdSections(inner)
	};
}

function makeBlock(rawText: string, index: number): ReminderBlock {
	const { inner } = stripReminderWrapper(rawText);
	return {
		index,
		text: inner,
		charCount: inner.length,
		estTokens: estimateTokens(inner.length),
		preview: inner.trim(),
		subKind: classifySubKind(inner)
	};
}

const GROUP_META: Record<GroupKind, Omit<ReminderGroup, 'blocks' | 'charCount' | 'estTokens'>> = {
	hooks: {
		kind: 'hooks',
		label: 'Hooks',
		subtitle: 'Lifecycle events fired',
		hint: 'in fire order'
	},
	mcp: {
		kind: 'mcp',
		label: 'MCP',
		subtitle: 'Servers & tools',
		hint: ''
	},
	tools: {
		kind: 'tools',
		label: 'Tools',
		subtitle: 'Available tool schemas',
		hint: ''
	},
	skills: {
		kind: 'skills',
		label: 'Skills',
		subtitle: 'Available skills',
		hint: ''
	},
	claudeMd: {
		kind: 'claudeMd',
		label: 'Context',
		subtitle: 'claudeMd',
		hint: ''
	},
	'context-global': {
		kind: 'context-global',
		label: 'Context',
		subtitle: 'Global',
		hint: '~/.claude/'
	},
	'context-project': {
		kind: 'context-project',
		label: 'Context',
		subtitle: 'Project',
		hint: ''
	},
	event: {
		kind: 'event',
		label: 'Events',
		subtitle: 'Mid-session reminders',
		hint: ''
	},
	other: {
		kind: 'other',
		label: 'Other',
		subtitle: 'Uncategorised reminders',
		hint: ''
	}
};

function assembleGroups(buckets: Map<GroupKind, ReminderBlock[]>): ReminderGroup[] {
	const prescribed: GroupKind[] = [
		'hooks',
		'mcp',
		'tools',
		'skills',
		'claudeMd',
		'context-global',
		'context-project',
		'event',
		'other'
	];
	const out: ReminderGroup[] = [];
	for (const kind of prescribed) {
		const list = buckets.get(kind);
		if (!list || list.length === 0) continue;
		const charCount = list.reduce((acc, b) => acc + b.charCount, 0);
		out.push({
			...GROUP_META[kind],
			blocks: list,
			charCount,
			estTokens: estimateTokens(charCount)
		});
	}
	return out;
}

export function parseReminders(
	userMessageBlocks: string[],
	agentSystemPrompt: string | null
): ParsedSystemPrompt {
	const buckets = new Map<GroupKind, ReminderBlock[]>();
	let claudeMd: ClaudeMdReminder | null = null;

	userMessageBlocks.forEach((raw, i) => {
		if (typeof raw !== 'string' || raw.length === 0) return;
		const { inner, wrapped } = stripReminderWrapper(raw);
		if (!wrapped) return;
		const block = makeBlock(raw, i);
		const kind = classifyBlock(inner);
		const list = buckets.get(kind) ?? [];
		list.push(block);
		buckets.set(kind, list);
		if (kind === 'claudeMd' && claudeMd === null) {
			claudeMd = buildClaudeMdReminder(inner);
		}
	});

	const groups = assembleGroups(buckets);
	const totalChars = groups.reduce((acc, g) => acc + g.charCount, 0);
	const totalBlocks = groups.reduce((acc, g) => acc + g.blocks.length, 0);
	return {
		groups,
		totalChars,
		totalTokens: estimateTokens(totalChars),
		totalBlocks,
		claudeMd,
		agentSystemPrompt
	};
}

function buildBlocks(raw: string): ReminderBlock[] {
	if (raw.length === 0) return [];
	return raw
		.split(/\n{2,}/)
		.map((block, index) => makeBlock(block, index))
		.filter((b) => b.charCount > 0);
}

const LEGACY_HEADER_PATTERNS: Array<[GroupKind, RegExp]> = [
	['hooks', /^(?:#{1,3}\s*(?:hook|hooks|lifecycle hook)|<hook|hook:)/i],
	['mcp', /^(?:#{1,3}\s*(?:mcp|mcp server)|<mcp|mcp_servers|mcp server:)/i],
	['tools', /^(?:#{1,3}\s*(?:tools?|available tools)|<tools|tools available)/i],
	['skills', /^(?:#{1,3}\s*skills?|<skills)/i],
	[
		'context-global',
		/^(?:contents of ~\/\.claude|~\/\.claude\/|contents of .*\.claude\/claude\.md|global instructions|user's (?:private )?global)/i
	],
	[
		'context-project',
		/^(?:contents of \/(?!users\/[^/]+\/\.claude)|project (?:rules|context|instructions)|codebase and user instructions|claudemd)/i
	]
];

function classifyLegacy(text: string): GroupKind {
	const trimmed = text.trimStart();
	for (const [kind, pattern] of LEGACY_HEADER_PATTERNS) {
		if (pattern.test(trimmed)) return kind;
	}
	return 'other';
}

export function parseSystemPrompt(raw: string | null): ParsedSystemPrompt {
	if (raw === null || raw.length === 0) {
		return {
			groups: [],
			totalChars: 0,
			totalTokens: 0,
			totalBlocks: 0,
			claudeMd: null,
			agentSystemPrompt: raw
		};
	}
	const blocks = buildBlocks(raw);
	const buckets = new Map<GroupKind, ReminderBlock[]>();
	for (const block of blocks) {
		const kind = classifyLegacy(block.text);
		const list = buckets.get(kind) ?? [];
		list.push(block);
		buckets.set(kind, list);
	}
	const groups = assembleGroups(buckets);
	const totalChars = blocks.reduce((acc, b) => acc + b.charCount, 0);
	return {
		groups,
		totalChars,
		totalTokens: estimateTokens(totalChars),
		totalBlocks: blocks.length,
		claudeMd: null,
		agentSystemPrompt: raw
	};
}
