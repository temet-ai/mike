import { fetchSessionFlowPage, type FetchSessionFlowPage } from '$lib/api/sessionFlow';
import type { SessionFlowActivity } from '$lib/api/generated/SessionFlowActivity';
import type { SessionFlowActor } from '$lib/api/generated/SessionFlowActor';
import type { SessionFlowAvailability } from '$lib/api/generated/SessionFlowAvailability';
import type { SessionFlowComponent } from '$lib/api/generated/SessionFlowComponent';
import type { SessionFlowCoverage } from '$lib/api/generated/SessionFlowCoverage';
import type { SessionFlowRelationship } from '$lib/api/generated/SessionFlowRelationship';
import { normalizeFlowDataset, preferFlowActivity } from './projection';
import type { FlowDataset } from './types';

export const FLOW_COMPONENTS = [
	'requests',
	'dispatches',
	'tools',
	'results',
	'messages',
	'worker_status',
	'manifest_stages',
	'lesson_injections',
	'verifier_evaluations',
	'judgments',
	'llm_calls',
	'embeddings',
	'consolidation',
	'interventions'
] as const satisfies readonly SessionFlowComponent[];
export interface FlowLiveMark {
	sessionId: string;
	requestId: string;
	family: 'StreamingStarted' | 'DiagnosticReady';
	receivedAtMs: number;
	statusCode?: number;
	ttfbMs?: number;
	count?: number;
}
export interface FlowStoreLimits {
	activities: number;
	relationships: number;
	actors: number;
	overlay: number;
	pagesPerTraversal: number;
}
const DEFAULT_LIMITS: FlowStoreLimits = {
	activities: 10000,
	relationships: 2000,
	actors: 256,
	overlay: 256,
	pagesPerTraversal: 1000
};
export interface SessionFlowStore {
	readonly dataset: FlowDataset;
	readonly coverage: FlowDataset['coverage'];
	readonly availability: SessionFlowAvailability | null;
	readonly activeSessionId: string | null;
	readonly loading: boolean;
	readonly error: Error | null;
	readonly stale: boolean;
	readonly staleReasons: readonly string[];
	readonly overlay: readonly FlowLiveMark[];
	readonly overlayHistoryUnavailable: boolean;
	load(sessionId: string): Promise<void>;
	loadEarlier(
		component: SessionFlowComponent
	): Promise<'loaded' | 'exhausted' | 'bounded' | 'inactive'>;
	invalidate(reason: string, components?: readonly SessionFlowComponent[]): void;
	retry(): Promise<void>;
	observeLive(mark: FlowLiveMark): void;
	clearLiveHistory(reason: string): void;
	deactivate(): void;
	dispose(): void;
}
interface ComponentData {
	actors: Map<string, SessionFlowActor>;
	activities: Map<string, SessionFlowActivity>;
	relationships: Map<string, SessionFlowRelationship>;
	coverage: SessionFlowCoverage;
}
export function createSessionFlowStore(
	options: {
		fetchPage?: FetchSessionFlowPage;
		components?: readonly SessionFlowComponent[];
		limits?: Partial<FlowStoreLimits>;
		continuationDelayMs?: number;
		compareRevision?: (table: string, incoming: string, previous: string) => number | null;
	} = {}
): SessionFlowStore {
	const fetchPage = options.fetchPage ?? fetchSessionFlowPage;
	const components = [...new Set(options.components ?? FLOW_COMPONENTS)];
	const limits = { ...DEFAULT_LIMITS, ...options.limits };
	const continuationDelayMs = options.continuationDelayMs ?? 0;
	if (!Number.isFinite(continuationDelayMs) || continuationDelayMs < 0)
		throw new RangeError('Invalid Flow continuation delay');
	for (const [key, value] of Object.entries(limits))
		if (
			!Number.isInteger(value) ||
			value < 1 ||
			value > DEFAULT_LIMITS[key as keyof FlowStoreLimits]
		)
			throw new RangeError(`Invalid Flow ${key} limit`);
	let dataset = $state.raw<FlowDataset>(normalizeFlowDataset({ sessionId: '' }));
	let availability = $state.raw<SessionFlowAvailability | null>(null);
	let activeSessionId = $state<string | null>(null);
	let loading = $state(false),
		error = $state.raw<Error | null>(null);
	let staleReasons = $state<readonly string[]>([]);
	let overlay = $state.raw<readonly FlowLiveMark[]>([]);
	let overlayHistoryUnavailable = $state(false);
	let epoch = 0,
		disposed = false,
		controller: AbortController | null = null;
	let running: Promise<void> | null = null;
	let scheduled = false;
	const authoritative = new Map<SessionFlowComponent, ComponentData>();
	const dirty = new Map<SessionFlowComponent, number>();
	const pending = new Set<SessionFlowComponent>();
	const failures = new Map<SessionFlowComponent, string>();
	const bounded = new Set<SessionFlowComponent>();
	function publish(): void {
		const actors = new Map<string, SessionFlowActor>(),
			activities = new Map<string, SessionFlowActivity>(),
			relationships = new Map<string, SessionFlowRelationship>();
		const coverage: SessionFlowCoverage[] = [];
		for (const [component, data] of authoritative) {
			for (const [id, actor] of data.actors) actors.set(id, actor);
			for (const [id, activity] of data.activities)
				activities.set(
					id,
					preferFlowActivity(activities.get(id), activity, options.compareRevision)
				);
			for (const [id, relationship] of data.relationships) relationships.set(id, relationship);
			coverage.push({
				...data.coverage,
				stale_reason: failures.get(component) ?? data.coverage.stale_reason
			});
		}
		dataset = normalizeFlowDataset({
			sessionId: activeSessionId ?? '',
			actors: [...actors.values()],
			activities: [...activities.values()],
			relationships: [...relationships.values()],
			coverage
		});
		staleReasons = [...new Set(failures.values())];
	}
	function fits(component: SessionFlowComponent, stage: ComponentData): boolean {
		const actors = new Set(stage.actors.keys()),
			activities = new Set(stage.activities.keys()),
			relationships = new Set(stage.relationships.keys());
		for (const [other, data] of authoritative)
			if (other !== component) {
				for (const id of data.actors.keys()) actors.add(id);
				for (const id of data.activities.keys()) activities.add(id);
				for (const id of data.relationships.keys()) relationships.add(id);
			}
		return (
			actors.size <= limits.actors &&
			activities.size <= limits.activities &&
			relationships.size <= limits.relationships
		);
	}
	function markPending(scope: readonly SessionFlowComponent[], reason: string): void {
		for (const component of scope)
			if (components.includes(component)) {
				dirty.set(component, (dirty.get(component) ?? 0) + 1);
				pending.add(component);
				failures.set(component, reason);
			}
		staleReasons = [...new Set(failures.values())];
	}
	function capped(
		component: SessionFlowComponent,
		stage: ComponentData | null,
		reason: string
	): void {
		failures.set(component, reason);
		bounded.add(component);
		const retained = authoritative.get(component) ?? stage;
		if (retained)
			authoritative.set(component, {
				...retained,
				coverage: { ...retained.coverage, exhausted: false, stale_reason: reason }
			});
		publish();
	}
	async function traverse(
		component: SessionFlowComponent,
		token: number,
		sessionId: string,
		signal: AbortSignal
	): Promise<boolean> {
		const generation = dirty.get(component) ?? 0;
		let stage: ComponentData | null = null,
			cursor: string | undefined;
		const cursors = new Set<string>();
		for (let pageNumber = 0; pageNumber < limits.pagesPerTraversal; pageNumber++) {
			const page = await fetchPage({ sessionId, component, cursor, limit: 100, signal });
			if (token !== epoch || signal.aborted) return false;
			if (
				page.version !== 1 ||
				page.session_id !== sessionId ||
				page.coverage.component !== component
			)
				throw new Error('Flow response contract or scope mismatch');
			availability = page.availability;
			if (typeof availability === 'string' || !('eligible' in availability)) {
				authoritative.clear();
				failures.clear();
				pending.clear();
				publish();
				return false;
			}
			const next: ComponentData = {
				actors: new Map(stage?.actors),
				activities: new Map(stage?.activities),
				relationships: new Map(stage?.relationships),
				coverage: { ...page.coverage, lower_id: stage?.coverage.lower_id ?? page.coverage.lower_id }
			};
			for (const actor of page.actors) next.actors.set(actor.id, actor);
			for (const activity of page.activities) {
				const retained = authoritative.get(component)?.activities.get(activity.id);
				const accepted = preferFlowActivity(retained, activity, options.compareRevision);
				next.activities.set(
					activity.id,
					preferFlowActivity(next.activities.get(activity.id), accepted, options.compareRevision)
				);
			}
			for (const relationship of page.relationships)
				next.relationships.set(relationship.id, relationship);
			if (!fits(component, next)) {
				capped(
					component,
					stage ?? {
						actors: new Map(),
						activities: new Map(),
						relationships: new Map(),
						coverage: { ...page.coverage, returned_through_id: null }
					},
					'Browser data limit reached; remaining coverage is unloaded'
				);
				return false;
			}
			stage = next;
			if (page.next_cursor === null) {
				if (!page.coverage.exhausted)
					throw new Error('Flow traversal ended without exhausted coverage');
				if ((dirty.get(component) ?? 0) !== generation) return true;
				authoritative.set(component, stage);
				failures.delete(component);
				bounded.delete(component);
				publish();
				return false;
			}
			if (page.coverage.exhausted || cursors.has(page.next_cursor))
				throw new Error('Invalid Flow continuation cursor');
			cursors.add(page.next_cursor);
			cursor = page.next_cursor;
		}
		capped(component, stage, 'Flow traversal page limit reached; remaining coverage is unloaded');
		return false;
	}
	function start(): Promise<void> {
		if (running) return running;
		if (disposed || activeSessionId === null || pending.size === 0) return Promise.resolve();
		const token = epoch,
			sessionId = activeSessionId;
		controller = new AbortController();
		const signal = controller.signal;
		loading = true;
		error = null;
		const attempts = new Map<SessionFlowComponent, number>();
		const work = async () => {
			while (pending.size && token === epoch && !signal.aborted) {
				const component = [...pending].find((item) => (attempts.get(item) ?? 0) < 2);
				if (!component) break;
				pending.delete(component);
				attempts.set(component, (attempts.get(component) ?? 0) + 1);
				try {
					const repeat = await traverse(component, token, sessionId, signal);
					if (repeat && token === epoch) pending.add(component);
				} catch (cause) {
					if (token !== epoch || signal.aborted) break;
					error = cause instanceof Error ? cause : new Error(String(cause));
					failures.set(component, error.message);
					pending.delete(component);
					publish();
				}
			}
		};
		running = work().finally(() => {
			if (token === epoch) {
				loading = false;
				running = null;
				controller = null;
				if (pending.size > 0) {
					if (continuationDelayMs > 0)
						setTimeout(() => {
							if (token === epoch) schedule();
						}, continuationDelayMs);
					else schedule();
				}
			}
		});
		return running;
	}
	function schedule(): void {
		if (scheduled || running || disposed) return;
		scheduled = true;
		const token = epoch;
		queueMicrotask(() => {
			if (token !== epoch) return;
			scheduled = false;
			void start();
		});
	}
	function deactivate(): void {
		epoch++;
		controller?.abort();
		controller = null;
		running = null;
		scheduled = false;
		activeSessionId = null;
		loading = false;
		error = null;
		availability = null;
		pending.clear();
		dirty.clear();
		failures.clear();
		authoritative.clear();
		bounded.clear();
		overlay = [];
		overlayHistoryUnavailable = true;
		publish();
	}
	return {
		get dataset() {
			return dataset;
		},
		get coverage() {
			return dataset.coverage;
		},
		get availability() {
			return availability;
		},
		get activeSessionId() {
			return activeSessionId;
		},
		get loading() {
			return loading;
		},
		get error() {
			return error;
		},
		get stale() {
			return staleReasons.length > 0;
		},
		get staleReasons() {
			return staleReasons;
		},
		get overlay() {
			return overlay;
		},
		get overlayHistoryUnavailable() {
			return overlayHistoryUnavailable;
		},
		load(sessionId) {
			if (disposed) return Promise.resolve();
			deactivate();
			activeSessionId = sessionId;
			publish();
			overlayHistoryUnavailable = false;
			markPending(components, 'Loading flow data');
			return start();
		},
		async loadEarlier(component) {
			if (disposed || activeSessionId === null) return 'inactive';
			if (bounded.has(component)) return 'bounded';
			if (authoritative.get(component)?.coverage.exhausted) return 'exhausted';
			markPending([component], 'Loading recorded flow data');
			await start();
			return bounded.has(component) ? 'bounded' : 'loaded';
		},
		invalidate(reason, scope = components) {
			if (disposed || activeSessionId === null) return;
			markPending(scope, reason);
			schedule();
		},
		retry() {
			if (disposed || activeSessionId === null) return Promise.resolve();
			markPending(failures.size ? [...failures.keys()] : components, 'Retrying flow data');
			return start();
		},
		observeLive(mark) {
			if (
				disposed ||
				(mark.family !== 'StreamingStarted' && mark.family !== 'DiagnosticReady') ||
				mark.sessionId !== activeSessionId ||
				!Number.isFinite(mark.receivedAtMs)
			)
				return;
			if (overlay.some((item) => item.requestId === mark.requestId && item.family === mark.family))
				return;
			const safe: FlowLiveMark = {
				sessionId: mark.sessionId,
				requestId: mark.requestId,
				family: mark.family,
				receivedAtMs: mark.receivedAtMs
			};
			if (mark.statusCode !== undefined && Number.isFinite(mark.statusCode))
				safe.statusCode = mark.statusCode;
			if (mark.ttfbMs !== undefined && Number.isFinite(mark.ttfbMs)) safe.ttfbMs = mark.ttfbMs;
			if (mark.count !== undefined && Number.isFinite(mark.count)) safe.count = mark.count;
			const next = [...overlay, safe];
			if (next.length > limits.overlay) {
				next.shift();
				overlayHistoryUnavailable = true;
			}
			overlay = next;
		},
		clearLiveHistory(_reason) {
			overlay = [];
			overlayHistoryUnavailable = true;
		},
		deactivate,
		dispose() {
			if (disposed) return;
			deactivate();
			disposed = true;
		}
	};
}
