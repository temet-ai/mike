import type { query as QueryFn } from '@anthropic-ai/claude-agent-sdk';
import { buildSdkEnv } from './env.js';
import { buildCanUseTool, BUILTIN_TOOL_NAMES } from './sandbox.js';
import type { InspectClient } from '../inspectClient.js';
import { buildToolServer } from '../mcp/tools.js';
import type { AssistantScope, ConversationMessage, PromptAssistantSettings, Proposal, SseEvent } from '../types.js';

/** How often (real wall-clock ms), at most, a running turn re-checks the daily budget. */
const BUDGET_CHECK_INTERVAL_MS = 2000;

export interface RunTurnParams {
	queryFn: typeof QueryFn;
	inspectClient: InspectClient;
	settings: PromptAssistantSettings;
	requestId: number;
	priorMessages: ConversationMessage[];
	userText: string;
	quote?: string;
	/** 'all' tells the assistant to check the operator's other projects before answering; defaults to 'project'. */
	scope?: AssistantScope;
	/**
	 * The project the workbench has picked, when it differs from (or isn't yet known to be) the
	 * one `requestId` itself belongs to — e.g. the operator picked a project other than "Global"
	 * on the filter workbench's project picker. Only meaningful with `scope: 'project'`; `runTurn`
	 * tells the model to call `read_project_instructions` with this path explicitly rather than
	 * falling back to its request_id-derived default.
	 */
	project_path?: string;
	captureBaseUrl: string;
	claudeHome: string;
	/** Neutral cwd for the SDK subprocess — the service's own state dir, never a project checkout (G1). */
	stateDir: string;
	/** Aborts the SDK query. Callers tie this to the HTTP response's `close` event so a client disconnect stops the turn, and `runTurn` itself aborts it when `checkBudget` reports the budget is gone (G13). */
	abortController: AbortController;
	/**
	 * Re-checked periodically while the turn is running (not just once before
	 * it starts), so a second, concurrent turn spending the last of the day's
	 * budget aborts this one instead of letting both overshoot. Optional so
	 * existing callers/tests that don't care about budget can omit it.
	 */
	checkBudget?: () => Promise<boolean>;
	emit: (event: SseEvent) => void;
	/**
	 * The resolved Anthropic auth mode/key for this turn (from
	 * `InspectClient.getAnthropicCredentials()`). Optional so existing
	 * callers/tests that don't care about it keep compiling — when omitted,
	 * `buildSdkEnv` gets no override and falls back to its existing
	 * `claude_code_login` allowlist pass-through.
	 */
	anthropicCredentials?: { mode: 'claude_code_login' | 'api_key'; api_key: string | null };
}

export interface RunTurnResult {
	text: string;
	costUsd: number;
	proposals: Proposal[];
	error: string | null;
}

/** Renders the conversation-so-far as a plain transcript the model reads as prior turns. */
export function renderTranscript(priorMessages: ConversationMessage[], userText: string, quote?: string): string {
	const lines: string[] = [];
	for (const m of priorMessages) {
		const speaker = m.role === 'user' ? 'Operator' : 'Assistant';
		if (m.role === 'user' && m.quote) {
			lines.push(`Operator quoted:\n"""\n${m.quote}\n"""`);
		}
		lines.push(`${speaker}: ${m.text}`);
	}
	if (quote) {
		lines.push(`Operator quoted:\n"""\n${quote}\n"""`);
	}
	lines.push(`Operator: ${userText}`);
	return lines.join('\n\n');
}

function scopeInstruction(scope: AssistantScope | undefined, projectPath: string | undefined): string {
	if (scope === 'all') {
		return "Scope for this message: All projects. Before recommending anything, use list_projects and compare_across_projects to check whether the operator's other projects agree or conflict, and say so even when nothing conflicts.";
	}
	if (projectPath !== undefined) {
		return `Scope for this message: the project at ${projectPath} — this is what the operator picked on the workbench, and may not be the project request_id belongs to. Call read_project_instructions with project_path="${projectPath}" to read ITS CLAUDE.md/rules, not the request_id default. Do not call list_projects or compare_across_projects unless the operator explicitly asks about another project.`;
	}
	return 'Scope for this message: this project only. Do not call list_projects or compare_across_projects unless the operator explicitly asks about another project.';
}

function isTextDelta(
	event: unknown
): event is { type: 'content_block_delta'; delta: { type: 'text_delta'; text: string } } {
	return (
		typeof event === 'object' &&
		event !== null &&
		(event as { type?: unknown }).type === 'content_block_delta' &&
		typeof (event as { delta?: unknown }).delta === 'object' &&
		(event as { delta?: { type?: unknown } }).delta !== null &&
		(event as { delta: { type?: unknown } }).delta.type === 'text_delta'
	);
}

/**
 * Runs one conversational turn against the Claude Agent SDK, sandboxed (G1):
 * no user/project settings, CLAUDE.md, output styles, plugins, or MCP server
 * other than its own in-process one, and a deny-by-default `canUseTool` on
 * top of `tools: []` and `disallowedTools` as belt-and-braces guards. Streams
 * `text` and `status`/`proposal`/`applied` events (the latter three are
 * emitted by the tool handlers themselves, via the same `emit` callback) as
 * they occur, and resolves once the SDK's `result` message lands — or the
 * turn is aborted (budget exhausted mid-turn, or the caller's `abortController`
 * fires) or the underlying stream throws, in which case it still resolves
 * with whatever partial `text`/`costUsd` were seen and a non-null `error`,
 * rather than throwing (G13: a turn's spend must be recorded even when it
 * ends badly).
 */
export async function runTurn(params: RunTurnParams): Promise<RunTurnResult> {
	const issuedProposals: Proposal[] = [];
	const { mcpServers, allowedTools } = buildToolServer({
		inspectClient: params.inspectClient,
		settings: params.settings,
		requestId: params.requestId,
		emit: params.emit,
		issuedProposals,
		claudeHome: params.claudeHome
	});

	const prompt = renderTranscript(params.priorMessages, params.userText, params.quote);
	const env = buildSdkEnv(process.env, {
		capture: params.settings.capture,
		captureBaseUrl: params.captureBaseUrl,
		anthropicApiKeyOverride:
			params.anthropicCredentials?.mode === 'api_key' ? params.anthropicCredentials.api_key : undefined
	});

	const q = params.queryFn({
		prompt,
		options: {
			model: params.settings.model,
			effort: params.settings.effort,
			maxTurns: params.settings.max_turns,
			tools: [],
			allowedTools,
			disallowedTools: [...BUILTIN_TOOL_NAMES],
			canUseTool: buildCanUseTool(allowedTools),
			mcpServers,
			strictMcpConfig: true,
			settingSources: [],
			cwd: params.stateDir,
			systemPrompt: {
				type: 'custom',
				prompt: `${params.settings.instructions}\n\nThe operator is looking at request_id ${params.requestId} right now. Use that request_id for get_prompt, dry_run, and read_project_instructions unless they name a different one.\n\n${scopeInstruction(params.scope, params.project_path)}`,
				snapshot: false
			},
			env,
			abortController: params.abortController,
			includePartialMessages: true,
			permissionMode: 'default',
			// The operator's message is composed by this service, not typed into a
			// CLI, and `settingSources: []` alone does not stop it: the CLI's
			// turn-start "attachment pass" still injects environment/userEmail/date
			// system-reminders into the first user message on every turn,
			// independent of file-based settings. `verbatimPrompts` skips that pass
			// (G1 — a live-verified request otherwise carried the operator's own
			// email address and cwd despite the sandbox already being in place).
			verbatimPrompts: true
		}
	});

	let text = '';
	let costUsd = 0;
	let resultSeen = false;
	let resultError: string | null = null;
	let lastBudgetCheckMs = 0;

	try {
		for await (const message of q) {
			if (params.checkBudget) {
				const nowMs = Date.now();
				if (nowMs - lastBudgetCheckMs >= BUDGET_CHECK_INTERVAL_MS) {
					lastBudgetCheckMs = nowMs;
					const withinBudget = await params.checkBudget();
					if (!withinBudget) {
						resultError = "today's prompt-assistant budget was used up while this turn was running";
						params.emit({ event: 'error', data: { message: resultError } });
						params.abortController.abort();
						break;
					}
				}
			}
			if (message.type === 'stream_event' && isTextDelta(message.event)) {
				const delta = message.event.delta.text;
				text += delta;
				params.emit({ event: 'text', data: { delta } });
				continue;
			}
			if (message.type === 'result') {
				resultSeen = true;
				costUsd = message.total_cost_usd;
				if (message.subtype === 'success') {
					if (message.is_error) {
						resultError = message.result.length > 0 ? message.result : 'the assistant turn ended in an error';
					}
				} else {
					resultError = message.errors.length > 0 ? message.errors.join('; ') : `the assistant turn ended: ${message.subtype}`;
				}
			}
		}
	} catch (err) {
		resultError = resultError ?? (err as Error).message;
	}

	if (!resultSeen && resultError === null) {
		resultError = 'the assistant turn ended without a result';
	}

	return { text, costUsd, proposals: issuedProposals, error: resultError };
}
