import type { query as QueryFn } from '@anthropic-ai/claude-agent-sdk';
import { describe, expect, it } from 'vitest';
import { renderTranscript, runTurn } from '../src/agent/runTurn.js';
import { InspectClient } from '../src/inspectClient.js';
import { SETTINGS_DEFAULTS } from '../src/settings.js';
import type { PromptAssistantSettings, SseEvent } from '../src/types.js';
import { stubFetch } from './fakes.js';

const UNUSED_CLAUDE_HOME = '/tmp/prompt-assistant-tests-unused-claude-home';
const UNUSED_STATE_DIR = '/tmp/prompt-assistant-tests-unused-state-dir';

function settings(overrides: Partial<PromptAssistantSettings> = {}): PromptAssistantSettings {
	return { ...SETTINGS_DEFAULTS, enabled: true, ...overrides };
}

interface CapturedOptions {
	tools?: unknown;
	allowedTools?: string[];
	disallowedTools?: string[];
	canUseTool?: (
		toolName: string,
		input: Record<string, unknown>,
		options: Record<string, unknown>
	) => Promise<{ behavior: 'allow' | 'deny'; message?: string } | null>;
	settingSources?: unknown;
	strictMcpConfig?: unknown;
	cwd?: string;
	env?: Record<string, string | undefined>;
	abortController?: AbortController;
	systemPrompt?: { prompt?: string };
	effort?: string;
	verbatimPrompts?: boolean;
}

function fakeQuery(capture: { options?: CapturedOptions }, deltas: string[], costUsd: number): typeof QueryFn {
	return ((params: { options?: CapturedOptions }) => {
		capture.options = params.options;
		async function* gen() {
			for (const delta of deltas) {
				yield {
					type: 'stream_event',
					event: { type: 'content_block_delta', delta: { type: 'text_delta', text: delta } },
					parent_tool_use_id: null,
					uuid: 'u',
					session_id: 's'
				};
			}
			yield {
				type: 'result',
				subtype: 'success',
				duration_ms: 1,
				duration_api_ms: 1,
				is_error: false,
				num_turns: 1,
				result: deltas.join(''),
				stop_reason: null,
				total_cost_usd: costUsd,
				usage: {},
				modelUsage: {},
				permission_denials: [],
				uuid: 'u2',
				session_id: 's'
			};
		}
		return gen();
	}) as unknown as typeof QueryFn;
}

/** A fake `queryFn` whose generator throws partway through, after yielding some messages. */
function throwingQuery(
	capture: { options?: CapturedOptions },
	messages: Array<{ delta: string } | { result: number }>,
	errorMessage: string
): typeof QueryFn {
	return ((params: { options?: CapturedOptions }) => {
		capture.options = params.options;
		async function* gen() {
			for (const m of messages) {
				if ('delta' in m) {
					yield {
						type: 'stream_event',
						event: { type: 'content_block_delta', delta: { type: 'text_delta', text: m.delta } },
						parent_tool_use_id: null,
						uuid: 'u',
						session_id: 's'
					};
				} else {
					yield {
						type: 'result',
						subtype: 'success',
						duration_ms: 1,
						duration_api_ms: 1,
						is_error: false,
						num_turns: 1,
						result: 'partial',
						stop_reason: null,
						total_cost_usd: m.result,
						usage: {},
						modelUsage: {},
						permission_denials: [],
						uuid: 'u2',
						session_id: 's'
					};
				}
			}
			throw new Error(errorMessage);
		}
		return gen();
	}) as unknown as typeof QueryFn;
}

function baseParams(overrides: Partial<Parameters<typeof runTurn>[0]> = {}): Parameters<typeof runTurn>[0] {
	return {
		queryFn: fakeQuery({}, ['ok'], 0),
		inspectClient: new InspectClient('http://inspect.local', stubFetch({})),
		settings: settings(),
		requestId: 1,
		priorMessages: [],
		userText: 'hi',
		captureBaseUrl: 'http://localhost:9988',
		claudeHome: UNUSED_CLAUDE_HOME,
		stateDir: UNUSED_STATE_DIR,
		abortController: new AbortController(),
		emit: () => {},
		...overrides
	};
}

describe('runTurn', () => {
	it("tells the model which request_id it is looking at, so it never has to ask", async () => {
		const capture: { options?: CapturedOptions } = {};
		const result = await runTurn(
			baseParams({
				queryFn: fakeQuery(capture, ['ok'], 0.001),
				requestId: 421514,
				userText: 'what does the harness section say?'
			})
		);
		expect(result.text).toBe('ok');
		expect(result.error).toBeNull();
		expect(capture.options?.systemPrompt?.prompt).toContain('request_id 421514');
	});

	it('tells the model to read a specific project when the workbench picked one', async () => {
		const capture: { options?: CapturedOptions } = {};
		await runTurn(
			baseParams({
				queryFn: fakeQuery(capture, ['ok'], 0),
				scope: 'project',
				project_path: '/mnt/data/projects/other'
			})
		);
		expect(capture.options?.systemPrompt?.prompt).toContain(
			'read_project_instructions with project_path="/mnt/data/projects/other"'
		);
	});

	it('does not mention a specific project_path when scope is all', async () => {
		const capture: { options?: CapturedOptions } = {};
		await runTurn(
			baseParams({
				queryFn: fakeQuery(capture, ['ok'], 0),
				scope: 'all',
				project_path: '/mnt/data/projects/other'
			})
		);
		expect(capture.options?.systemPrompt?.prompt).not.toContain('project_path=');
		expect(capture.options?.systemPrompt?.prompt).toContain('All projects');
	});

	it('falls back to the request_id-derived project when scope is project but no project_path is given', async () => {
		const capture: { options?: CapturedOptions } = {};
		await runTurn(
			baseParams({
				queryFn: fakeQuery(capture, ['ok'], 0),
				scope: 'project'
			})
		);
		expect(capture.options?.systemPrompt?.prompt).toContain('this project only');
		expect(capture.options?.systemPrompt?.prompt).not.toContain('project_path=');
	});

	it('disables every built-in tool and only allows this turn’s registered MCP tools', async () => {
		const capture: { options?: CapturedOptions } = {};
		await runTurn(
			baseParams({
				queryFn: fakeQuery(capture, ['ok'], 0),
				settings: settings({ read_project_instructions: false })
			})
		);
		expect(capture.options?.tools).toEqual([]);
		expect(capture.options?.allowedTools).not.toContain('mcp__prompt_assistant__read_project_instructions');
		expect(capture.options?.allowedTools).toContain('mcp__prompt_assistant__propose_edit');
	});

	it('sets ANTHROPIC_BASE_URL to the capture proxy only when capture is on', async () => {
		const capturedOn: { options?: CapturedOptions } = {};
		await runTurn(baseParams({ queryFn: fakeQuery(capturedOn, ['ok'], 0), settings: settings({ capture: true }) }));
		expect(capturedOn.options?.env?.ANTHROPIC_BASE_URL).toBe('http://localhost:9988');

		const capturedOff: { options?: CapturedOptions } = {};
		await runTurn(baseParams({ queryFn: fakeQuery(capturedOff, ['ok'], 0), settings: settings({ capture: false }) }));
		expect(capturedOff.options?.env?.ANTHROPIC_BASE_URL).toBeUndefined();
	});

	describe('G1: sandboxing', () => {
		it('loads no user/project settings and only its own strict MCP config, in a neutral cwd', async () => {
			const capture: { options?: CapturedOptions } = {};
			await runTurn(baseParams({ queryFn: fakeQuery(capture, ['ok'], 0), stateDir: '/tmp/pa-state-xyz' }));
			expect(capture.options?.settingSources).toEqual([]);
			expect(capture.options?.strictMcpConfig).toBe(true);
			expect(capture.options?.cwd).toBe('/tmp/pa-state-xyz');
		});

		it('names the built-in tools in disallowedTools as a second guard alongside tools: []', async () => {
			const capture: { options?: CapturedOptions } = {};
			await runTurn(baseParams({ queryFn: fakeQuery(capture, ['ok'], 0) }));
			expect(capture.options?.disallowedTools).toEqual(expect.arrayContaining(['Bash', 'Read', 'Write', 'WebFetch']));
		});

		it('sends client_composed prompts so the CLI turn-start attachment pass never injects environment/userEmail/date reminders', async () => {
			const capture: { options?: CapturedOptions } = {};
			await runTurn(baseParams({ queryFn: fakeQuery(capture, ['ok'], 0) }));
			expect(capture.options?.verbatimPrompts).toBe(true);
		});

		it('canUseTool denies a foreign mcp__mike__* tool and allows this turn’s own mcp__prompt_assistant__ tool', async () => {
			const capture: { options?: CapturedOptions } = {};
			await runTurn(baseParams({ queryFn: fakeQuery(capture, ['ok'], 0) }));
			const canUseTool = capture.options?.canUseTool;
			expect(canUseTool).toBeTypeOf('function');
			const denied = await canUseTool!('mcp__mike__delete_filter_rule', {}, {} as never);
			expect(denied?.behavior).toBe('deny');
			const allowed = await canUseTool!('mcp__prompt_assistant__get_prompt', { request_id: 1 }, {} as never);
			expect(allowed?.behavior).toBe('allow');
		});
	});

	describe('G2: env allowlist', () => {
		it('never forwards CLAUDE_* / CLAUDE_CODE_* vars from the process env, even when set', async () => {
			const dirtyVars = [
				'CLAUDE_EFFORT',
				'CLAUDE_CODE_CHILD_SESSION',
				'CLAUDE_CODE_BRIDGE_SESSION_ID',
				'CLAUDE_CODE_MESSAGING_SOCKET',
				'CLAUDE_CODE_MESSAGING_TOKEN'
			] as const;
			const before: Record<string, string | undefined> = {};
			for (const key of dirtyVars) before[key] = process.env[key];
			process.env.CLAUDE_EFFORT = 'high';
			process.env.CLAUDE_CODE_CHILD_SESSION = '1';
			process.env.CLAUDE_CODE_BRIDGE_SESSION_ID = 'abc';
			process.env.CLAUDE_CODE_MESSAGING_SOCKET = '/tmp/sock';
			process.env.CLAUDE_CODE_MESSAGING_TOKEN = 'secret';
			try {
				const capture: { options?: CapturedOptions } = {};
				await runTurn(baseParams({ queryFn: fakeQuery(capture, ['ok'], 0), settings: settings({ effort: 'low' }) }));
				const env = capture.options?.env ?? {};
				expect(env.CLAUDE_EFFORT).toBeUndefined();
				expect(env.CLAUDE_CODE_CHILD_SESSION).toBeUndefined();
				expect(env.CLAUDE_CODE_BRIDGE_SESSION_ID).toBeUndefined();
				expect(env.CLAUDE_CODE_MESSAGING_SOCKET).toBeUndefined();
				expect(env.CLAUDE_CODE_MESSAGING_TOKEN).toBeUndefined();
				expect(capture.options?.effort).toBe('low');
			} finally {
				for (const key of dirtyVars) {
					if (before[key] === undefined) delete process.env[key];
					else process.env[key] = before[key];
				}
			}
		});

		it('still carries PATH and HOME through to the subprocess', async () => {
			const capture: { options?: CapturedOptions } = {};
			await runTurn(baseParams({ queryFn: fakeQuery(capture, ['ok'], 0) }));
			expect(capture.options?.env?.PATH).toBe(process.env.PATH);
			expect(capture.options?.env?.HOME).toBe(process.env.HOME);
		});
	});

	describe('anthropicCredentials', () => {
		it('overrides env.ANTHROPIC_API_KEY when mode is api_key', async () => {
			const capture: { options?: CapturedOptions } = {};
			const before = process.env.ANTHROPIC_API_KEY;
			process.env.ANTHROPIC_API_KEY = 'operator-login-key';
			try {
				await runTurn(
					baseParams({
						queryFn: fakeQuery(capture, ['ok'], 0),
						anthropicCredentials: { mode: 'api_key', api_key: 'sk-saved-override' }
					})
				);
				expect(capture.options?.env?.ANTHROPIC_API_KEY).toBe('sk-saved-override');
			} finally {
				if (before === undefined) delete process.env.ANTHROPIC_API_KEY;
				else process.env.ANTHROPIC_API_KEY = before;
			}
		});

		it('leaves the existing allowlist pass-through unchanged when mode is claude_code_login', async () => {
			const capture: { options?: CapturedOptions } = {};
			const before = process.env.ANTHROPIC_API_KEY;
			process.env.ANTHROPIC_API_KEY = 'operator-login-key';
			try {
				await runTurn(
					baseParams({
						queryFn: fakeQuery(capture, ['ok'], 0),
						anthropicCredentials: { mode: 'claude_code_login', api_key: null }
					})
				);
				expect(capture.options?.env?.ANTHROPIC_API_KEY).toBe('operator-login-key');
			} finally {
				if (before === undefined) delete process.env.ANTHROPIC_API_KEY;
				else process.env.ANTHROPIC_API_KEY = before;
			}
		});

		it('leaves the existing allowlist pass-through unchanged when no anthropicCredentials is supplied', async () => {
			const capture: { options?: CapturedOptions } = {};
			const before = process.env.ANTHROPIC_API_KEY;
			process.env.ANTHROPIC_API_KEY = 'operator-login-key';
			try {
				await runTurn(baseParams({ queryFn: fakeQuery(capture, ['ok'], 0) }));
				expect(capture.options?.env?.ANTHROPIC_API_KEY).toBe('operator-login-key');
			} finally {
				if (before === undefined) delete process.env.ANTHROPIC_API_KEY;
				else process.env.ANTHROPIC_API_KEY = before;
			}
		});
	});

	describe('G13: budget checked during the turn', () => {
		it('aborts via the AbortController and reports a budget error when checkBudget reports the budget is gone', async () => {
			const capture: { options?: CapturedOptions } = {};
			const events: SseEvent[] = [];
			const abortController = new AbortController();
			const result = await runTurn(
				baseParams({
					queryFn: fakeQuery(capture, ['first', 'second'], 0.01),
					abortController,
					checkBudget: async () => false,
					emit: (e) => events.push(e)
				})
			);
			expect(abortController.signal.aborted).toBe(true);
			expect(result.error).toContain('budget');
			expect(result.text).toBe('');
			expect(events.some((e) => e.event === 'error')).toBe(true);
			expect(capture.options?.abortController).toBe(abortController);
		});

		it('does not check the budget at all when no checkBudget is supplied', async () => {
			const result = await runTurn(baseParams({ queryFn: fakeQuery({}, ['ok'], 0) }));
			expect(result.error).toBeNull();
			expect(result.text).toBe('ok');
		});

		it('records the partial cost from the last result seen even when the stream then throws', async () => {
			const capture: { options?: CapturedOptions } = {};
			const result = await runTurn(
				baseParams({
					queryFn: throwingQuery(capture, [{ delta: 'partial answer' }, { result: 0.02 }], 'subprocess crashed')
				})
			);
			expect(result.text).toBe('partial answer');
			expect(result.costUsd).toBe(0.02);
			expect(result.error).toBe('subprocess crashed');
		});
	});
});

describe('renderTranscript', () => {
	it('renders prior turns, a quote block, then the new operator line', () => {
		const transcript = renderTranscript(
			[
				{ id: '1', role: 'user', text: 'first question', created_at: 't' },
				{ id: '2', role: 'assistant', text: 'first answer', created_at: 't' }
			],
			'second question',
			'the quoted paragraph'
		);
		expect(transcript).toContain('Operator: first question');
		expect(transcript).toContain('Assistant: first answer');
		expect(transcript).toContain('"""\nthe quoted paragraph\n"""');
		expect(transcript.endsWith('Operator: second question')).toBe(true);
	});
});
