import type { RequestSummary, ToolUseDto } from './types';

export type DispatchOutcome = 'completed' | 'abandoned' | 'error' | null;

export interface AgentEvent {
	requestId: number;
	createdAt: string;
	outcome: DispatchOutcome;
	description: string | null;
}

export interface AgentEntry {
	subagentType: string;
	model: string | null;
	description: string;
	events: AgentEvent[];
}

export const DEFAULT_SUBAGENT_TYPE = 'general-purpose';

function dispatchOutcome(r: ToolUseDto): DispatchOutcome {
	if (!r.result) return null;
	return r.result.is_error ? 'error' : 'completed';
}

export function deriveAgentDispatchGroups(
	toolUses: readonly ToolUseDto[],
	requests: readonly RequestSummary[]
): AgentEntry[] {
	const groups = new Map<string, AgentEvent[]>();
	for (const r of toolUses) {
		if (r.tool_name !== 'Agent') continue;
		const input = r.input;
		const obj = input && typeof input === 'object' ? (input as Record<string, unknown>) : {};
		const rawType = typeof obj.subagent_type === 'string' ? obj.subagent_type.trim() : '';
		const subagentType = rawType.length > 0 ? rawType : DEFAULT_SUBAGENT_TYPE;
		const description = typeof obj.description === 'string' ? obj.description : null;
		const list = groups.get(subagentType) ?? [];
		list.push({
			requestId: r.request_id,
			createdAt: r.created_at,
			outcome: dispatchOutcome(r),
			description
		});
		groups.set(subagentType, list);
	}

	function modelFor(subType: string): string | null {
		const counts = new Map<string, number>();
		for (const req of requests) {
			if ((req.turn_origin ?? '') !== 'subagent') continue;
			const persona = req.subagent_persona ?? null;
			if (persona === null || persona !== subType) continue;
			if (!req.model) continue;
			counts.set(req.model, (counts.get(req.model) ?? 0) + 1);
		}
		let best: string | null = null;
		let bestN = 0;
		for (const [m, n] of counts) {
			if (n > bestN) {
				best = m;
				bestN = n;
			}
		}
		return best;
	}

	const out: AgentEntry[] = [];
	for (const [subagentType, events] of groups) {
		out.push({
			subagentType,
			model: modelFor(subagentType),
			description: events[0]?.description ?? '(no description)',
			events
		});
	}
	return out;
}
