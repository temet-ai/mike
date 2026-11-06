import { PITCH, type ChipRect, type NodeKind, type NodeState } from '$lib/board/types';
import type { BoardInput, GridLayout, StripLayout } from '$lib/board/router';
import type { IconKey } from '$lib/components/shared/board/icons';
import type { ColumnControl, ColumnState } from '$lib/components/shared/board/BoardColumn.svelte';
import { activitiesOfActor } from './actorActivities';
import { bornAtMs } from './born';
import {
	buildAgentIdentities,
	buildGroups,
	toolFamilyOf,
	type FlowAgentIdentity
} from './grouping';
import { rolesOf, type FlowRole } from './roles';
import { chipFor, type MikeChip } from './serviceRoles';
import { RECENT_EVENT_WINDOW_MS } from './traceModel';
import { buildCounts } from './counts';
import { isMessageDelivery } from './messagePhases';
import type { FlowActivity, FlowDataset } from './types';

const STRIP_X = 926;
const STRIP_W = 230;
const OUT_X = 1180;
const OUT_W = 176;
const S_X = 40;
const S_W = 216;
const A_X = 264;
const A_W = 574;
const AX0 = A_X + 28;
const AY0 = 100;
const COLS = 3;
const PITCH_X = 192;
const PITCH_Y = 66;
const CW = 150;
const CH = 40;
const TERM_HEAD_Y = 306;

const REQUEST_VISUAL = ['r_capture', 'filters', 'composer', 'inject', 'compress'] as const;
const RESPONSE_VISUAL = ['s_capture', 'intercept', 'analyzers', 'extract', 'persist'] as const;
const BACKGROUND_VISUAL = ['judge', 'verifier', 'embed', 'mailbox', 'upkeep'] as const;

export const MIKE_OUT_PAIRS: ReadonlyArray<readonly [string, string]> = [
	['judge', 'openrouter'],
	['verifier', 'openrouter'],
	['embed', 'ollama']
];

const MAX_WINDOW_PAIRS = 40;

const TOOL_CHIP_KEYS: ReadonlySet<string> = new Set(['shell', 'files', 'browser', 'mike', 'xmcp']);

export interface LayoutChip {
	readonly key: string;
	readonly kind: NodeKind;
	readonly icon: IconKey;
	readonly x: number;
	readonly y: number;
	readonly w: number;
	readonly h: number;
	readonly name: string;
	readonly sub: string;
	readonly state: NodeState;
	readonly since: number | null;
}

export interface ColumnSpec {
	readonly key: string;
	readonly label: string;
	readonly x: number;
	readonly w: number;
	readonly count?: number;
	readonly summary?: string;
	readonly state?: ColumnState;
	readonly control?: ColumnControl;
}

export interface SectionSpec {
	readonly key: string;
	readonly columnKey: string;
	readonly label: string;
	readonly y: number;
	readonly count?: number;
	readonly summary?: string;
	readonly state?: ColumnState;
}

export interface BoardLayoutOpts {
	readonly stageW: number;
	readonly collapsed: boolean;
	readonly atMs: number;
	readonly selected: string | null;
	readonly calls: ReadonlyArray<readonly [string, string]>;
	readonly events: ReadonlyArray<readonly [string, string]>;
	readonly mikeOut: ReadonlyArray<readonly [string, string]>;
}

export interface BoardLayoutResult {
	readonly input: BoardInput;
	readonly chips: readonly LayoutChip[];
	readonly columns: readonly ColumnSpec[];
	readonly sections: readonly SectionSpec[];
	readonly H: number;
	readonly keyOfActor: ReadonlyMap<string, string>;
	readonly actorOfKey: ReadonlyMap<string, string>;
}

export interface ActorKeyMaps {
	readonly keyOfActor: Map<string, string>;
	readonly actorOfKey: Map<string, string>;
	readonly agentIdentities: readonly FlowAgentIdentity[];
	readonly coordinatorId: string | null;
	readonly peerId: string | null;
	readonly roles: ReadonlyMap<string, FlowRole>;
}

function toolFamilyKey(role: FlowRole, label: string): string {
	if (role === 'mike-mcp') return 'mike';
	if (role.startsWith('other-mcp:')) return 'xmcp';
	const family = toolFamilyOf(label);
	return family === 'other' ? 'files' : family;
}

export function buildActorKeyMaps(dataset: FlowDataset): ActorKeyMaps {
	const roles = rolesOf(dataset.actors.values(), dataset.sessionId);
	const keyOfActor = new Map<string, string>();
	const actorOfKey = new Map<string, string>();
	const set = (actorId: string, key: string): void => {
		keyOfActor.set(actorId, key);
		if (!actorOfKey.has(key)) actorOfKey.set(key, actorId);
	};

	let coordinatorId: string | null = null;
	let peerId: string | null = null;
	for (const actor of dataset.actors.values()) {
		const role = roles.get(actor.id);
		if (role === 'coordinator') {
			coordinatorId = actor.id;
			set(actor.id, 'coord');
		} else if (role === 'peer' && peerId === null) {
			peerId = actor.id;
			set(actor.id, 'peer');
		} else if (role === 'sidecars') {
			set(actor.id, 'side');
		} else if (role === 'upstream') {
			set(actor.id, 'model');
		} else if (actor.id.startsWith('mailbox:')) {
			set(actor.id, 'mailbox');
		} else if (
			role === 'local-tools' ||
			role === 'mike-mcp' ||
			(role && role.startsWith('other-mcp:'))
		) {
			set(actor.id, toolFamilyKey(role, actor.label));
		}
	}

	const agentIdentities = buildAgentIdentities(dataset);
	agentIdentities.forEach((identity, i) => {
		const key = `w${i}`;
		const preferred =
			identity.workerActorIds[0] ?? identity.dispatchActorId ?? identity.actorIds[0];
		if (preferred !== undefined) set(preferred, key);
		for (const actorId of identity.actorIds) set(actorId, key);
	});

	return { keyOfActor, actorOfKey, agentIdentities, coordinatorId, peerId, roles };
}

export interface DerivedConnections {
	readonly calls: ReadonlyArray<readonly [string, string]>;
	readonly events: ReadonlyArray<readonly [string, string]>;
	readonly momentFallback: boolean;
}

function addPair(set: Map<string, [string, string]>, a: string, b: string): void {
	set.set(`${a}\u0000${b}`, [a, b]);
}

function targetActorIdFor(
	dataset: FlowDataset,
	activity: FlowActivity,
	coordinatorId: string | null
): string | null {
	for (const relationship of dataset.relationships.values()) {
		if (relationship.kind !== 'activity_request' || relationship.from_id !== activity.id) continue;
		if (relationship.to_id === null) continue;
		const targetActivity = dataset.activities.get(relationship.to_id);
		if (targetActivity) return targetActivity.actor_id;
	}
	return coordinatorId;
}

function callPairsInWindow(
	dataset: FlowDataset,
	maps: ActorKeyMaps,
	atMs: number | null
): Map<string, [string, string]> {
	const pairs = new Map<string, [string, string]>();
	for (const relationship of dataset.relationships.values()) {
		if (relationship.kind !== 'request_tool' && relationship.kind !== 'dispatch_tool') continue;
		if (relationship.from_id === null || relationship.to_id === null) continue;
		if (atMs !== null && (relationship.atMs === null || relationship.atMs > atMs)) continue;
		const fromActivity = dataset.activities.get(relationship.from_id);
		const toActivity = dataset.activities.get(relationship.to_id);
		if (!fromActivity || !toActivity) continue;
		const agentKey = maps.keyOfActor.get(fromActivity.actor_id);
		const toolKey = maps.keyOfActor.get(toActivity.actor_id);
		if (agentKey === undefined || toolKey === undefined) continue;
		addPair(pairs, agentKey, toolKey);
	}
	return pairs;
}

function eventPairsInWindow(
	dataset: FlowDataset,
	maps: ActorKeyMaps,
	atMs: number | null
): Map<string, [string, string]> {
	const pairs = new Map<string, [string, string]>();
	for (const activity of dataset.activities.values()) {
		if (activity.atMs === null) continue;
		if (atMs !== null && activity.atMs > atMs) continue;
		const actor = dataset.actors.get(activity.actor_id);
		if (!actor) continue;
		const chip = chipFor(actor, activity);
		if (chip === null) continue;
		const targetId = targetActorIdFor(dataset, activity, maps.coordinatorId);
		if (targetId === null) continue;
		const targetKey = maps.keyOfActor.get(targetId);
		if (targetKey === undefined) continue;
		addPair(pairs, chip, targetKey);
	}
	return pairs;
}

export function deriveConnections(
	dataset: FlowDataset,
	atMs: number,
	maps: ActorKeyMaps
): DerivedConnections {
	const wholeCalls = callPairsInWindow(dataset, maps, null);
	const wholeEvents = eventPairsInWindow(dataset, maps, null);
	if (wholeCalls.size + wholeEvents.size <= MAX_WINDOW_PAIRS) {
		return {
			calls: [...wholeCalls.values()],
			events: [...wholeEvents.values()],
			momentFallback: false
		};
	}
	const momentCalls = callPairsInWindow(dataset, maps, atMs);
	const momentEvents = eventPairsInWindow(dataset, maps, atMs);
	return {
		calls: [...momentCalls.values()],
		events: [...momentEvents.values()],
		momentFallback: true
	};
}

function pad2(n: number): string {
	return String(n).padStart(2, '0');
}
function formatDone(ms: number): string {
	const d = new Date(ms);
	return `DONE ${pad2(d.getUTCHours())}:${pad2(d.getUTCMinutes())}:${pad2(d.getUTCSeconds())}`;
}
function capitalize(word: string): string {
	return word.length ? word[0]!.toUpperCase() + word.slice(1) : word;
}

interface RequestEvidence {
	open: number;
	error: FlowActivity | null;
	lastClosedMs: number | null;
}

function requestEvidence(
	dataset: FlowDataset,
	actorIds: readonly string[],
	atMs: number
): RequestEvidence {
	let open = 0;
	let error: FlowActivity | null = null;
	let lastClosedMs: number | null = null;
	for (const actorId of actorIds)
		for (const activity of activitiesOfActor(dataset, actorId)) {
			if (activity.kind !== 'requests' || activity.time_semantics !== 'capture_interval') continue;
			if (activity.atMs === null || activity.atMs > atMs) continue;
			const closed = activity.endedAtMs !== null && activity.endedAtMs <= atMs;
			if (!closed) {
				open += 1;
				continue;
			}
			if (activity.outcome === 'error' && (error === null || activity.atMs < error.atMs!))
				error = activity;
			if (lastClosedMs === null || activity.endedAtMs! > lastClosedMs)
				lastClosedMs = activity.endedAtMs!;
		}
	return { open, error, lastClosedMs };
}

function agentChipContent(
	dataset: FlowDataset,
	identity: FlowAgentIdentity,
	atMs: number
): { sub: string; state: NodeState } {
	if (identity.pending) return { sub: 'NOT RESOLVED', state: 'pending' };
	const evidence = requestEvidence(dataset, identity.actorIds, atMs);
	if (evidence.open > 0) return { sub: `${evidence.open} IN FLIGHT`, state: 'busy' };
	if (evidence.error) {
		const status = evidence.error.metadata.status_code;
		return { sub: status === null ? 'ERR' : `ERR ${status}`, state: 'error' };
	}
	if (evidence.lastClosedMs !== null && atMs - evidence.lastClosedMs <= RECENT_EVENT_WINDOW_MS) {
		return { sub: formatDone(evidence.lastClosedMs), state: 'recent' };
	}
	return { sub: 'IDLE', state: 'quiet' };
}

function chipFiredRecently(dataset: FlowDataset, atMs: number, target: MikeChip): boolean {
	for (const activity of dataset.activities.values()) {
		if (activity.atMs === null || activity.atMs > atMs) continue;
		const sinceFired = atMs - activity.atMs;
		if (sinceFired < 0 || sinceFired > RECENT_EVENT_WINDOW_MS) continue;
		const actor = dataset.actors.get(activity.actor_id);
		if (!actor) continue;
		if (chipFor(actor, activity) === target) return true;
	}
	return false;
}

const SERVICE_NAME: Readonly<Record<MikeChip, string>> = {
	r_capture: 'CAPTURE',
	filters: 'FILTERS',
	composer: 'COMPOSER',
	inject: 'INJECT',
	compress: 'COMPRESS',
	s_capture: 'CAPTURE',
	intercept: 'INTERCEPT',
	analyzers: 'ANALYZERS',
	extract: 'EXTRACT',
	persist: 'PERSIST',
	judge: 'JUDGE',
	verifier: 'VERIFIER',
	embed: 'EMBED',
	mailbox: 'MAILBOX',
	upkeep: 'UPKEEP'
};

function serviceIcon(key: MikeChip): IconKey {
	return key === 'r_capture' || key === 's_capture' ? 'capture' : (key as IconKey);
}

function serviceVisualState(key: MikeChip): 'busy' | 'response' | 'hot' {
	if ((REQUEST_VISUAL as readonly string[]).includes(key)) return 'busy';
	if ((RESPONSE_VISUAL as readonly string[]).includes(key)) return 'response';
	return 'hot';
}

function firingRuleCount(dataset: FlowDataset): number | null {
	for (const activity of dataset.activities.values()) {
		if (activity.kind !== 'manifest_stages' || activity.metadata.stage !== 'filter') continue;
		if (activity.metadata.firing_rule_count != null) return activity.metadata.firing_rule_count;
	}
	return null;
}

function serviceChipContent(
	dataset: FlowDataset,
	atMs: number,
	key: MikeChip,
	lessonInjectionsSoFar: number,
	messagesQueued: number
): { sub: string; state: NodeState } {
	const lit = chipFiredRecently(dataset, atMs, key);
	const state: NodeState = lit ? serviceVisualState(key) : 'quiet';
	if (key === 'r_capture') return { sub: 'ORIGINAL', state };
	if (key === 's_capture') return { sub: 'RESPONSE', state };
	if (key === 'filters') {
		const n = firingRuleCount(dataset);
		return { sub: n === null ? 'IDLE' : `${n} RULES`, state };
	}
	if (key === 'inject')
		return { sub: lessonInjectionsSoFar > 0 ? `${lessonInjectionsSoFar} LESSONS` : 'IDLE', state };
	if (key === 'mailbox')
		return { sub: messagesQueued > 0 ? `${messagesQueued} MSG` : 'IDLE', state };
	return { sub: 'IDLE', state };
}

const TOOL_IDLE_SUB: Readonly<Record<'shell' | 'files' | 'browser' | 'mike' | 'xmcp', string>> = {
	shell: 'BASH',
	files: 'READ · EDIT · WRITE',
	browser: 'COMPUTER USE',
	mike: 'LOCAL',
	xmcp: 'REMOTE'
};
const TOOL_ICON: Readonly<Record<'shell' | 'files' | 'browser' | 'mike' | 'xmcp', IconKey>> = {
	shell: 'shell',
	files: 'files',
	browser: 'browser',
	mike: 'mcp',
	xmcp: 'mcp'
};

function otherMcpServer(dataset: FlowDataset, roles: ReadonlyMap<string, FlowRole>): string | null {
	for (const actor of dataset.actors.values()) {
		const role = roles.get(actor.id);
		if (role && role.startsWith('other-mcp:')) return role.slice('other-mcp:'.length);
	}
	return null;
}

function toolFamilyForCounts(
	key: 'shell' | 'files' | 'browser' | 'mike' | 'xmcp',
	server: string | null
): string {
	if (key === 'mike') return 'mike-mcp';
	if (key === 'xmcp') return server ?? 'xmcp';
	return key;
}

export function buildBoardLayout(dataset: FlowDataset, opts: BoardLayoutOpts): BoardLayoutResult {
	const maps = buildActorKeyMaps(dataset);
	const { roles, agentIdentities, coordinatorId, peerId } = maps;
	const { atMs, collapsed, stageW } = opts;
	const counts = buildCounts(dataset, atMs);
	const chips: LayoutChip[] = [];
	const push = (chip: Omit<LayoutChip, 'since'> & { since?: number | null }): void =>
		void chips.push({ since: null, ...chip });
	const bornOf = (identity: FlowAgentIdentity): number | null =>
		bornAtMs(dataset, identity.dispatchActorId ?? identity.workerActorIds[0]!);
	const bornBy = (since: number | null): boolean => since !== null && since <= atMs;
	const earliestOf = (ids: readonly string[]): number | null =>
		ids.reduce<number | null>((min, id) => {
			const born = bornAtMs(dataset, id);
			return born !== null && (min === null || born < min) ? born : min;
		}, null);

	const anthropicOpen = anyOpenCaptureCount(dataset, atMs);
	const upstreamBusy = anthropicOpen > 0;
	push({
		key: 'model',
		kind: 'outside',
		icon: 'model',
		x: OUT_X,
		y: 44,
		w: OUT_W,
		h: 48,
		name: 'Anthropic',
		sub: upstreamBusy ? `${anthropicOpen} IN FLIGHT` : 'MODEL API',
		state: upstreamBusy ? 'busy' : 'quiet'
	});
	const routerBusy =
		chipFiredRecently(dataset, atMs, 'judge') || chipFiredRecently(dataset, atMs, 'verifier');
	push({
		key: 'openrouter',
		kind: 'outside',
		icon: 'router',
		x: OUT_X,
		y: 172,
		w: OUT_W,
		h: 48,
		name: 'OpenRouter',
		sub: 'JUDGE · STEERER LLM',
		state: routerBusy ? 'busy' : 'quiet'
	});
	const embedBusy = chipFiredRecently(dataset, atMs, 'embed');
	push({
		key: 'ollama',
		kind: 'outside',
		icon: 'embed',
		x: OUT_X,
		y: 300,
		w: OUT_W,
		h: 48,
		name: 'Ollama',
		sub: 'NOMIC-EMBED-TEXT',
		state: embedBusy ? 'busy' : 'quiet'
	});
	const internetOpen = openInternetCalls(dataset, roles, atMs);
	push({
		key: 'internet',
		kind: 'outside',
		icon: 'internet',
		x: OUT_X,
		y: 428,
		w: OUT_W,
		h: 48,
		name: 'Internet',
		sub: internetOpen > 0 ? `${internetOpen} IN FLIGHT` : 'WEB · SEARCH · MCP',
		state: internetOpen > 0 ? 'busy' : 'quiet'
	});

	const REQ_X = STRIP_X + 22;
	const RES_X = STRIP_X + 120;
	const BG_X = STRIP_X + 71;
	REQUEST_VISUAL.forEach((key, i) => {
		const content = serviceChipContent(
			dataset,
			atMs,
			key,
			counts.lessonInjectionsSoFar,
			counts.messagesQueued
		);
		push({
			key,
			kind: 'service',
			icon: serviceIcon(key),
			x: REQ_X,
			y: 100 + i * 52,
			w: 92,
			h: 28,
			name: SERVICE_NAME[key],
			...content
		});
	});
	RESPONSE_VISUAL.forEach((key, i) => {
		const content = serviceChipContent(
			dataset,
			atMs,
			key,
			counts.lessonInjectionsSoFar,
			counts.messagesQueued
		);
		push({
			key,
			kind: 'service',
			icon: serviceIcon(key),
			x: RES_X,
			y: 100 + i * 52,
			w: 92,
			h: 28,
			name: SERVICE_NAME[key],
			...content
		});
	});
	BACKGROUND_VISUAL.forEach((key, i) => {
		const content = serviceChipContent(
			dataset,
			atMs,
			key,
			counts.lessonInjectionsSoFar,
			counts.messagesQueued
		);
		push({
			key,
			kind: 'service',
			icon: serviceIcon(key),
			x: BG_X,
			y: 380 + i * 48,
			w: 92,
			h: 28,
			name: SERVICE_NAME[key],
			...content
		});
	});

	const coordEvidence = coordinatorId
		? requestEvidence(dataset, [coordinatorId], atMs)
		: { open: 0 };
	push({
		key: 'coord',
		kind: 'session',
		icon: 'session',
		x: 56,
		y: 100,
		w: 184,
		h: 56,
		name: coordinatorId ? `Session ${coordinatorId.slice(-8)}` : 'Session',
		sub: coordEvidence.open > 0 ? `${coordEvidence.open} IN FLIGHT` : 'IDLE',
		state: coordEvidence.open > 0 ? 'busy' : 'quiet'
	});
	const peerRecent = peerId !== null && peerSentRecently(dataset, peerId, atMs);
	const peerSince =
		peerId !== null ? (bornAtMs(dataset, peerId) ?? Number.POSITIVE_INFINITY) : null;
	if (peerId !== null)
		push({
			key: 'peer',
			kind: 'peer',
			icon: 'peer',
			x: 56,
			y: 172,
			w: 184,
			h: 48,
			name: `Peer ${peerId.slice(-8)}`,
			sub: peerRecent ? 'RECENT' : 'IDLE',
			state: peerRecent ? 'recent' : 'quiet',
			since: peerSince
		});
	const sideGroup = buildGroups(dataset).find((g) => g.key === 'sidecars') ?? null;
	const sideTotal = sideGroup ? [...sideGroup.counts.values()].reduce((a, b) => a + b, 0) : 0;
	const sideSince = sideGroup
		? (earliestOf(sideGroup.memberIds) ?? Number.POSITIVE_INFINITY)
		: null;
	if (sideGroup)
		push({
			key: 'side',
			kind: 'group',
			icon: 'group',
			x: 56,
			y: 236,
			w: 184,
			h: 48,
			name: 'Sidecars',
			sub: `${sideTotal} · ${sideGroup.memberIds.length} CLASSES`,
			state: 'quiet',
			since: sideSince
		});
	push({
		key: 'term',
		kind: 'terminal',
		icon: 'terminal',
		x: 56,
		y: TERM_HEAD_Y + 18,
		w: 184,
		h: 36,
		name: 'Terminal · claude',
		sub: 'OPERATOR',
		state: 'plain'
	});

	const rows = Math.max(8, Math.ceil(agentIdentities.length / COLS));
	const agentSince = agentIdentities.map((identity) => bornOf(identity));
	const bornAgents = agentIdentities.filter((_identity, i) => bornBy(agentSince[i]!));
	let busyAgents = 0;
	let errorAgents = 0;
	let idleAgents = 0;
	if (collapsed) {
		bornAgents.forEach((identity) => {
			const { state } = agentChipContent(dataset, identity, atMs);
			if (state === 'busy') busyAgents += 1;
			else if (state === 'error') errorAgents += 1;
			else idleAgents += 1;
		});
		const firstBorn = agentSince.reduce<number | null>(
			(min, born) => (born !== null && (min === null || born < min) ? born : min),
			null
		);
		push({
			key: 'agents',
			kind: 'group',
			icon: 'agent',
			x: 280,
			y: AY0,
			w: 542,
			h: 40,
			name: 'Agents',
			sub: `${bornAgents.length} OF ${agentIdentities.length} · ${busyAgents} IN FLIGHT · ${errorAgents} ERROR · ${idleAgents} IDLE`,
			state: busyAgents > 0 ? 'busy' : 'quiet',
			since: firstBorn ?? Number.POSITIVE_INFINITY
		});
	} else {
		agentIdentities.forEach((identity, i) => {
			const c = i % COLS;
			const r = Math.floor(i / COLS);
			const { sub, state } = agentChipContent(dataset, identity, atMs);
			const since = agentSince[i]!;
			if (bornBy(since)) {
				if (state === 'busy') busyAgents += 1;
				else if (state === 'error') errorAgents += 1;
				else idleAgents += 1;
			}
			push({
				key: `w${i}`,
				kind: 'agent',
				icon: 'agent',
				x: AX0 + c * PITCH_X,
				y: AY0 + r * PITCH_Y,
				w: CW,
				h: CH,
				name: identity.label,
				sub,
				state,
				since: since ?? Number.POSITIVE_INFINITY
			});
		});
	}
	const gridBottom = collapsed ? AY0 + 40 : AY0 + (rows - 1) * PITCH_Y + CH;
	const toolHeadY = gridBottom + 22;
	const bandY0 = toolHeadY + 16;
	const toolsY = bandY0 + 8 * PITCH + 10;

	const server = otherMcpServer(dataset, roles);
	const toolOpen = (family: 'shell' | 'files' | 'browser' | 'mike' | 'xmcp'): number =>
		counts.openToolCallsByFamily.get(toolFamilyForCounts(family, server)) ?? 0;
	const toolSince = new Map<string, number>();
	for (const [actorId, key] of maps.keyOfActor) {
		if (!TOOL_CHIP_KEYS.has(key)) continue;
		const born = bornAtMs(dataset, actorId);
		if (born === null) continue;
		const known = toolSince.get(key);
		if (known === undefined || born < known) toolSince.set(key, born);
	}
	let toolsShown = 0;
	const toolChip = (
		key: 'shell' | 'files' | 'browser' | 'mike' | 'xmcp',
		x: number,
		y: number,
		name: string
	): void => {
		const n = toolOpen(key);
		const since = toolSince.get(key) ?? Number.POSITIVE_INFINITY;
		if (since <= atMs) toolsShown += 1;
		push({
			key,
			kind: 'tool',
			icon: TOOL_ICON[key],
			x,
			y,
			w: 160,
			h: 40,
			name,
			sub: n > 0 ? `${n} IN FLIGHT` : TOOL_IDLE_SUB[key],
			state: n > 0 ? 'busy' : 'quiet',
			since
		});
	};
	toolChip('shell', AX0 - 12, toolsY, 'Shell');
	toolChip('files', AX0 - 12 + 176, toolsY, 'Files');
	toolChip('browser', AX0 - 12 + 352, toolsY, 'Browser');
	toolChip('mike', AX0 - 12 + 88, toolsY + 52, 'Mike MCP');
	toolChip('xmcp', AX0 - 12 + 264, toolsY + 52, server ? `${capitalize(server)} MCP` : 'Other MCP');

	const upkeepChip = chips.find((c) => c.key === 'upkeep')!;
	const footY = Math.max(toolsY + 52 + 40 + 18, upkeepChip.y + upkeepChip.h + 12);
	const H = Math.max(footY + 20, 640);

	const selectedActorId = opts.selected;
	const selectedKey =
		selectedActorId === null ? null : (maps.keyOfActor.get(selectedActorId) ?? null);
	const chipsWithSelection = chips.map((chip) =>
		selectedKey !== null && chip.key === selectedKey
			? { ...chip, state: 'selected' as const }
			: chip
	);

	const chipRects: ChipRect[] = chipsWithSelection.map((c) => ({
		key: c.key,
		kind: c.kind,
		x: c.x,
		y: c.y,
		w: c.w,
		h: c.h
	}));
	const strip: StripLayout = {
		x: STRIP_X,
		w: STRIP_W,
		spineReq: STRIP_X + 10,
		spineRes: STRIP_X + 117,
		spineReqR: STRIP_X + STRIP_W - 8,
		spineResR: STRIP_X + STRIP_W - 11,
		spineEvt: STRIP_X + 4,
		floorReq: 349,
		floorRes: 346,
		evtY: 59
	};
	const grid: GridLayout = {
		cols: COLS,
		rows,
		pitchX: PITCH_X,
		pitchY: PITCH_Y,
		x0: AX0,
		y0: AY0,
		columnX: A_X,
		columnW: A_W
	};
	const normalize = (
		pairs: ReadonlyArray<readonly [string, string]>
	): Array<readonly [string, string]> => {
		if (!collapsed) return pairs.slice();
		const seen = new Map<string, [string, string]>();
		for (const [a, b] of pairs) {
			const na = /^w\d+$/.test(a) ? 'agents' : a;
			const nb = /^w\d+$/.test(b) ? 'agents' : b;
			seen.set(`${na}\u0000${nb}`, [na, nb]);
		}
		return [...seen.values()];
	};
	const fallbackAgentKey = collapsed
		? 'agents'
		: agentIdentities.length
			? `w${agentIdentities.length - 1}`
			: undefined;
	const routerSelected = collapsed ? 'agents' : (selectedKey ?? fallbackAgentKey);

	const input: BoardInput = {
		stageW,
		stageH: H,
		strip,
		outsideX: OUT_X,
		chips: chipRects,
		grid,
		collapsed,
		selected: routerSelected,
		calls: normalize(opts.calls),
		events: normalize(opts.events),
		mikeOut: opts.mikeOut,
		bandY0,
		footY
	};

	const columns: ColumnSpec[] = [
		{
			key: 'session',
			label: 'SESSION',
			x: S_X,
			w: S_W,
			count:
				1 +
				(peerSince !== null && peerSince <= atMs ? 1 : 0) +
				(sideSince !== null && sideSince <= atMs ? 1 : 0),
			summary: coordEvidence.open > 0 ? `${coordEvidence.open} IN FLIGHT` : undefined,
			state: coordEvidence.open > 0 ? 'busy' : 'quiet'
		},
		{
			key: 'agents',
			label: 'AGENTS',
			x: A_X,
			w: A_W,
			count: bornAgents.length,
			summary:
				busyAgents > 0
					? `${busyAgents} IN FLIGHT${errorAgents > 0 ? ` · ${errorAgents} ERROR` : ''}`
					: undefined,
			state: busyAgents > 0 ? 'busy' : 'quiet',
			control: agentIdentities.length === 0 ? 'none' : collapsed ? 'expand' : 'collapse'
		},
		{ key: 'strip', label: 'MIKE · PROXY · THE BOUNDARY', x: STRIP_X, w: STRIP_W },
		{ key: 'outside', label: 'OUTSIDE THE BOUNDARY', x: OUT_X, w: OUT_W, count: 4 }
	];
	const totalToolsOpen = (['shell', 'files', 'browser', 'mike', 'xmcp'] as const).reduce(
		(sum, k) => sum + toolOpen(k),
		0
	);
	const sections: SectionSpec[] = [
		{ key: 'terminal', columnKey: 'session', label: 'TERMINAL', y: TERM_HEAD_Y },
		{
			key: 'tools',
			columnKey: 'agents',
			label: 'TOOLS · COMPUTER USE · MCP SERVERS',
			y: toolHeadY,
			count: toolsShown,
			summary: totalToolsOpen > 0 ? `${totalToolsOpen} IN FLIGHT` : undefined,
			state: totalToolsOpen > 0 ? 'busy' : 'quiet'
		},
		{ key: 'background', columnKey: 'strip', label: 'BACKGROUND · SCHEDULED', y: 364 }
	];

	return {
		input,
		chips: chipsWithSelection,
		columns,
		sections,
		H,
		keyOfActor: maps.keyOfActor,
		actorOfKey: maps.actorOfKey
	};
}

function anyOpenCaptureCount(dataset: FlowDataset, atMs: number): number {
	let open = 0;
	for (const activity of dataset.activities.values()) {
		if (activity.kind !== 'requests' || activity.time_semantics !== 'capture_interval') continue;
		if (activity.atMs === null || activity.atMs > atMs) continue;
		const closed = activity.endedAtMs !== null && activity.endedAtMs <= atMs;
		if (!closed) open += 1;
	}
	return open;
}

function peerSentRecently(dataset: FlowDataset, peerId: string, atMs: number): boolean {
	for (const activity of dataset.activities.values()) {
		if (activity.actor_id !== peerId || !isMessageDelivery(activity)) continue;
		if (activity.atMs === null || activity.atMs > atMs) continue;
		if (atMs - activity.atMs <= RECENT_EVENT_WINDOW_MS) return true;
	}
	return false;
}

function openInternetCalls(
	dataset: FlowDataset,
	roles: ReadonlyMap<string, FlowRole>,
	atMs: number
): number {
	const resultedWireIds = new Set<string>();
	for (const activity of dataset.activities.values()) {
		if (activity.kind !== 'results' || activity.atMs === null || activity.atMs > atMs) continue;
		const wireId = activity.metadata.wire_tool_use_id;
		if (wireId !== null) resultedWireIds.add(wireId);
	}
	let n = 0;
	for (const activity of dataset.activities.values()) {
		if (activity.kind !== 'tools' || activity.atMs === null || activity.atMs > atMs) continue;
		const wireId = activity.metadata.wire_tool_use_id;
		if (wireId !== null && resultedWireIds.has(wireId)) continue;
		const actor = dataset.actors.get(activity.actor_id);
		if (!actor) continue;
		const role = roles.get(actor.id);
		const isInternet =
			role === 'local-tools'
				? toolFamilyOf(actor.label) === 'browser'
				: role?.startsWith('other-mcp:');
		if (isInternet) n += 1;
	}
	return n;
}
