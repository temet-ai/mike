import type { CanUseTool } from '@anthropic-ai/claude-agent-sdk';

/**
 * Claude Code's built-in tool names. `tools: []` in the SDK `Options` already
 * disables every one of these (G1's primary guard), and `strictMcpConfig` +
 * `settingSources: []` keep out any MCP server/tool this service did not
 * register itself. `disallowedTools` below is a deliberate second, redundant
 * guard, and `canUseTool` (also below) is the third: deny-by-default, so a
 * future SDK version that changes what an empty `tools: []` disables, or a
 * bug that widens `allowedTools`, still cannot let the agent touch the
 * filesystem, the network, or any `mcp__mike__*`/other-server tool.
 */
export const BUILTIN_TOOL_NAMES = [
	'Bash',
	'BashOutput',
	'KillShell',
	'Read',
	'Write',
	'Edit',
	'MultiEdit',
	'NotebookEdit',
	'Glob',
	'Grep',
	'WebFetch',
	'WebSearch',
	'Task',
	'TodoWrite',
	'ExitPlanMode',
	'Skill',
	'SlashCommand'
] as const;

/**
 * Deny-by-default permission callback: allows only the tool names this turn
 * registered on its own in-process MCP server (`mcp__prompt_assistant__*`),
 * denying everything else — every built-in, and every other MCP server's
 * tools (`mcp__mike__*`, claude.ai Docs, storybook, …) that a misconfigured
 * `strictMcpConfig`/`settingSources` might otherwise still expose.
 */
export function buildCanUseTool(allowedTools: string[]): CanUseTool {
	const allowed = new Set(allowedTools);
	return async (toolName, input) => {
		if (allowed.has(toolName)) {
			return { behavior: 'allow', updatedInput: input };
		}
		return {
			behavior: 'deny',
			message: `The prompt assistant does not have the "${toolName}" tool — only ${Array.from(allowed).join(', ')} are available.`
		};
	};
}
