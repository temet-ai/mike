import { shortSessionId } from '$lib/format';
import { parseIso } from '$lib/time';
import type {
	FleetErrorKind,
	FleetGraphEdge,
	FleetGraphNode,
	FleetNodeQuality,
	FleetNodeState
} from './types';

const STATE_LABEL: Record<FleetNodeState, string> = {
	working: 'Working',
	idle: 'Idle',
	stale: 'Stale',
	done: 'Done',
	error: 'Error',
	unknown: 'Unknown'
};

export interface CanvasNode {
	id: string;
	role: string | null;
	agentType: string | null;
	title: string | null;
	label: string;
	hasIdentityLabel: boolean;
	model: string | null;
	workspace: string | null;
	cwd: string | null;
	worktree: string | null;
	sessionId: string;
	state: FleetNodeState;
	stateLabel: string;
	stateDetail: string | null;
	errorKind: FleetErrorKind | null;
	quality: FleetNodeQuality | null;
	unconsumedMail: number;
	workingOnNow: string | null;
	goal: string | null;
	files: string[];
	lastActivityAt: string | null;
	requestCount: number | null;
}

export interface CanvasEdge {
	id: string;
	from: string;
	to: string;
	kind: 'messaged';
	count: number;
}

export interface CanvasGraph {
	nodes: CanvasNode[];
	edges: CanvasEdge[];
}

export function fromGraphResponse(nodes: FleetGraphNode[], edges: FleetGraphEdge[]): CanvasGraph {
	const canvasNodes: CanvasNode[] = nodes.map((n) => {
		const identityLabel = n.title ?? n.goal;
		return {
			id: n.session_id,
			role: n.role,
			agentType: n.agent_type,
			title: n.title,
			label: identityLabel ?? shortSessionId(n.session_id),
			hasIdentityLabel: identityLabel !== null,
			model: n.model,
			workspace: n.workspace,
			cwd: n.cwd,
			worktree: n.worktree,
			sessionId: n.session_id,
			state: n.state,
			stateLabel: STATE_LABEL[n.state],
			stateDetail: n.state_detail ?? null,
			errorKind: n.error_kind ?? null,
			quality: n.quality ?? null,
			unconsumedMail: n.unconsumed_mail ?? 0,
			workingOnNow: n.next_steps ?? n.goal,
			goal: n.goal,
			files: [],
			lastActivityAt: n.last_activity_at,
			requestCount: n.request_count
		};
	});
	const canvasEdges: CanvasEdge[] = edges.map((e, i) => ({
		id: `${e.from_session_id}->${e.to_session_id}:${e.kind}:${i}`,
		from: e.from_session_id,
		to: e.to_session_id,
		kind: e.kind,
		count: e.count
	}));
	return { nodes: canvasNodes, edges: canvasEdges };
}

export function partitionByConnectivity(
	nodes: CanvasNode[],
	edges: CanvasEdge[]
): { connected: CanvasNode[]; isolated: CanvasNode[] } {
	const touched = new Set<string>();
	for (const e of edges) {
		if (e.from === e.to) continue;
		touched.add(e.from);
		touched.add(e.to);
	}
	const connected: CanvasNode[] = [];
	const isolated: CanvasNode[] = [];
	for (const n of nodes) {
		(touched.has(n.id) ? connected : isolated).push(n);
	}
	return { connected, isolated };
}

export function distinctRoles(nodes: CanvasNode[]): string[] {
	const set = new Set<string>();
	for (const n of nodes) {
		const identity = n.role ?? n.agentType;
		if (identity !== null) set.add(identity);
	}
	return Array.from(set).sort();
}

export function matchesQuery(node: CanvasNode, query: string): boolean {
	const q = query.trim().toLowerCase();
	if (q.length === 0) return true;
	const haystacks: Array<string | null> = [
		node.label,
		node.role,
		node.agentType,
		node.id,
		shortSessionId(node.id),
		node.stateLabel
	];
	return haystacks.some((h) => h !== null && h.toLowerCase().includes(q));
}

type StatusPillClass =
	| 'status-pill-streaming'
	| 'status-pill-idle'
	| 'status-pill-complete'
	| 'status-pill-error'
	| 'status-pill-unknown';

export function canvasStatePillClass(state: FleetNodeState): StatusPillClass {
	switch (state) {
		case 'working':
			return 'status-pill-streaming';
		case 'idle':
			return 'status-pill-idle';
		case 'done':
			return 'status-pill-complete';
		case 'error':
			return 'status-pill-error';
		default:
			return 'status-pill-unknown';
	}
}

export function canvasStateBorderVar(state: FleetNodeState): string {
	switch (state) {
		case 'working':
			return 'var(--status-warning-border)';
		case 'idle':
			return 'var(--status-info-border)';
		case 'done':
			return 'var(--status-success-border)';
		case 'error':
			return 'var(--status-danger-border)';
		default:
			return 'var(--border-default)';
	}
}

export function errorLineFor(
	node: Pick<CanvasNode, 'state' | 'errorKind' | 'stateDetail'>
): string | null {
	if (node.state !== 'error') return null;
	const prefix =
		node.errorKind === 'infra'
			? 'infra event — '
			: node.errorKind === 'ended_early'
				? 'ended early — '
				: null;
	if (node.stateDetail !== null) return `${prefix ?? ''}${node.stateDetail}`;
	return prefix !== null ? prefix.replace(/\s+—\s+$/, '') : null;
}

export function errorLineIsAgent(node: Pick<CanvasNode, 'errorKind'>): boolean {
	return node.errorKind === null || node.errorKind === 'agent';
}

export const RECENT_ACTIVITY_WINDOW_MS = 15 * 60 * 1000;

export function deriveActiveFleet(
	nodes: CanvasNode[],
	edges: CanvasEdge[],
	now: number
): { active: CanvasNode[]; historyCount: number } {
	const touched = new Set<string>();
	for (const e of edges) {
		if (e.from === e.to) continue;
		touched.add(e.from);
		touched.add(e.to);
	}
	const active: CanvasNode[] = [];
	let historyCount = 0;
	for (const n of nodes) {
		const hasRole = n.role !== null;
		const isConnected = touched.has(n.id);
		const isError = n.state === 'error';
		const activityMs =
			n.lastActivityAt !== null ? parseIso(n.lastActivityAt)?.getTime() : undefined;
		const isRecent = activityMs !== undefined && now - activityMs < RECENT_ACTIVITY_WINDOW_MS;
		if (hasRole || isConnected || isError || isRecent) active.push(n);
		else historyCount++;
	}
	return { active, historyCount };
}

export type BoardGroupKind = 'attention' | 'active-now' | 'active-fleet';

export interface BoardGroup {
	kind: BoardGroupKind;
	title: string;
	nodes: CanvasNode[];
}

function byLastActivityDesc(a: CanvasNode, b: CanvasNode): number {
	const am = a.lastActivityAt !== null ? (parseIso(a.lastActivityAt)?.getTime() ?? null) : null;
	const bm = b.lastActivityAt !== null ? (parseIso(b.lastActivityAt)?.getTime() ?? null) : null;
	if (am === null && bm === null) return 0;
	if (am === null) return 1;
	if (bm === null) return -1;
	return bm - am;
}

export function groupBoardRows(active: CanvasNode[], now: number): BoardGroup[] {
	const attention: CanvasNode[] = [];
	const activeNow: CanvasNode[] = [];
	const rest: CanvasNode[] = [];
	for (const n of active) {
		if (n.state === 'error') {
			attention.push(n);
			continue;
		}
		const activityMs =
			n.lastActivityAt !== null ? parseIso(n.lastActivityAt)?.getTime() : undefined;
		const isRecent = activityMs !== undefined && now - activityMs < RECENT_ACTIVITY_WINDOW_MS;
		(isRecent ? activeNow : rest).push(n);
	}
	attention.sort(byLastActivityDesc);
	activeNow.sort(byLastActivityDesc);
	rest.sort(byLastActivityDesc);
	const groups: BoardGroup[] = [];
	if (attention.length > 0)
		groups.push({ kind: 'attention', title: 'Needs attention', nodes: attention });
	if (activeNow.length > 0)
		groups.push({ kind: 'active-now', title: 'Active now', nodes: activeNow });
	if (rest.length > 0) groups.push({ kind: 'active-fleet', title: 'Active fleet', nodes: rest });
	return groups;
}

export interface Connection {
	other: CanvasNode;
	direction: 'out' | 'in';
	kind: CanvasEdge['kind'];
	count: number;
}

export function connectionsFor(
	id: string,
	edges: CanvasEdge[],
	nodesById: Map<string, CanvasNode>
): Connection[] {
	const out: Connection[] = [];
	for (const e of edges) {
		if (e.from === id) {
			const other = nodesById.get(e.to);
			if (other) out.push({ other, direction: 'out', kind: e.kind, count: e.count });
		} else if (e.to === id) {
			const other = nodesById.get(e.from);
			if (other) out.push({ other, direction: 'in', kind: e.kind, count: e.count });
		}
	}
	return out;
}

export interface EdgeGroup {
	id: string;
	from: string;
	to: string;
	forwardCount: number;
	backwardCount: number;
	bidirectional: boolean;
}

export function groupBidirectionalEdges(edges: CanvasEdge[]): EdgeGroup[] {
	const byPair = new Map<string, { from: string; to: string; forward: number; backward: number }>();
	for (const e of edges) {
		if (e.from === e.to) continue;
		const canonicalFrom = e.from < e.to ? e.from : e.to;
		const canonicalTo = e.from < e.to ? e.to : e.from;
		const key = `${canonicalFrom}\0${canonicalTo}`;
		const entry = byPair.get(key) ?? {
			from: canonicalFrom,
			to: canonicalTo,
			forward: 0,
			backward: 0
		};
		if (e.from === canonicalFrom) entry.forward += e.count;
		else entry.backward += e.count;
		byPair.set(key, entry);
	}
	return Array.from(byPair.entries()).map(([key, v]) => ({
		id: `edge-group:${key}`,
		from: v.from,
		to: v.to,
		forwardCount: v.forward,
		backwardCount: v.backward,
		bidirectional: v.forward > 0 && v.backward > 0
	}));
}
