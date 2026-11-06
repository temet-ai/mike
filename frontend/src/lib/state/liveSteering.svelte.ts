import { proxyEvents as defaultProxyEvents } from '$lib/ws.svelte';
import type { ProxyEvent } from '$lib/types';

export const FEED_RING_CAP = 100;

export interface SteererCounters {
	steererName: string;
	displayName: string | null;
	fires: number;
	suppressions: number;
	noDrifts: number;
	lastFireAt: string | null;
}

export interface LiveInterventionEvent {
	key: number;
	interventionId: number;
	sessionId: string;
	sessionUuid: string;
	steererName: string | null;
	steererDisplayName: string | null;
	authority: string | null;
	mode: string | null;
	verdict: string | null;
	confidence: number | null;
	offThreadTopic: string | null;
	payloadText: string | null;
	receivedAt: number;
}

export interface LiveSteeringStore {
	readonly feed: LiveInterventionEvent[];
	readonly perSteerer: Map<string, SteererCounters>;
	readonly enrolledSessionCount: number;
	readonly awaitingFirstEvent: boolean;
	readonly wsState: string;

	start(): () => void;

	__drainForTests(): void;
}

interface Deps {
	proxyEvents?: typeof defaultProxyEvents;
}

let _keyCounter = 0;
function nextKey(): number {
	return ++_keyCounter;
}

export function createLiveSteeringStore(deps: Deps = {}): LiveSteeringStore {
	const ws = deps.proxyEvents ?? defaultProxyEvents;

	const s = $state<{
		feed: LiveInterventionEvent[];
		perSteerer: Map<string, SteererCounters>;
		enrolledSessions: Set<string>;
		awaitingFirstEvent: boolean;
	}>({
		feed: [],
		perSteerer: new Map(),
		enrolledSessions: new Set(),
		awaitingFirstEvent: true
	});

	let processedEvents = 0;
	let stopEffect: (() => void) | null = null;
	let started = false;

	function handleSteeringFired(
		ev: Extract<ProxyEvent, { type: 'SteeringFired' }>,
		nowMs: number
	): void {
		const key = nextKey();
		const entry: LiveInterventionEvent = {
			key,
			interventionId: ev.intervention_id,
			sessionId: ev.session_id,
			sessionUuid: ev.session_uuid,
			steererName: ev.steerer_name,
			steererDisplayName: ev.steerer_display_name,
			authority: ev.authority,
			mode: ev.mode,
			verdict: ev.verdict,
			confidence: ev.confidence,
			offThreadTopic: ev.off_thread_topic,
			payloadText: ev.payload_text,
			receivedAt: nowMs
		};

		s.feed = [entry, ...s.feed].slice(0, FEED_RING_CAP);

		s.enrolledSessions = new Set([...s.enrolledSessions, ev.session_uuid]);

		const name = ev.steerer_name ?? '__unknown__';
		const existing = s.perSteerer.get(name);
		const isFire = ev.verdict === 'fired';
		const isSuppress =
			ev.verdict === 'suppressed_debounce' || ev.verdict === 'suppressed_arbitration';
		const isNoDrift = ev.verdict === 'no_drift';

		const updated: SteererCounters = existing
			? {
					...existing,
					fires: existing.fires + (isFire ? 1 : 0),
					suppressions: existing.suppressions + (isSuppress ? 1 : 0),
					noDrifts: existing.noDrifts + (isNoDrift ? 1 : 0),
					lastFireAt: isFire ? new Date(nowMs).toISOString() : existing.lastFireAt,
					displayName: ev.steerer_display_name ?? existing.displayName
				}
			: {
					steererName: name,
					displayName: ev.steerer_display_name,
					fires: isFire ? 1 : 0,
					suppressions: isSuppress ? 1 : 0,
					noDrifts: isNoDrift ? 1 : 0,
					lastFireAt: isFire ? new Date(nowMs).toISOString() : null
				};

		s.perSteerer = new Map(s.perSteerer).set(name, updated);
		s.awaitingFirstEvent = false;
	}

	function drain(): void {
		const nowMs = Date.now();
		const total = ws.totalEvents;
		if (total === processedEvents) return;

		const start = total < processedEvents ? 0 : Math.max(processedEvents, total - 1000);
		for (let i = start; i < total; i++) {
			const ev = ws.at(i);
			if (!ev) continue;
			if (ev.type === 'SteeringFired') {
				handleSteeringFired(ev, nowMs);
			}
		}
		processedEvents = total;
	}

	function start(): () => void {
		if (started) return () => {};
		started = true;

		const cleanupRoot = $effect.root(() => {
			$effect(() => {
				void ws.totalEvents;
				void ws.laggedCount;
				void ws.state;
				drain();
			});
		});

		stopEffect = () => {
			cleanupRoot();
		};

		return () => {
			if (stopEffect) {
				stopEffect();
				stopEffect = null;
			}
			started = false;
		};
	}

	return {
		get feed() {
			return s.feed;
		},
		get perSteerer() {
			return s.perSteerer;
		},
		get enrolledSessionCount() {
			return s.enrolledSessions.size;
		},
		get awaitingFirstEvent() {
			return s.awaitingFirstEvent;
		},
		get wsState() {
			return ws.state;
		},
		start,
		__drainForTests: drain
	};
}
