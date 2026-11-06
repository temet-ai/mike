import type { ServerResponse } from 'node:http';
import type { query as QueryFn } from '@anthropic-ai/claude-agent-sdk';
import { runTurn } from '../agent/runTurn.js';
import { runCodexTurn } from '../agent/runCodexTurn.js';
import { readCodexAccountStatus, type CodexAccountStatus } from '../codex/appServer.js';
import type { Config } from '../config.js';
import type { InspectClient } from '../inspectClient.js';
import { resolvePromptAssistantSettings } from '../settings.js';
import { addSpend, canSpend, spentToday } from '../store/budget.js';
import {
	appendMessage,
	createConversation,
	getConversation,
	newMessage
} from '../store/conversations.js';
import type { AssistantScope, SseEvent } from '../types.js';
import { startSse, writeSse } from './sse.js';

export interface AppDeps {
	config: Config;
	inspectClient: InspectClient;
	queryFn: typeof QueryFn;
	/** Injectable clock, so budget/sweep tests don't depend on wall-clock time. */
	now: () => Date;
}

export interface JsonResult {
	status: number;
	body: unknown;
}

export interface HealthPayload {
	ok: true;
	enabled: boolean;
	model: string;
	effort: string;
	may_apply: string;
	spent_today_usd: number;
	daily_budget_usd: number;
	auth_mode: 'claude_code_login' | 'api_key';
	credentials_available: boolean;
	provider: 'anthropic' | 'codex';
}

/**
 * Degrade-gracefully fetch of the Anthropic auth mode/key, matching the
 * `getSettings()`/`getFeatures()` pattern in `settings.ts`: the credentials
 * route may 404 on an older/not-yet-restarted mike, so a failure falls back
 * to the default `claude_code_login` mode with no key.
 */
async function resolveAnthropicCredentials(
	inspectClient: AppDeps['inspectClient']
): Promise<{ mode: 'claude_code_login' | 'api_key'; api_key: string | null }> {
	return inspectClient
		.getAnthropicCredentials()
		.catch(() => ({ mode: 'claude_code_login' as const, api_key: null }));
}

export async function handleHealth(deps: AppDeps): Promise<HealthPayload> {
	const settings = await resolvePromptAssistantSettings(deps.inspectClient);
	const spent = await spentToday(deps.config.stateDir, deps.now());
	const creds = await resolveAnthropicCredentials(deps.inspectClient);
	return {
		ok: true,
		enabled: settings.enabled,
		model: settings.provider === 'codex' ? settings.codex_model : settings.model,
		effort: settings.effort,
		may_apply: settings.may_apply,
		spent_today_usd: spent,
		daily_budget_usd: settings.daily_budget_usd,
		auth_mode: creds.mode,
		credentials_available:
			settings.provider === 'codex'
				? true
				: creds.mode === 'claude_code_login'
					? true
					: creds.api_key !== null,
		provider: settings.provider
	};
}

export async function handleCodexStatus(
	deps: AppDeps
): Promise<CodexAccountStatus & { selected: boolean }> {
	const [status, settings] = await Promise.all([
		readCodexAccountStatus(),
		resolvePromptAssistantSettings(deps.inspectClient)
	]);
	return { ...status, selected: settings.provider === 'codex' };
}

export async function handleCreateConversation(deps: AppDeps, body: unknown): Promise<JsonResult> {
	const requestId = (body as { request_id?: unknown } | null)?.request_id;
	if (typeof requestId !== 'number' || !Number.isInteger(requestId)) {
		return { status: 400, body: { error: 'request_id must be an integer' } };
	}
	const conversation = await createConversation(deps.config.stateDir, requestId);
	return { status: 200, body: { id: conversation.id } };
}

export async function handleGetConversation(deps: AppDeps, id: string): Promise<JsonResult> {
	const conversation = await getConversation(deps.config.stateDir, id);
	if (conversation === null) {
		return { status: 404, body: { error: 'conversation not found' } };
	}
	return { status: 200, body: conversation };
}

interface ParsedMessageBody {
	text: string;
	quote?: string;
	scope: AssistantScope;
	project_path?: string;
}

function parseMessageBody(body: unknown): ParsedMessageBody | { error: string } {
	const text = (body as { text?: unknown } | null)?.text;
	if (typeof text !== 'string' || text.trim().length === 0) {
		return { error: 'text must be a non-empty string' };
	}
	const quoteRaw = (body as { quote?: unknown } | null)?.quote;
	if (quoteRaw !== undefined && typeof quoteRaw !== 'string') {
		return { error: 'quote must be a string' };
	}
	const scopeRaw = (body as { scope?: unknown } | null)?.scope;
	if (scopeRaw !== undefined && scopeRaw !== 'project' && scopeRaw !== 'all') {
		return { error: "scope must be 'project' or 'all'" };
	}
	const projectPathRaw = (body as { project_path?: unknown } | null)?.project_path;
	if (
		projectPathRaw !== undefined &&
		(typeof projectPathRaw !== 'string' || projectPathRaw.length === 0)
	) {
		return { error: 'project_path must be a non-empty string' };
	}
	const scope: AssistantScope = scopeRaw === 'all' ? 'all' : 'project';
	const parsed: ParsedMessageBody = { text, scope };
	if (quoteRaw) parsed.quote = quoteRaw;
	if (typeof projectPathRaw === 'string') parsed.project_path = projectPathRaw;
	return parsed;
}

/**
 * Streams one conversational turn as SSE. Writes the response directly
 * (rather than returning a JsonResult) because the whole point of this route
 * is the stream — headers go out as soon as we know the conversation and the
 * feature toggle both check out.
 */
export async function handlePostMessage(
	deps: AppDeps,
	conversationId: string,
	body: unknown,
	res: ServerResponse
): Promise<void> {
	const now = deps.now();
	const parsed = parseMessageBody(body);
	if ('error' in parsed) {
		res.writeHead(400, { 'content-type': 'application/json' });
		res.end(JSON.stringify({ error: parsed.error }));
		return;
	}

	const conversation = await getConversation(deps.config.stateDir, conversationId);
	if (conversation === null) {
		res.writeHead(404, { 'content-type': 'application/json' });
		res.end(JSON.stringify({ error: 'conversation not found' }));
		return;
	}

	const settings = await resolvePromptAssistantSettings(deps.inspectClient);
	const creds = await resolveAnthropicCredentials(deps.inspectClient);

	startSse(res);
	const emit = (event: SseEvent): void => writeSse(res, event);

	if (!settings.enabled) {
		emit({
			event: 'error',
			data: {
				message: 'The prompt assistant is turned off — enable it on Settings › Assistants.'
			}
		});
		res.end();
		return;
	}

	if (settings.provider === 'anthropic' && creds.mode === 'api_key' && creds.api_key === null) {
		emit({
			event: 'error',
			data: {
				message:
					'The prompt assistant is set to use an Anthropic API key, but none is saved — add one on Settings › Integrations.'
			}
		});
		res.end();
		return;
	}

	if (
		settings.provider === 'anthropic' &&
		!(await canSpend(deps.config.stateDir, settings.daily_budget_usd, now))
	) {
		const spent = await spentToday(deps.config.stateDir, now);
		emit({
			event: 'error',
			data: {
				message: `Today's prompt-assistant budget of $${settings.daily_budget_usd.toFixed(2)} is used up ($${spent.toFixed(2)} spent). Raise prompt_assistant.daily_budget_usd or wait for tomorrow.`
			}
		});
		res.end();
		return;
	}

	const priorMessages = conversation.messages;
	const userMessage = newMessage('user', parsed.text, parsed.quote ? { quote: parsed.quote } : {});
	await appendMessage(deps.config.stateDir, conversationId, userMessage);

	const abortController = new AbortController();
	res.on('close', () => abortController.abort());

	try {
		const turnParams = {
			queryFn: deps.queryFn,
			inspectClient: deps.inspectClient,
			settings,
			requestId: conversation.request_id,
			priorMessages,
			userText: parsed.text,
			quote: parsed.quote,
			scope: parsed.scope,
			project_path: parsed.project_path,
			captureBaseUrl: deps.config.captureBaseUrl,
			claudeHome: deps.config.claudeHome,
			stateDir: deps.config.stateDir,
			abortController,
			checkBudget: () => canSpend(deps.config.stateDir, settings.daily_budget_usd, deps.now()),
			emit,
			anthropicCredentials: creds
		};
		const result =
			settings.provider === 'codex' ? await runCodexTurn(turnParams) : await runTurn(turnParams);

		if (result.costUsd > 0) {
			await addSpend(deps.config.stateDir, result.costUsd, now);
		}
		await appendMessage(
			deps.config.stateDir,
			conversationId,
			newMessage('assistant', result.text, {
				proposals: result.proposals.length > 0 ? result.proposals : undefined,
				cost_usd: result.costUsd
			})
		);

		if (result.error !== null) {
			emit({ event: 'error', data: { message: result.error } });
		}
		emit({ event: 'done', data: { cost_usd: result.costUsd } });
	} catch (err) {
		emit({ event: 'error', data: { message: (err as Error).message } });
	}
	res.end();
}
