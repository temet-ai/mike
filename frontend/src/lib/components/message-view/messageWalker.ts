import { deepEqual, stripBillingHeader, stripCacheControl } from './stripChrome';

export type ContentBlock =
	| { kind: 'text'; text: string }
	| { kind: 'thinking'; text: string }
	| {
			kind: 'tool_use';
			id: string;
			name: string;
			input: unknown;
			subagent_type?: string;
			agent_description?: string;
	  }
	| {
			kind: 'tool_result';
			tool_use_id: string;
			text: string;
			is_error: boolean;
			tool_name: string;
			tool_input?: unknown;
			subagent_type?: string;
			agent_description?: string;
	  };

const SUBAGENT_TOOL_NAMES = new Set(['Agent', 'Task']);

function extractSubagentMeta(
	name: string,
	input: unknown
): { subagent_type?: string; agent_description?: string } {
	if (!SUBAGENT_TOOL_NAMES.has(name)) return {};
	if (!isRecord(input)) return {};
	const out: { subagent_type?: string; agent_description?: string } = {};
	if (typeof input.subagent_type === 'string' && input.subagent_type.length > 0) {
		out.subagent_type = input.subagent_type;
	}
	if (typeof input.description === 'string' && input.description.length > 0) {
		out.agent_description = input.description;
	}
	return out;
}

export interface Message {
	role: 'user' | 'assistant';
	blocks: ContentBlock[];
	originIndex: number;
	rawSize: number;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function toolResultText(content: unknown): string {
	if (typeof content === 'string') return content;
	if (!Array.isArray(content)) {
		if (isRecord(content)) return toolResultText([content]);
		return '';
	}
	const parts: string[] = [];
	for (const entry of content) {
		if (typeof entry === 'string') {
			parts.push(entry);
		} else if (isRecord(entry)) {
			if (entry.type === 'text' && typeof entry.text === 'string') {
				parts.push(entry.text);
			}
		}
	}
	return parts.join('\n');
}

function pushUserBlock(
	blocks: ContentBlock[],
	block: unknown,
	toolNameByUseId: Map<string, string>,
	toolInputByUseId: Map<string, unknown>,
	subagentMetaByUseId: Map<string, { subagent_type?: string; agent_description?: string }>
): void {
	if (typeof block === 'string') {
		if (block.length > 0) blocks.push({ kind: 'text', text: block });
		return;
	}
	if (!isRecord(block)) return;
	const type = typeof block.type === 'string' ? block.type : '';
	if (type === 'text' && typeof block.text === 'string') {
		blocks.push({ kind: 'text', text: block.text });
	} else if (type === 'tool_result') {
		const tool_use_id = typeof block.tool_use_id === 'string' ? block.tool_use_id : '';
		const tool_name = toolNameByUseId.get(tool_use_id) ?? '';
		const tool_input = toolInputByUseId.get(tool_use_id);
		const agentMeta = subagentMetaByUseId.get(tool_use_id) ?? {};
		blocks.push({
			kind: 'tool_result',
			tool_use_id,
			text: toolResultText(block.content),
			is_error: block.is_error === true,
			tool_name,
			tool_input,
			...agentMeta
		});
	}
}

function pushAssistantBlock(
	blocks: ContentBlock[],
	block: unknown,
	toolNameByUseId: Map<string, string>,
	toolInputByUseId: Map<string, unknown>,
	subagentMetaByUseId: Map<string, { subagent_type?: string; agent_description?: string }>
): void {
	if (typeof block === 'string') {
		if (block.length > 0) blocks.push({ kind: 'text', text: block });
		return;
	}
	if (!isRecord(block)) return;
	const type = typeof block.type === 'string' ? block.type : '';
	switch (type) {
		case 'text':
			if (typeof block.text === 'string' && block.text.length > 0) {
				blocks.push({ kind: 'text', text: block.text });
			}
			return;
		case 'thinking':
			if (typeof block.thinking === 'string' && block.thinking.length > 0) {
				blocks.push({ kind: 'thinking', text: block.thinking });
			}
			return;
		case 'tool_use': {
			const id = typeof block.id === 'string' ? block.id : '';
			const name = typeof block.name === 'string' ? block.name : '';
			if (id.length > 0) {
				toolNameByUseId.set(id, name);
				toolInputByUseId.set(id, block.input);
			}
			const agentMeta = extractSubagentMeta(name, block.input);
			if (id.length > 0 && (agentMeta.subagent_type || agentMeta.agent_description)) {
				subagentMetaByUseId.set(id, agentMeta);
			}
			blocks.push({ kind: 'tool_use', id, name, input: block.input, ...agentMeta });
			return;
		}
		default:
			return;
	}
}

export function estimateBlockSize(block: ContentBlock): number {
	switch (block.kind) {
		case 'text':
		case 'thinking':
			return block.text.length;
		case 'tool_result':
			return block.text.length;
		case 'tool_use':
			try {
				return JSON.stringify(block.input ?? '').length;
			} catch {
				return 0;
			}
	}
}

function walkMessageAt(
	msg: unknown,
	originIndex: number,
	toolNameByUseId: Map<string, string>,
	toolInputByUseId: Map<string, unknown>,
	subagentMetaByUseId: Map<string, { subagent_type?: string; agent_description?: string }>
): Message | null {
	if (!isRecord(msg)) return null;
	const roleRaw = typeof msg.role === 'string' ? msg.role : '';
	const role: 'user' | 'assistant' | null =
		roleRaw === 'user' ? 'user' : roleRaw === 'assistant' ? 'assistant' : null;
	if (role === null) return null;
	const content = msg.content;
	const blocks: ContentBlock[] = [];
	if (role === 'user') {
		if (typeof content === 'string') {
			pushUserBlock(blocks, content, toolNameByUseId, toolInputByUseId, subagentMetaByUseId);
		} else if (Array.isArray(content)) {
			for (const block of content)
				pushUserBlock(blocks, block, toolNameByUseId, toolInputByUseId, subagentMetaByUseId);
		}
	} else {
		if (typeof content === 'string') {
			pushAssistantBlock(blocks, content, toolNameByUseId, toolInputByUseId, subagentMetaByUseId);
		} else if (Array.isArray(content)) {
			for (const block of content)
				pushAssistantBlock(blocks, block, toolNameByUseId, toolInputByUseId, subagentMetaByUseId);
		}
	}
	let rawSize = 0;
	for (const b of blocks) rawSize += estimateBlockSize(b);
	return { role, blocks, originIndex, rawSize };
}

export function walkRequestMessages(parsed: unknown): Message[] {
	if (!isRecord(parsed)) return [];
	const messages = parsed.messages;
	if (!Array.isArray(messages)) return [];
	const out: Message[] = [];
	const toolNameByUseId = new Map<string, string>();
	const toolInputByUseId = new Map<string, unknown>();
	const subagentMetaByUseId = new Map<
		string,
		{ subagent_type?: string; agent_description?: string }
	>();
	for (let i = 0; i < messages.length; i++) {
		const m = walkMessageAt(messages[i], i, toolNameByUseId, toolInputByUseId, subagentMetaByUseId);
		if (m !== null) out.push(m);
	}
	return out;
}

export function extractTrailingUserTurn(parsed: unknown): Message[] {
	const all = walkRequestMessages(parsed);
	if (all.length === 0) return [];
	let lastAssistant = -1;
	for (let i = 0; i < all.length; i++) {
		if (all[i]?.role === 'assistant') lastAssistant = i;
	}
	const tail = all.slice(lastAssistant + 1);
	return tail.filter((m) => m.role === 'user');
}

export function walkResponseContent(parsed: unknown, originIndex: number): Message | null {
	if (!isRecord(parsed)) return null;
	const content = parsed.content;
	if (!Array.isArray(content)) return null;
	const toolNameByUseId = new Map<string, string>();
	const toolInputByUseId = new Map<string, unknown>();
	const subagentMetaByUseId = new Map<
		string,
		{ subagent_type?: string; agent_description?: string }
	>();
	const blocks: ContentBlock[] = [];
	for (const block of content)
		pushAssistantBlock(blocks, block, toolNameByUseId, toolInputByUseId, subagentMetaByUseId);
	if (blocks.length === 0) return null;
	let rawSize = 0;
	for (const b of blocks) rawSize += estimateBlockSize(b);
	return { role: 'assistant', blocks, originIndex, rawSize };
}

export function firstUserMessageTextBlocks(parsed: unknown): string[] {
	if (!isRecord(parsed)) return [];
	const messages = parsed.messages;
	if (!Array.isArray(messages) || messages.length === 0) return [];
	const first = messages[0];
	if (!isRecord(first)) return [];
	if (first.role !== 'user') return [];
	const content = first.content;
	if (typeof content === 'string') {
		return [content];
	}
	if (!Array.isArray(content)) return [];
	const out: string[] = [];
	for (const block of content) {
		if (typeof block === 'string') {
			out.push(block);
		} else if (isRecord(block) && block.type === 'text' && typeof block.text === 'string') {
			out.push(block.text);
		}
	}
	return out;
}

const REMINDER_WRAP_RE = /^<system-reminder>[\s\S]*<\/system-reminder>\s*$/;

export function isReminderBlock(text: string): boolean {
	return REMINDER_WRAP_RE.test(text.trim());
}

export function splitFirstUserMessage(blocks: string[]): {
	reminders: string[];
	primaryUserText: string;
} {
	const reminders: string[] = [];
	const userText: string[] = [];
	for (const b of blocks) {
		if (isReminderBlock(b)) {
			reminders.push(b);
		} else if (b.length > 0) {
			userText.push(b);
		}
	}
	return { reminders, primaryUserText: userText.join('\n\n') };
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function pickString(input: Record<string, unknown>, key: string): string | undefined {
	const v = input[key];
	return typeof v === 'string' && v.length > 0 ? v : undefined;
}

export function summarizeToolInput(name: string, input: unknown): string {
	if (!isPlainObject(input)) {
		if (input === undefined || input === null) return '';
		try {
			return JSON.stringify(input);
		} catch {
			return String(input);
		}
	}
	switch (name) {
		case 'Bash': {
			const cmd = pickString(input, 'command');
			if (cmd) return cmd;
			break;
		}
		case 'Read':
		case 'Write':
		case 'Edit':
		case 'MultiEdit':
		case 'NotebookEdit': {
			const path = pickString(input, 'file_path') ?? pickString(input, 'notebook_path');
			if (path) return path;
			break;
		}
		case 'Grep': {
			const pattern = pickString(input, 'pattern');
			const path = pickString(input, 'path');
			if (pattern && path) return `${pattern}  in  ${path}`;
			if (pattern) return pattern;
			break;
		}
		case 'Glob': {
			const pattern = pickString(input, 'pattern');
			if (pattern) return pattern;
			break;
		}
		case 'WebFetch': {
			const url = pickString(input, 'url');
			if (url) return url;
			break;
		}
		case 'WebSearch': {
			const q = pickString(input, 'query');
			if (q) return q;
			break;
		}
		case 'Agent':
		case 'Task': {
			const who = pickString(input, 'subagent_type');
			const desc = pickString(input, 'description');
			if (who && desc) return `${who} · ${desc}`;
			if (who) return who;
			if (desc) return desc;
			break;
		}
		case 'TaskCreate':
		case 'TaskUpdate':
		case 'TaskGet': {
			const subject = pickString(input, 'subject');
			const taskId = pickString(input, 'taskId') ?? pickString(input, 'task_id');
			if (subject) return subject;
			if (taskId) return `task ${taskId}`;
			break;
		}
		case 'SendMessage': {
			const to = pickString(input, 'to');
			const summary = pickString(input, 'summary');
			if (to && summary) return `→ ${to} · ${summary}`;
			if (to) return `→ ${to}`;
			if (summary) return summary;
			break;
		}
		case 'PushNotification': {
			const msg = pickString(input, 'message');
			if (msg) return msg;
			break;
		}
		case 'Skill': {
			const skill = pickString(input, 'skill');
			const args = pickString(input, 'args');
			if (skill && args) return `${skill} · ${args}`;
			if (skill) return skill;
			break;
		}
	}
	for (const key of Object.keys(input)) {
		const v = input[key];
		if (typeof v === 'string' && v.length > 0) return `${key}: ${v}`;
	}
	for (const key of Object.keys(input)) {
		const v = input[key];
		if (typeof v === 'number' || typeof v === 'boolean') return `${key}: ${v}`;
	}
	const keys = Object.keys(input);
	return keys.length > 0 ? `[${keys.join(', ')}]` : '';
}

export function userMessageInlinePreview(blocks: ContentBlock[]): string {
	const parts: string[] = [];
	for (const block of blocks) {
		if (block.kind !== 'tool_result') continue;
		const summary = summarizeToolInput(block.tool_name || 'result', block.tool_input);
		if (summary.length > 0) parts.push(summary);
	}
	return parts.join('  ·  ');
}

export type DeltaShape = 'tail-append' | 'compaction' | 'first-turn' | 'lineage-broken';

export interface MessageDelta {
	messages_added: Message[];
	commonLen: number;
	currLen: number;
	systemChanged: boolean;
	toolsChanged: boolean;
	modelChanged: boolean;
	paramsChanged: boolean;
	shape: DeltaShape;
}

function emptyDelta(shape: DeltaShape, currLen: number = 0): MessageDelta {
	return {
		messages_added: [],
		commonLen: 0,
		currLen,
		systemChanged: false,
		toolsChanged: false,
		modelChanged: false,
		paramsChanged: false,
		shape
	};
}

export function walkMessageDelta(prev: unknown, curr: unknown): MessageDelta {
	if (!isRecord(curr)) return emptyDelta('lineage-broken');
	if (!isRecord(prev)) return emptyDelta('first-turn');

	const prevMessages = Array.isArray(prev.messages) ? prev.messages : [];
	const currMessages = Array.isArray(curr.messages) ? curr.messages : [];

	const prevCanon = prevMessages.map((m) => stripCacheControl(m));
	const currCanon = currMessages.map((m) => stripCacheControl(m));
	let commonLen = 0;
	while (
		commonLen < prevCanon.length &&
		commonLen < currCanon.length &&
		deepEqual(prevCanon[commonLen], currCanon[commonLen])
	) {
		commonLen++;
	}

	let shape: DeltaShape;
	if (commonLen === 0 && currMessages.length === prevMessages.length && currMessages.length > 0) {
		shape = 'lineage-broken';
	} else if (commonLen === 0 && currMessages.length < prevMessages.length) {
		shape = 'compaction';
	} else if (commonLen === prevMessages.length && currMessages.length >= prevMessages.length) {
		shape = 'tail-append';
	} else if (commonLen === 0 && currMessages.length === 0 && prevMessages.length === 0) {
		shape = 'tail-append';
	} else {
		shape = 'lineage-broken';
	}

	const tailIndices: Message[] = [];
	if (shape === 'tail-append' && commonLen < currMessages.length) {
		const toolNameByUseId = new Map<string, string>();
		const toolInputByUseId = new Map<string, unknown>();
		const subagentMetaByUseId = new Map<
			string,
			{ subagent_type?: string; agent_description?: string }
		>();
		for (let i = 0; i < commonLen; i++) {
			walkMessageAt(currMessages[i], i, toolNameByUseId, toolInputByUseId, subagentMetaByUseId);
		}
		for (let i = commonLen; i < currMessages.length; i++) {
			const m = walkMessageAt(
				currMessages[i],
				i,
				toolNameByUseId,
				toolInputByUseId,
				subagentMetaByUseId
			);
			if (m !== null) tailIndices.push(m);
		}
	}

	const prevSystem = stripBillingHeader(stripCacheControl(prev.system));
	const currSystem = stripBillingHeader(stripCacheControl(curr.system));
	const systemChanged = !deepEqual(prevSystem, currSystem);

	const prevTools = (prev as Record<string, unknown>).tools ?? null;
	const currTools = (curr as Record<string, unknown>).tools ?? null;
	const toolsChanged = !deepEqual(prevTools, currTools);

	const prevModel = (prev as Record<string, unknown>).model ?? null;
	const currModel = (curr as Record<string, unknown>).model ?? null;
	const modelChanged = prevModel !== currModel;

	const paramKeys = ['max_tokens', 'temperature', 'top_p', 'top_k', 'stop_sequences'] as const;
	const paramsChanged = paramKeys.some(
		(k) => !deepEqual((prev as Record<string, unknown>)[k], (curr as Record<string, unknown>)[k])
	);

	return {
		messages_added: tailIndices,
		commonLen,
		currLen: currMessages.length,
		systemChanged,
		toolsChanged,
		modelChanged,
		paramsChanged,
		shape
	};
}
