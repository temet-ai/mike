<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import SystemPromptBreakdown from './SystemPromptBreakdown.svelte';
	import type { SystemBlock, SystemBlocksProjection } from '$lib/types';

	function buildBlock(
		index: number,
		text: string,
		composition_match: SystemBlock['composition_match'] = null,
		cache_control: SystemBlock['cache_control'] = null
	): SystemBlock {
		return { index, text, cache_control, composition_match };
	}

	function buildProjection(
		prompt_sha256: string | null,
		blocks: SystemBlock[]
	): SystemBlocksProjection {
		return { prompt_sha256, composition_id: null, composition_version: null, blocks };
	}

	const LONG_BLOCK_TEXT = `You are Claude Code, Anthropic's official CLI for Claude, running within the Claude Agent SDK.
You are an interactive agent that helps users with software engineering tasks. Use the instructions below and the tools available to you to assist the user.

IMPORTANT: Assist with authorized security testing, defensive security, CTF challenges, and educational contexts. Refuse requests for destructive techniques, DoS attacks, mass targeting, supply chain compromise, or detection evasion for malicious purposes.

# System
 - All text you output outside of tool use is displayed to the user. Output text to communicate with the user. You can use Github-flavored markdown for formatting, and will be rendered in a monospace font using the CommonMark specification.
 - Tools are executed in a user-selected permission mode. When you attempt to call a tool that is not automatically allowed by the user's permission mode or permission settings, the user will be prompted so that they can approve or deny the execution.

# Doing tasks
 - The user will primarily request you to perform software engineering tasks. These may include solving bugs, adding new functionality, refactoring code, explaining code, and more.
 - You are highly capable and often allow users to complete ambitious tasks that would otherwise be too complex or take too long.
 - For exploratory questions, respond in 2-3 sentences with a recommendation and the main tradeoff.
 - Prefer editing existing files to creating new ones.
 - Be careful not to introduce security vulnerabilities such as command injection, XSS, SQL injection, and other OWASP top 10 vulnerabilities.

# Tone and style
 - Only use emojis if the user explicitly requests it. Avoid using emojis in all communication unless asked.
 - Your responses should be short and concise.
 - Do not use a colon before tool calls.`;

	const SECOND_BLOCK_TEXT = `# Environment
You have been invoked in the following environment:
 - Primary working directory: /Users/operator/projects/ai/temet-proxy
 - Is a git repository: true
 - Platform: darwin
 - Shell: zsh
 - OS Version: Darwin 25.4.0

# Session-specific guidance
 - If you need the user to run a shell command themselves, suggest they type ! <command> in the prompt.
 - Use the Agent tool with specialized agents when the task at hand matches the agent description.
 - For broad codebase exploration or research that will take more than 3 queries, spawn Agent with subagent_type=Explore.`;

	const TYPICAL_BLOCKS = [
		buildBlock(0, LONG_BLOCK_TEXT, 'matched', { type: 'ephemeral' }),
		buildBlock(1, SECOND_BLOCK_TEXT, 'matched')
	];

	const COMPOSED_PROJECTION = buildProjection(
		'a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2',
		TYPICAL_BLOCKS
	);

	const { Story } = defineMeta({
		title: 'Components/Detail/SystemPromptBreakdown',
		component: SystemPromptBreakdown,
		tags: ['autodocs']
	});
</script>

<Story
	name="Typical"
	args={{ blocks: TYPICAL_BLOCKS, systemBlocks: COMPOSED_PROJECTION, requestId: 'req-001' }}
>
	{#snippet template(args)}
		<div class="p-4 max-w-[700px]">
			<SystemPromptBreakdown {...args} />
		</div>
	{/snippet}
</Story>
