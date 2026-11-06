import { describe, expect, it } from 'vitest';
import {
	classifySubKind,
	estimateTokens,
	formatTokens,
	parseReminders,
	parseSystemPrompt,
	type ClaudeMdReminder
} from './parseSystemPrompt';

const BLOCK_HOOKS_START =
	'<system-reminder>\nSessionStart:startup hook success: OK\n</system-reminder>';
const BLOCK_TOOLS =
	'<system-reminder>\nThe following deferred tools are now available via ToolSearch. Their schemas are NOT loaded — calling them directly will fail with InputValidationError.\n</system-reminder>';
const BLOCK_MCP = `<system-reminder>
# MCP Server Instructions

The following MCP servers have provided instructions for how to use their tools and resources:

## mike
Read-only tools and resources backed by a running mike inspect bind.
</system-reminder>`;
const BLOCK_HOOK_SUBMIT =
	'<system-reminder>\nUserPromptSubmit hook success: OK\n</system-reminder>';
const BLOCK_CLAUDE_MD = `<system-reminder>
As you answer the user's questions, you can use the following context:
# claudeMd
Codebase and user instructions are shown below. Be sure to adhere to these instructions.

Contents of /Users/alice/.claude/CLAUDE.md (user's private global instructions for all projects):

# Global Instructions
Do things this way.

Contents of /Users/alice/.claude/rules/rule-one.md (user's private global instructions for all projects):

Rule one body.

Contents of /Users/alice/.claude/rules/rule-two.md (user's private global instructions for all projects):

Rule two body.

Contents of /Users/alice/work/proxy/CLAUDE.md (project instructions, checked into the codebase):

# Proxy project
Project body.

Contents of /Users/alice/work/proxy/child/CLAUDE.md (project instructions, checked into the codebase):

# Child module
Child body.

Contents of /Users/alice/work/proxy/.claude/rules/testing.md (project instructions, checked into the codebase):

Testing rule body.

Contents of /Users/alice/.claude/projects/-Users-alice-work-proxy/memory/MEMORY.md (user's auto-memory, persists across conversations):

Memory body.
# userEmail
alice@example.com
# currentDate
2026-04-19
</system-reminder>`;

const BLOCK_USER_PROMPT = 'I want you to fix the thing.';

const AGENT_SYSTEM_PROMPT =
	"You are Claude Code, Anthropic's official CLI for Claude.\n\nYou are a Frontend Development Expert...";

describe('estimateTokens', () => {
	it('rounds to 4 chars per token', () => {
		expect(estimateTokens(0)).toBe(0);
		expect(estimateTokens(4)).toBe(1);
		expect(estimateTokens(10)).toBe(3);
		expect(estimateTokens(100)).toBe(25);
	});
	it('never returns a negative token count', () => {
		expect(estimateTokens(-5)).toBe(0);
	});
});

describe('parseReminders', () => {
	it('returns empty structure for empty input', () => {
		const out = parseReminders([], null);
		expect(out.groups).toEqual([]);
		expect(out.claudeMd).toBeNull();
		expect(out.totalBlocks).toBe(0);
		expect(out.totalChars).toBe(0);
	});

	it('ignores blocks not wrapped in <system-reminder>', () => {
		const out = parseReminders([BLOCK_USER_PROMPT], null);
		expect(out.groups).toEqual([]);
		expect(out.totalBlocks).toBe(0);
	});

	it('classifies the request-#270 shape correctly', () => {
		const blocks = [
			BLOCK_HOOKS_START,
			BLOCK_TOOLS,
			BLOCK_MCP,
			BLOCK_HOOK_SUBMIT,
			BLOCK_CLAUDE_MD,
			BLOCK_USER_PROMPT
		];
		const out = parseReminders(blocks, AGENT_SYSTEM_PROMPT);

		const byKind = new Map(out.groups.map((g) => [g.kind, g]));
		expect(byKind.get('hooks')?.blocks.length).toBe(2);
		expect(byKind.get('tools')?.blocks.length).toBe(1);
		expect(byKind.get('mcp')?.blocks.length).toBe(1);
		expect(byKind.get('claudeMd')?.blocks.length).toBe(1);
		expect(out.totalBlocks).toBe(5);
		expect(out.agentSystemPrompt).toBe(AGENT_SYSTEM_PROMPT);
	});

	it('extracts a claudeMd reminder with correct bucket split', () => {
		const out = parseReminders([BLOCK_CLAUDE_MD], null);
		const cmd = out.claudeMd as ClaudeMdReminder | null;
		expect(cmd).not.toBeNull();
		if (!cmd) return;

		const byBucket = { global: 0, project: 0, memory: 0, harness: 0 } as Record<string, number>;
		for (const s of cmd.sections) byBucket[s.bucket] = (byBucket[s.bucket] ?? 0) + 1;

		expect(byBucket.global).toBe(3);
		expect(byBucket.project).toBe(3);
		expect(byBucket.memory).toBe(1);
		expect(byBucket.harness).toBe(2);
	});

	it('assigns correct subsections per bucket', () => {
		const out = parseReminders([BLOCK_CLAUDE_MD], null);
		const sections = out.claudeMd?.sections ?? [];

		const globalClaudeMd = sections.filter(
			(s) => s.bucket === 'global' && s.subsection === 'claudeMd'
		);
		expect(globalClaudeMd.length).toBe(1);
		expect(globalClaudeMd[0]?.displayPath).toBe('~/.claude/CLAUDE.md');

		const globalRules = sections.filter((s) => s.bucket === 'global' && s.subsection === 'rules');
		expect(globalRules.length).toBe(2);

		expect(
			sections.filter((s) => s.bucket === 'project' && s.subsection === 'claudeMd').length
		).toBe(2);
		expect(sections.filter((s) => s.bucket === 'project' && s.subsection === 'rules').length).toBe(
			1
		);
		expect(sections.filter((s) => s.bucket === 'memory').length).toBe(1);
		expect(sections.filter((s) => s.bucket === 'harness').length).toBe(2);
	});

	it('abbreviates /Users/<name>/ paths to ~/', () => {
		const out = parseReminders([BLOCK_CLAUDE_MD], null);
		const sections = out.claudeMd?.sections ?? [];
		const dp = sections.map((s) => s.displayPath);
		expect(dp).toContain('~/.claude/CLAUDE.md');
		expect(dp).toContain('~/.claude/rules/rule-one.md');
	});

	it('extracts body content for each section', () => {
		const out = parseReminders([BLOCK_CLAUDE_MD], null);
		const rule1 = out.claudeMd?.sections.find((s) => s.path.endsWith('rule-one.md'));
		expect(rule1?.body).toContain('Rule one body.');
	});

	it('extracts harness variables as separate sections', () => {
		const out = parseReminders([BLOCK_CLAUDE_MD], null);
		const harness = out.claudeMd?.sections.filter((s) => s.bucket === 'harness') ?? [];
		const names = harness.map((s) => s.displayPath).sort();
		expect(names).toEqual(['currentDate', 'userEmail']);
		const userEmail = harness.find((s) => s.displayPath === 'userEmail');
		expect(userEmail?.body).toContain('alice@example.com');
	});

	it('handles null / empty agent system prompt', () => {
		const out = parseReminders([BLOCK_HOOKS_START], null);
		expect(out.agentSystemPrompt).toBeNull();
	});
});

describe('parseSystemPrompt (legacy)', () => {
	it('returns empty groups for null / empty input', () => {
		for (const v of [null, '']) {
			const out = parseSystemPrompt(v);
			expect(out.groups).toEqual([]);
			expect(out.totalChars).toBe(0);
			expect(out.totalTokens).toBe(0);
			expect(out.totalBlocks).toBe(0);
			expect(out.claudeMd).toBeNull();
		}
	});

	it('classifies blocks into hooks / mcp / tools / skills / context / other', () => {
		const raw = [
			'## Hooks\nSessionStart hook fired',
			'## MCP Servers\nclaude-in-chrome: 18 tools',
			'## Tools\nAvailable tools: Read, Write',
			'## Skills\nmanage-agents · skill-creator',
			'Contents of ~/.claude/CLAUDE.md (user global instructions)\n...',
			'Contents of /repo/CLAUDE.md (project rules)\n...',
			'# Random standalone block'
		].join('\n\n');
		const { groups } = parseSystemPrompt(raw);
		const kinds = groups.map((g) => g.kind);
		expect(kinds).toEqual([
			'hooks',
			'mcp',
			'tools',
			'skills',
			'context-global',
			'context-project',
			'other'
		]);
	});

	it('keeps prescribed order even when source order differs', () => {
		const raw = ['# Random block', '## Skills\nskill-list', '## Hooks\nhook 1'].join('\n\n');
		const kinds = parseSystemPrompt(raw).groups.map((g) => g.kind);
		expect(kinds).toEqual(['hooks', 'skills', 'other']);
	});

	it('aggregates char count / est tokens across blocks per group', () => {
		const a = '## Hooks\nhook 1 text';
		const b = '## Hooks\nhook 2 longer text';
		const raw = [a, b].join('\n\n');
		const out = parseSystemPrompt(raw);
		expect(out.groups.length).toBe(1);
		const hooks = out.groups[0];
		if (!hooks) throw new Error('expected hooks group');
		expect(hooks.blocks.length).toBe(2);
		expect(hooks.charCount).toBe(a.length + b.length);
		expect(hooks.estTokens).toBe(estimateTokens(a.length + b.length));
	});

	it('skips empty blocks produced by runs of blank lines', () => {
		const raw = '## Hooks\nA\n\n\n\n## MCP Servers\nB';
		const out = parseSystemPrompt(raw);
		expect(out.groups.length).toBe(2);
		expect(out.totalBlocks).toBe(2);
	});
});

describe('formatTokens', () => {
	it('renders <1024 as "~N tokens"', () => {
		expect(formatTokens(0)).toBe('~0 tokens');
		expect(formatTokens(8)).toBe('~8 tokens');
		expect(formatTokens(1023)).toBe('~1023 tokens');
	});
	it('renders 1024+ as "~Nk tokens" with one decimal place under 10k', () => {
		expect(formatTokens(1024)).toBe('~1k tokens');
		expect(formatTokens(1500)).toBe('~1.5k tokens');
		expect(formatTokens(5400)).toBe('~5.4k tokens');
	});
	it('renders 10k+ without a decimal', () => {
		expect(formatTokens(10000)).toBe('~10k tokens');
		expect(formatTokens(33000)).toBe('~33k tokens');
	});
});

describe('classifySubKind', () => {
	it('extracts hook name from SessionStart payload', () => {
		const sub = classifySubKind('SessionStart:startup hook success: OK');
		expect(sub.family).toBe('hook');
		if (sub.family === 'hook') expect(sub.name).toBe('SessionStart');
	});

	it('extracts hook name from UserPromptSubmit payload', () => {
		const sub = classifySubKind('UserPromptSubmit hook success: OK');
		expect(sub.family).toBe('hook');
		if (sub.family === 'hook') expect(sub.name).toBe('UserPromptSubmit');
	});

	it('classifies the MCP catalogue', () => {
		const sub = classifySubKind('# MCP Server Instructions\n\n...');
		expect(sub.family).toBe('boot');
		if (sub.family === 'boot') expect(sub.catalogue).toBe('mcp');
	});

	it('classifies the skills catalogue', () => {
		const sub = classifySubKind('The following skills are available for use with the Skill tool:');
		expect(sub.family).toBe('boot');
		if (sub.family === 'boot') expect(sub.catalogue).toBe('skills');
	});

	it('classifies the claudeMd context catalogue', () => {
		const sub = classifySubKind(
			"As you answer the user's questions, you can use the following context:\n# claudeMd\nContents of /a/CLAUDE.md (...):\n..."
		);
		expect(sub.family).toBe('boot');
		if (sub.family === 'boot') expect(sub.catalogue).toBe('context');
	});

	it('classifies a date-change event as info', () => {
		const sub = classifySubKind("The date has changed. Today's date is now 2026-04-28.");
		expect(sub.family).toBe('event');
		if (sub.family === 'event') {
			expect(sub.name).toBe('date-change');
			expect(sub.severity).toBe('info');
		}
	});

	it('classifies a stale-task-list event as warning', () => {
		const sub = classifySubKind("The task tools haven't been used recently. ...");
		expect(sub.family).toBe('event');
		if (sub.family === 'event') {
			expect(sub.name).toBe('stale-task-list');
			expect(sub.severity).toBe('warning');
		}
	});

	it('classifies a file-modified event as info', () => {
		const sub = classifySubKind('Note: /Users/x/foo.md has been modified externally...');
		expect(sub.family).toBe('event');
		if (sub.family === 'event') {
			expect(sub.name).toBe('file-modified');
			expect(sub.severity).toBe('info');
		}
	});

	it('falls back to catchall for unknown reminder shapes', () => {
		const sub = classifySubKind('Some completely unrecognised text body.');
		expect(sub.family).toBe('catchall');
	});
});

describe('parseReminders subKind population', () => {
	it('populates subKind on every block', () => {
		const out = parseReminders(
			[BLOCK_HOOKS_START, BLOCK_MCP, BLOCK_HOOK_SUBMIT, BLOCK_CLAUDE_MD],
			null
		);
		const blocks = out.groups.flatMap((g) => g.blocks);
		for (const b of blocks) {
			expect(b.subKind).toBeDefined();
			expect(b.subKind.family).toBeDefined();
		}
		const hooks = blocks.filter((b) => b.subKind.family === 'hook');
		expect(hooks.length).toBe(2);
	});
});
