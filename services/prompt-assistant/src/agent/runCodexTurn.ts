import { z } from 'zod';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { CodexAppServer } from '../codex/appServer.js';
import { buildTools } from '../mcp/tools.js';
import type { Proposal } from '../types.js';
import { renderTranscript, type RunTurnParams, type RunTurnResult } from './runTurn.js';

interface DynamicTool {
	name: string;
	description: string;
	inputSchema: unknown;
	handler: (
		args: unknown,
		extra: unknown
	) => Promise<{
		content: Array<{ type: string; text?: string }>;
		isError?: boolean;
	}>;
}

function scopeInstruction(scope: RunTurnParams['scope'], projectPath: string | undefined): string {
	if (scope === 'all') {
		return 'Scope for this message: All projects. Use list_projects and compare_across_projects before recommending anything.';
	}
	if (projectPath) {
		return `Scope for this message: the project at ${projectPath}. Call read_project_instructions with that project_path.`;
	}
	return 'Scope for this message: this project only.';
}

function toolSpecs(tools: DynamicTool[]): unknown[] {
	return tools.map((tool) => ({
		type: 'function',
		name: tool.name,
		description: tool.description,
		inputSchema: z.toJSONSchema(z.object(tool.inputSchema as z.ZodRawShape))
	}));
}

function toolText(result: Awaited<ReturnType<DynamicTool['handler']>>): string {
	return result.content
		.map((item) => item.text ?? '')
		.filter(Boolean)
		.join('\n');
}

export async function runCodexTurn(params: RunTurnParams): Promise<RunTurnResult> {
	const issuedProposals: Proposal[] = [];
	const tools = buildTools({
		inspectClient: params.inspectClient,
		settings: params.settings,
		requestId: params.requestId,
		emit: params.emit,
		issuedProposals,
		claudeHome: params.claudeHome
	}) as DynamicTool[];
	const byName = new Map(tools.map((tool) => [tool.name, tool]));
	const isolatedCwd = await mkdtemp(join(tmpdir(), 'mike-codex-'));
	let server: CodexAppServer;
	try {
		server = await CodexAppServer.start();
	} catch (error) {
		await rm(isolatedCwd, { recursive: true, force: true });
		throw error;
	}
	if (params.abortController.signal.aborted) {
		server.close();
		await rm(isolatedCwd, { recursive: true, force: true });
		return { text: '', costUsd: 0, proposals: issuedProposals, error: 'Codex turn was cancelled' };
	}
	let text = '';
	let resultError: string | null = null;
	let toolCalls = 0;
	let settled = false;
	let completed!: () => void;
	const completion = new Promise<void>((resolve) => (completed = resolve));

	server.onServerRequest(async (method, request) => {
		if (method !== 'item/tool/call') throw new Error(`unsupported Codex request: ${method}`);
		toolCalls += 1;
		if (toolCalls > params.settings.max_turns) {
			throw new Error(`Codex exceeded the ${params.settings.max_turns}-tool limit for this turn`);
		}
		const tool = byName.get(String(request.tool));
		if (!tool) throw new Error(`Codex requested unavailable tool: ${String(request.tool)}`);
		const result = await tool.handler(request.arguments ?? {}, {});
		return {
			contentItems: [{ type: 'inputText', text: toolText(result) }],
			success: result.isError !== true
		};
	});
	server.onNotification((message) => {
		if (message.method === 'item/agentMessage/delta') {
			const delta = String((message.params as { delta?: unknown })?.delta ?? '');
			text += delta;
			params.emit({ event: 'text', data: { delta } });
		}
		if (message.method === 'turn/completed') {
			const turn = (
				message.params as {
					turn?: { status?: string; error?: { message?: string } };
				}
			).turn;
			if (turn?.status !== 'completed')
				resultError = turn?.error?.message ?? `Codex turn ${turn?.status}`;
			settled = true;
			completed();
		}
		if (message.method === 'transport/closed' && !settled) {
			resultError = String(
				(message.params as { message?: unknown })?.message ?? 'Codex app-server closed'
			);
			settled = true;
			completed();
		}
	});
	params.abortController.signal.addEventListener(
		'abort',
		() => {
			if (!settled) resultError = 'Codex turn was cancelled';
			settled = true;
			completed();
			server.close();
		},
		{ once: true }
	);

	try {
		const threadResult = (await server.request('thread/start', {
			model: params.settings.codex_model,
			cwd: isolatedCwd,
			approvalPolicy: 'never',
			sandbox: 'read-only',
			ephemeral: true,
			serviceName: 'mike_prompt_assistant',
			config: {
				features: {
					shell_tool: false,
					unified_exec: false,
					apps: false,
					plugins: false
				},
				agents: { enabled: false },
				mcp_servers: {},
				hooks: {},
				tools: { web_search: false, view_image: false },
				web_search: 'disabled'
			},
			baseInstructions: `${params.settings.instructions}\n\nThe operator is looking at request_id ${params.requestId}. ${scopeInstruction(params.scope, params.project_path)}`,
			dynamicTools: toolSpecs(tools)
		})) as { thread: { id: string } };
		await server.request('turn/start', {
			threadId: threadResult.thread.id,
			input: [
				{
					type: 'text',
					text: renderTranscript(params.priorMessages, params.userText, params.quote),
					text_elements: []
				}
			],
			effort: params.settings.effort
		});
		await completion;
	} catch (error) {
		resultError = (error as Error).message;
	} finally {
		server.close();
		await rm(isolatedCwd, { recursive: true, force: true });
	}
	return { text, costUsd: 0, proposals: issuedProposals, error: resultError };
}
