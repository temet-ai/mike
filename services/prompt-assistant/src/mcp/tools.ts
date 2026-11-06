import { randomUUID } from 'node:crypto';
import { createSdkMcpServer, tool } from '@anthropic-ai/claude-agent-sdk';
import type { SdkMcpToolDefinition } from '@anthropic-ai/claude-agent-sdk';
import { z } from 'zod';
import type { InspectClient } from '../inspectClient.js';
import type {
	FilterRuleDto,
	PromptAssistantSettings,
	Proposal,
	ProposalAction,
	SseEvent
} from '../types.js';
import { computeAfter, computeBefore, concatSystemBlocks } from './proposals.js';
import { readProjectInstructions } from './projectFiles.js';
import { readProjectMemory } from './memory.js';
import { buildCompareAcrossProjectsTool, buildListProjectsTool } from './crossProject.js';

export const MCP_SERVER_NAME = 'prompt_assistant';

export interface ToolContext {
	inspectClient: InspectClient;
	settings: PromptAssistantSettings;
	requestId: number;
	emit: (event: SseEvent) => void;
	/** propose_edit pushes every proposal it records here, so the caller can persist them after the turn. */
	issuedProposals: Proposal[];
	/** `~/.claude` — root the `read_memory` tool reads `projects/<slug>/memory/*.md` under. */
	claudeHome: string;
}

function textResult(payload: unknown): { content: Array<{ type: 'text'; text: string }> } {
	return { content: [{ type: 'text', text: JSON.stringify(payload) }] };
}
function errorResult(message: string): { content: Array<{ type: 'text'; text: string }>; isError: true } {
	return { content: [{ type: 'text', text: message }], isError: true };
}

async function recordProposal(
	ctx: ToolContext,
	args: {
		title: string;
		edit_id: string | null;
		action: ProposalAction;
		match: string;
		text: string;
		reach: string;
		project_path: string | null;
	}
): Promise<Proposal> {
	let before = '';
	try {
		const request = await ctx.inspectClient.getRequest(ctx.requestId);
		const haystack = `${concatSystemBlocks(request.system_blocks?.blocks)}\n${request.last_user_message ?? ''}`;
		before = computeBefore(haystack, args.match);
	} catch {
		before = '';
	}
	const after = computeAfter(args.action, args.match, args.text);
	// The raw text to persist on the filter rule — never the match-concatenated
	// `after` used for display. Mirrors exactly what the apply payload below
	// sends as `text`, so a caller previewing/applying this proposal writes the
	// same rule the service's own auto-apply would.
	const ruleText = args.action === 'drop' ? null : args.text;
	const proposal: Proposal = {
		id: randomUUID(),
		title: args.title,
		edit_id: args.edit_id,
		action: args.action,
		match: args.match,
		rule_text: ruleText,
		before,
		after,
		reach: args.reach,
		project_path: args.project_path,
		created_at: new Date().toISOString(),
		applied: false
	};
	ctx.issuedProposals.push(proposal);
	ctx.emit({
		event: 'proposal',
		data: {
			id: proposal.id,
			title: proposal.title,
			edit_id: proposal.edit_id,
			action: proposal.action,
			match: proposal.match,
			rule_text: proposal.rule_text,
			before: proposal.before,
			after: proposal.after,
			reach: proposal.reach,
			project_path: proposal.project_path
		}
	});

	if (ctx.settings.may_apply === 'apply') {
		try {
			const payload: Record<string, unknown> = {
				kind: 'text_substring' as const,
				value: args.match,
				action: args.action,
				text: ruleText,
				enabled: true,
				title: args.title
			};
			if (args.project_path !== null) {
				payload.project_path = args.project_path;
			}
			const response = args.edit_id
				? await ctx.inspectClient.updateFilterRule(args.edit_id, payload)
				: await ctx.inspectClient.createFilterRule(payload);
			proposal.applied = true;
			proposal.applied_rule_id = response.rule.id;
			ctx.emit({ event: 'applied', data: { proposal_id: proposal.id, rule_id: response.rule.id } });
		} catch (err) {
			ctx.emit({ event: 'error', data: { message: `could not apply proposal: ${(err as Error).message}` } });
		}
	}
	return proposal;
}

function buildGetPromptTool(ctx: ToolContext): SdkMcpToolDefinition<any> {
	return tool(
		'get_prompt',
		"Return the system blocks and session message mike captured for a request.",
		{ request_id: z.number().int() },
		async ({ request_id }) => {
			ctx.emit({ event: 'status', data: { phase: 'reading prompt' } });
			try {
				const request = await ctx.inspectClient.getRequest(request_id);
				return textResult({
					system_blocks: request.system_blocks?.blocks ?? [],
					session_message: request.last_user_message,
					project: request.project
				});
			} catch (err) {
				return errorResult(`could not read request ${request_id}: ${(err as Error).message}`);
			}
		}
	);
}

function buildListEditsTool(ctx: ToolContext): SdkMcpToolDefinition<any> {
	return tool(
		'list_edits',
		'List the filter rules (title, match, action, text, enabled, fire counts) that rewrite captured prompts.',
		{},
		async () => {
			ctx.emit({ event: 'status', data: { phase: 'reading edits' } });
			try {
				const rules = await ctx.inspectClient.listFilterRules();
				return textResult(
					rules.map((r) => ({
						id: r.id,
						title: r.title ?? null,
						kind: r.kind,
						match: r.value,
						action: r.action,
						text: r.text,
						enabled: r.enabled,
						fires_24h: r.fires_24h,
						fires_7d: r.fires_7d
					}))
				);
			} catch (err) {
				return errorResult(`could not list filter rules: ${(err as Error).message}`);
			}
		}
	);
}

const candidateRuleSchema = z.object({
	id: z.string().optional(),
	kind: z.enum(['text_substring', 'xml_tag', 'lazy_section', 'inject_reminder']),
	value: z.string(),
	action: z.enum(['drop', 'replace', 'append']),
	text: z.string().nullable().optional(),
	enabled: z.boolean().optional(),
	trigger_token: z.string().nullable().optional()
});

function buildDryRunTool(ctx: ToolContext): SdkMcpToolDefinition<any> {
	return tool(
		'dry_run',
		"Run the filter engine's dry-run for a candidate rule set against a request and see the resulting prompt.",
		{ request_id: z.number().int(), rules: z.array(candidateRuleSchema) },
		async ({ request_id, rules }) => {
			ctx.emit({ event: 'status', data: { phase: 'dry-run' } });
			try {
				const live = await ctx.inspectClient.listFilterRules();
				const liveById = new Map(live.map((r) => [r.id, r]));
				const fullRules: FilterRuleDto[] = rules.map((r, idx) => {
					const existing = r.id ? liveById.get(r.id) : undefined;
					return {
						id: r.id ?? `candidate-${idx}`,
						kind: r.kind,
						value: r.value,
						action: r.action,
						text: r.text ?? null,
						enabled: r.enabled ?? true,
						source: existing?.source ?? 'ui',
						updated_at: existing?.updated_at ?? new Date().toISOString(),
						trigger_token: r.trigger_token ?? existing?.trigger_token ?? null,
						sort_order: existing?.sort_order ?? idx,
						fires_24h: existing?.fires_24h ?? 0,
						fires_7d: existing?.fires_7d ?? 0,
						last_fired_at: existing?.last_fired_at ?? null
					};
				});
				const result = await ctx.inspectClient.dryRunFilterRules(request_id, fullRules);
				return textResult(result);
			} catch (err) {
				return errorResult(`dry-run failed: ${(err as Error).message}`);
			}
		}
	);
}

function buildReadProjectInstructionsTool(ctx: ToolContext): SdkMcpToolDefinition<any> {
	return tool(
		'read_project_instructions',
		'Read CLAUDE.md and .claude/rules/*.md (read-only, size-capped). Pass request_id for the project the operator is looking at right now (the default), or project_path for any other project list_projects reported — project_path must be one of those known paths, not an arbitrary filesystem location.',
		{ request_id: z.number().int().optional(), project_path: z.string().optional() },
		async ({ request_id, project_path }) => {
			ctx.emit({ event: 'status', data: { phase: 'reading project instructions' } });
			try {
				const resolved = await resolveProjectPath(ctx, request_id, project_path);
				if ('error' in resolved) {
					return errorResult(resolved.error);
				}
				if (!resolved.projectPath) {
					return textResult({ project: null, files: [] });
				}
				const files = await readProjectInstructions(resolved.projectPath);
				return textResult({ project: resolved.projectPath, files });
			} catch (err) {
				return errorResult(`could not read project instructions: ${(err as Error).message}`);
			}
		}
	);
}

async function resolveProjectPath(
	ctx: ToolContext,
	requestId: number | undefined,
	projectPath: string | undefined
): Promise<{ projectPath: string | null } | { error: string }> {
	if (projectPath !== undefined) {
		const known = await ctx.inspectClient.listProjects();
		if (!known.some((p) => p.project_path === projectPath)) {
			return { error: `project_path "${projectPath}" is not a project mike has captured traffic for` };
		}
		return { projectPath };
	}
	const request = await ctx.inspectClient.getRequest(requestId ?? ctx.requestId);
	return { projectPath: request.project };
}

function buildReadMemoryTool(ctx: ToolContext): SdkMcpToolDefinition<any> {
	return tool(
		'read_memory',
		"Read the project's Claude Code memory (~/.claude/projects/<slug>/memory/*.md, MEMORY.md first) for the request's project (read-only, size-capped).",
		{ request_id: z.number().int() },
		async ({ request_id }) => {
			ctx.emit({ event: 'status', data: { phase: 'reading memory' } });
			try {
				const request = await ctx.inspectClient.getRequest(request_id);
				if (!request.project) {
					return textResult({ project: null, files: [] });
				}
				const files = await readProjectMemory(ctx.claudeHome, request.project);
				return textResult({ project: request.project, files });
			} catch (err) {
				return errorResult(`could not read project memory: ${(err as Error).message}`);
			}
		}
	);
}

function buildListPromptVersionsTool(ctx: ToolContext): SdkMcpToolDefinition<any> {
	return tool(
		'list_prompt_versions',
		'List the prompt-class catalogue mike has observed: lanes, models seen, and observation counts.',
		{ model: z.string().optional() },
		async ({ model }) => {
			ctx.emit({ event: 'status', data: { phase: 'reading prompt versions' } });
			try {
				const classes = await ctx.inspectClient.listPromptClasses();
				const filtered = model
					? classes.filter((c) => c.model_breakdown.some((mb) => mb.model === model))
					: classes;
				return textResult(filtered);
			} catch (err) {
				return errorResult(`could not list prompt versions: ${(err as Error).message}`);
			}
		}
	);
}

function buildProposeEditTool(ctx: ToolContext): SdkMcpToolDefinition<any> {
	return tool(
		'propose_edit',
		'Record a proposed prompt edit as a before/after pair and stream it to the operator. Never applies it yourself unless the operator has switched may_apply to apply.',
		{
			title: z.string(),
			edit_id: z.string().nullable().optional(),
			action: z.enum(['replace', 'append', 'drop']),
			match: z.string(),
			text: z.string(),
			reach: z.string(),
			project_path: z
				.string()
				.nullable()
				.optional()
				.describe(
					'Set this to scope the proposal to one project (from list_projects) instead of the whole shared prompt — use it when a conflict found by compare_across_projects is specific to that project.'
				)
		},
		async ({ title, edit_id, action, match, text, reach, project_path }) => {
			const proposal = await recordProposal(ctx, {
				title,
				edit_id: edit_id ?? null,
				action,
				match,
				text,
				reach,
				project_path: project_path ?? null
			});
			return textResult({ proposal_id: proposal.id, applied: proposal.applied });
		}
	);
}

/**
 * Builds the in-process MCP server for one turn. Only the tools whose
 * matching `read_*` setting is on are registered — see the spec's "Tools are
 * only registered when the matching read_* setting is on." `dry_run` reads
 * both the prompt and the edits, so it is gated on both `read_prompt` and
 * `read_edits` (the spec names five read_* settings for six tools and does
 * not spell out dry_run's own mapping). `read_other_projects` gates
 * `list_projects` and `compare_across_projects` together, the two tools that
 * check the operator's other captured projects before a global edit ships.
 * `propose_edit` is always registered: it only records/streams a proposal, it
 * does not read privileged state itself — the `before` text it publishes
 * comes from a direct inspectClient lookup, not from a tool the model chose
 * to call.
 */
export function buildTools(ctx: ToolContext): SdkMcpToolDefinition<any>[] {
	const tools: SdkMcpToolDefinition<any>[] = [];
	if (ctx.settings.read_prompt) tools.push(buildGetPromptTool(ctx));
	if (ctx.settings.read_edits) tools.push(buildListEditsTool(ctx));
	if (ctx.settings.read_prompt && ctx.settings.read_edits) tools.push(buildDryRunTool(ctx));
	if (ctx.settings.read_project_instructions) tools.push(buildReadProjectInstructionsTool(ctx));
	if (ctx.settings.read_memory) tools.push(buildReadMemoryTool(ctx));
	if (ctx.settings.read_versions) tools.push(buildListPromptVersionsTool(ctx));
	if (ctx.settings.read_other_projects) {
		tools.push(buildListProjectsTool(ctx));
		tools.push(buildCompareAcrossProjectsTool(ctx));
	}
	tools.push(buildProposeEditTool(ctx));
	return tools;
}

export function buildToolServer(ctx: ToolContext): {
	mcpServers: Record<string, ReturnType<typeof createSdkMcpServer>>;
	allowedTools: string[];
} {
	const tools = buildTools(ctx);
	const server = createSdkMcpServer({ name: MCP_SERVER_NAME, version: '0.1.0', tools });
	const allowedTools = tools.map((t) => `mcp__${MCP_SERVER_NAME}__${t.name}`);
	return { mcpServers: { [MCP_SERVER_NAME]: server }, allowedTools };
}
