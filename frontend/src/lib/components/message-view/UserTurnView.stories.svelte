<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import UserTurnView from './UserTurnView.svelte';
	import { parseReminders, parseSystemPrompt } from './parseSystemPrompt';

	const CLAUDE_MD_BLOCK = `<system-reminder>
As you answer the user's questions, you can use the following context:
# claudeMd
Codebase and user instructions are shown below. Be sure to adhere to these instructions. IMPORTANT: These instructions OVERRIDE any default behavior and you MUST follow them exactly as written.

Contents of /Users/alice/.claude/CLAUDE.md (user's private global instructions for all projects):

# Global Instructions

## Mandatory Agent Delegation

You have specialized agents. You will use these agents to do the work.

Contents of /Users/alice/.claude/rules/agent-memory-hygiene.md (user's private global instructions for all projects):

# Agent Memory Hygiene

When you have a persistent memory directory, follow these rules strictly.

Contents of /Users/alice/.claude/rules/scope-and-agents.md (user's private global instructions for all projects):

# Scope Boundaries & Agent Guards

Before making multi-file changes, list the EXACT files you plan to modify and get confirmation.

Contents of /Users/alice/work/ai/tools/CLAUDE.md (project instructions, checked into the codebase):

# ai/tools

Monorepo of AI agent tools and infrastructure utilities. All Python projects use uv.

Contents of /Users/alice/work/ai/tools/proxy/CLAUDE.md (project instructions, checked into the codebase):

# Claude Proxy — Project Instructions

A Rust reverse proxy for Claude API traffic. Three ports, two Rust binaries, one SPA process.

Contents of /Users/alice/work/ai/tools/proxy/.claude/rules/testing.md (project instructions, checked into the codebase):

# Testing Rules — Claude API Proxy

Every PR must maintain passing coverage of 10 mandatory tests.

Contents of /Users/alice/.claude/projects/-Users-alice-work-proxy/memory/MEMORY.md (user's auto-memory, persists across conversations):

## Feedback

- Docs are living truth | Verify watchdog findings before relaying
# userEmail
The user's email address is alice@example.com.
# currentDate
Today's date is 2026-04-19.
</system-reminder>`;

	const SAMPLE_USER_BLOCKS = [
		'<system-reminder>\nSessionStart:startup hook success: OK\n</system-reminder>',
		'<system-reminder>\nThe following deferred tools are now available via ToolSearch. Their schemas are NOT loaded — calling them directly will fail with InputValidationError.\n</system-reminder>',
		`<system-reminder>
# MCP Server Instructions

The following MCP servers have provided instructions for how to use their tools and resources:

## mike
Read-only tools and resources backed by a running mike inspect bind.
</system-reminder>`,
		'<system-reminder>\nUserPromptSubmit hook success: OK\n</system-reminder>',
		CLAUDE_MD_BLOCK,
		'I want you to go to the project docs and tell me what you see.'
	];

	const AGENT_SP =
		"You are Claude Code, Anthropic's official CLI for Claude. You are a Frontend Development Expert...";

	const parsedFull = parseReminders(SAMPLE_USER_BLOCKS, AGENT_SP);
	const parsedEmpty = parseSystemPrompt(null);

	const SKILLS_BLOCK =
		'<system-reminder>\nThe following skills are available for use with the Skill tool:\n\n- update-config: Configure the harness via settings.json.\n- managing-hooks: Create, debug, or understand Claude Code hooks.\n</system-reminder>';
	const ALL_KINDS_BLOCKS = [
		SAMPLE_USER_BLOCKS[0] as string,
		SAMPLE_USER_BLOCKS[1] as string,
		SAMPLE_USER_BLOCKS[2] as string,
		SAMPLE_USER_BLOCKS[3] as string,
		SKILLS_BLOCK,
		CLAUDE_MD_BLOCK,
		SAMPLE_USER_BLOCKS[5] as string
	];
	const parsedAllKinds = parseReminders(ALL_KINDS_BLOCKS, AGENT_SP);

	const USER_MSG = SAMPLE_USER_BLOCKS[5] as string;

	const { Story } = defineMeta({
		title: 'Components/MessageView/UserTurnView',
		component: UserTurnView,
		tags: ['autodocs']
	});
</script>

<Story
	name="Full turn"
	args={{
		parsed: parsedFull,
		userMessage: USER_MSG
	}}
>
	{#snippet template(args)}
		<div style="padding: 40px; background: var(--surface-bg); max-width: 960px;">
			<UserTurnView {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="No system prompt"
	args={{
		parsed: parsedEmpty,
		userMessage: USER_MSG
	}}
>
	{#snippet template(args)}
		<div style="padding: 40px; background: var(--surface-bg); max-width: 960px;">
			<UserTurnView {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="All reminder kinds"
	args={{
		parsed: parsedAllKinds,
		userMessage: USER_MSG
	}}
>
	{#snippet template(args)}
		<div style="padding: 40px; background: var(--surface-bg); max-width: 960px;">
			<UserTurnView {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Flat chrome (renderCardChrome=false)"
	args={{
		parsed: parsedAllKinds,
		userMessage: USER_MSG,
		renderCardChrome: false
	}}
>
	{#snippet template(args)}
		<div style="padding: 40px; background: var(--surface-bg); max-width: 960px;">
			<UserTurnView {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="No user message (renderUserMessage=false)"
	args={{
		parsed: parsedAllKinds,
		userMessage: USER_MSG,
		renderUserMessage: false
	}}
>
	{#snippet template(args)}
		<div style="padding: 40px; background: var(--surface-bg); max-width: 960px;">
			<UserTurnView {...args} />
		</div>
	{/snippet}
</Story>
