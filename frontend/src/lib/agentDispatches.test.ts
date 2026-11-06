import { describe, it, expect } from 'vitest';
import { deriveAgentDispatchGroups, DEFAULT_SUBAGENT_TYPE } from './agentDispatches';
import type { ToolUseDto, RequestSummary } from './types';

function toolUse(overrides: Partial<ToolUseDto> & { request_id: number }): ToolUseDto {
	return {
		session_id: 'sess-1',
		created_at: '2026-07-10T09:00:00Z',
		tool_id: `toolu_${overrides.request_id}`,
		tool_name: 'Agent',
		input: {},
		file_path: null,
		ord: 0,
		...overrides
	};
}

describe('deriveAgentDispatchGroups', () => {
	it('groups Agent tool_uses by subagent_type, ignoring other tools', () => {
		const toolUses: ToolUseDto[] = [
			toolUse({ request_id: 1, input: { subagent_type: 'rust-developer', description: 'A' } }),
			toolUse({ request_id: 2, tool_name: 'Bash', input: { command: 'ls' } }),
			toolUse({ request_id: 3, input: { subagent_type: 'rust-developer', description: 'B' } }),
			toolUse({ request_id: 4, input: { subagent_type: 'frontend-developer', description: 'C' } })
		];
		const groups = deriveAgentDispatchGroups(toolUses, []);
		expect(groups).toHaveLength(2);
		const rust = groups.find((g) => g.subagentType === 'rust-developer');
		expect(rust?.events.map((e) => e.requestId)).toEqual([1, 3]);
		const frontend = groups.find((g) => g.subagentType === 'frontend-developer');
		expect(frontend?.events.map((e) => e.requestId)).toEqual([4]);
	});

	it('defaults a missing subagent_type to general-purpose, never "unknown"', () => {
		const toolUses: ToolUseDto[] = [
			toolUse({ request_id: 1, input: { description: 'no persona field' } })
		];
		const groups = deriveAgentDispatchGroups(toolUses, []);
		expect(groups).toHaveLength(1);
		expect(groups[0]?.subagentType).toBe('general-purpose');
		expect(groups[0]?.subagentType).toBe(DEFAULT_SUBAGENT_TYPE);
	});

	it('defaults an empty-string subagent_type to general-purpose', () => {
		const toolUses: ToolUseDto[] = [
			toolUse({ request_id: 1, input: { subagent_type: '   ', description: 'blank' } })
		];
		const groups = deriveAgentDispatchGroups(toolUses, []);
		expect(groups[0]?.subagentType).toBe('general-purpose');
	});

	it('derives outcome from the paired tool_result', () => {
		const toolUses: ToolUseDto[] = [
			toolUse({
				request_id: 1,
				input: { subagent_type: 'rust-developer' },
				result: {
					tool_use_id: 't1',
					is_error: false,
					content: 'ok',
					content_sha256: 'x',
					content_bytes: 2
				}
			}),
			toolUse({
				request_id: 2,
				input: { subagent_type: 'rust-developer' },
				result: {
					tool_use_id: 't2',
					is_error: true,
					content: 'boom',
					content_sha256: 'y',
					content_bytes: 4
				}
			}),
			toolUse({ request_id: 3, input: { subagent_type: 'rust-developer' } })
		];
		const groups = deriveAgentDispatchGroups(toolUses, []);
		const events = groups[0]?.events ?? [];
		expect(events.find((e) => e.requestId === 1)?.outcome).toBe('completed');
		expect(events.find((e) => e.requestId === 2)?.outcome).toBe('error');
		expect(events.find((e) => e.requestId === 3)?.outcome).toBeNull();
	});

	it("uses the first event's description as the group-level summary, falling back when absent", () => {
		const toolUses: ToolUseDto[] = [
			toolUse({
				request_id: 1,
				input: { subagent_type: 'rust-developer', description: 'first task' }
			}),
			toolUse({
				request_id: 2,
				input: { subagent_type: 'rust-developer', description: 'second task' }
			}),
			toolUse({ request_id: 3, input: { subagent_type: 'frontend-developer' } })
		];
		const groups = deriveAgentDispatchGroups(toolUses, []);
		expect(groups.find((g) => g.subagentType === 'rust-developer')?.description).toBe('first task');
		expect(groups.find((g) => g.subagentType === 'frontend-developer')?.description).toBe(
			'(no description)'
		);
	});

	it('picks the most common model among subagent requests matching the persona label', () => {
		const toolUses: ToolUseDto[] = [
			toolUse({ request_id: 1, input: { subagent_type: 'rust-developer' } })
		];
		const requests: RequestSummary[] = [
			{
				id: 101,
				method: 'POST',
				path: '/v1/messages',
				model: 'claude-sonnet-4-6',
				status: 'Complete',
				is_streaming: false,
				message_count: 1,
				max_tokens: null,
				created_at: '2026-07-10T09:00:00Z',
				completed_at: '2026-07-10T09:00:05Z',
				session_id: 'sess-1',
				project: null,
				human_prompt: '',
				turn_type: 'user_prompt',
				turn_origin: 'subagent',
				subagent_persona: 'rust-developer'
			} as RequestSummary,
			{
				id: 102,
				method: 'POST',
				path: '/v1/messages',
				model: 'claude-haiku-4-5-20251001',
				status: 'Complete',
				is_streaming: false,
				message_count: 1,
				max_tokens: null,
				created_at: '2026-07-10T09:01:00Z',
				completed_at: '2026-07-10T09:01:05Z',
				session_id: 'sess-1',
				project: null,
				human_prompt: '',
				turn_type: 'user_prompt',
				turn_origin: 'subagent',
				subagent_persona: 'frontend-developer'
			} as RequestSummary
		];
		const groups = deriveAgentDispatchGroups(toolUses, requests);
		expect(groups[0]?.model).toBe('claude-sonnet-4-6');
	});

	it('returns an empty list when there are no Agent tool_uses', () => {
		expect(deriveAgentDispatchGroups([], [])).toEqual([]);
	});
});
