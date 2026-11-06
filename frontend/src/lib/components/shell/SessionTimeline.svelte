<script lang="ts">
	import { tick, untrack, getContext } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { Filter, SearchX } from '@lucide/svelte';
	import type { OperatorTurnDto, RequestSummary, ToolUseDto, ToolUsesResponse } from '$lib/types';
	import { isAtBottom } from '$lib/scroll-utils';
	import {
		resetForRoute,
		setActiveBandId,
		timelineScroll
	} from '$lib/stores/timelineScroll.svelte';
	import type { SessionDetailStore } from '$lib/stores/cache/sessionDetailStore.svelte';
	import { ApiError, getSession, getRequestTransformationManifest } from '$lib/api';
	import {
		loadSessionToolUses,
		invalidateSessionToolUses
	} from '$lib/stores/cache/sessionToolUses.svelte';
	import { loadBody } from '$lib/stores/cache/unifiedCache.svelte';
	import { selectKickoffRequest } from '$lib/grouping';
	import { openDrawer, type DrawerKind } from '$lib/stores/shellState.svelte';
	import { COLLAPSE_STORE_KEY, MAILBOX_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import { createSessionMailboxStore } from '$lib/stores/mailbox.svelte';
	import type { SessionMailboxStore } from '$lib/stores/mailbox.svelte';
	import { getSessionSteering } from '$lib/api/steering';
	import type { SessionSteeringRow } from '$lib/api/steering';
	import { turnsRefetchSignal } from './turnReply';
	import TimelineCommandBar from './TimelineCommandBar.svelte';
	import TurnCard from './TurnCard.svelte';
	import HousekeepingFold from './HousekeepingFold.svelte';
	import IncomingMailboxMessage from './IncomingMailboxMessage.svelte';
	import SteerMarker from './SteerMarker.svelte';
	import SessionEventRow, { isEventPromptKind } from './SessionEventRow.svelte';
	import SystemPromptCard from './SystemPromptCard.svelte';
	import DetailSkeleton from '$lib/components/detail/DetailSkeleton.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import {
		assertNeverTimelineRow,
		buildTimelineRows,
		dispatchCountOf,
		findTurnForRequestId,
		mergeMailboxRows,
		mergeSteerRows,
		shouldAutoLoadOlder,
		timelineBodyState,
		timelineRowKey,
		turnCollapseKey,
		turnVisible,
		type TimelineRow,
		type RoleFilter
	} from './timelineTurnView';

	interface Props {
		detailStore: SessionDetailStore;
		now: number;
		scrollContainer: HTMLElement | undefined;
		onRequestActivate?: (requestId: number) => void;
		requestIdFilter?: ReadonlySet<number> | null;
	}

	let {
		detailStore,
		now,
		scrollContainer,
		onRequestActivate,
		requestIdFilter = null
	}: Props = $props();

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);

	const mailboxContextStore = getContext<SessionMailboxStore | undefined>(MAILBOX_STORE_KEY);
	const mailboxStore: SessionMailboxStore = mailboxContextStore ?? createSessionMailboxStore();

	$effect(() => {
		const sid = detailStore.currentId;
		untrack(() => {
			if (sid) void mailboxStore.load(sid);
		});
	});

	const inboxMessages = $derived(
		mailboxStore.currentId === detailStore.currentId ? (mailboxStore.envelope?.inbox ?? []) : []
	);

	let steeringRows = $state<SessionSteeringRow[]>([]);
	let steeringSessionId: string | null = null;

	$effect(() => {
		const sid = detailStore.currentId;
		untrack(() => {
			if (sid === steeringSessionId) return;
			steeringSessionId = sid;
			steeringRows = [];
			if (sid)
				void getSessionSteering(sid).then((rows) => {
					if (detailStore.currentId === sid) steeringRows = rows;
				});
		});
	});

	const firedSteers = $derived(
		steeringRows.filter((r) => r.verdict === 'fired' && r.payload_text !== null)
	);

	const sessionBasePath = $derived(`/sessions/${encodeURIComponent(detailStore.currentId ?? '')}`);

	let kickoffRequestId = $state<number | null>(null);

	$effect(() => {
		const sid = detailStore.currentId;
		untrack(() => {
			kickoffRequestId = null;
			if (!sid) return;
			getSession(sid, { order: 'asc', offset: 0, limit: 20 })
				.then((data) => {
					if (detailStore.currentId !== sid) return;
					kickoffRequestId = selectKickoffRequest(data.requests);
				})
				.catch((err) => {
					if (detailStore.currentId !== sid) return;
					if (!(err instanceof ApiError)) {
						console.error('kickoff discovery failed', err);
					}
				});
		});
	});

	let kickoffBodyText = $state<string | null>(null);

	$effect(() => {
		const id = kickoffRequestId;
		untrack(() => {
			if (id === null) {
				kickoffBodyText = null;
				return;
			}
			loadBody(id, 'request')
				.then((payload) => {
					if (kickoffRequestId !== id) return;
					kickoffBodyText = payload.text;
				})
				.catch(() => {
					if (kickoffRequestId !== id) return;
					kickoffBodyText = null;
				});
		});
	});

	let kickoffSentBodyText = $state<string | null>(null);
	let kickoffHasManifest = $state(false);

	$effect(() => {
		const id = kickoffRequestId;
		untrack(() => {
			if (id === null) {
				kickoffSentBodyText = null;
				kickoffHasManifest = false;
				return;
			}
			Promise.all([loadBody(id, 'request_sent'), getRequestTransformationManifest(id)])
				.then(([sentPayload, manifestPayload]) => {
					if (kickoffRequestId !== id) return;
					kickoffSentBodyText = sentPayload.text;
					kickoffHasManifest = manifestPayload.has_manifest;
				})
				.catch(() => {
					if (kickoffRequestId !== id) return;
					kickoffSentBodyText = null;
					kickoffHasManifest = false;
				});
		});
	});

	const kickoffSentBlocks = $derived.by((): string[] | null => {
		if (!kickoffHasManifest) return null;
		const parsed = parseJson(kickoffSentBodyText);
		if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return null;
		const sys = (parsed as Record<string, unknown>).system;
		if (!Array.isArray(sys)) return null;
		return sys.map((item) => {
			if (
				item &&
				typeof item === 'object' &&
				typeof (item as Record<string, unknown>).text === 'string'
			) {
				return (item as Record<string, unknown>).text as string;
			}
			return '';
		});
	});

	function parseJson(raw: string | null): unknown {
		if (raw === null) return null;
		try {
			return JSON.parse(raw);
		} catch {
			return null;
		}
	}

	interface SystemBlock {
		text: string;
		index: number;
		raw: unknown;
	}
	const kickoffSystemBlocks = $derived.by((): SystemBlock[] => {
		const parsed = parseJson(kickoffBodyText);
		if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return [];
		const sys = (parsed as Record<string, unknown>).system;
		if (!Array.isArray(sys)) return [];
		const out: SystemBlock[] = [];
		for (let i = 0; i < sys.length; i++) {
			const item = sys[i];
			if (!item || typeof item !== 'object') continue;
			const text = (item as Record<string, unknown>).text;
			if (typeof text === 'string' && text.length > 0) {
				out.push({ text, index: i, raw: item });
			}
		}
		return out;
	});

	const kickoffSystemRaw = $derived.by((): unknown[] | null => {
		const parsed = parseJson(kickoffBodyText);
		if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return null;
		const sys = (parsed as Record<string, unknown>).system;
		return Array.isArray(sys) ? sys : null;
	});

	const bands = $derived.by((): RequestSummary[] => {
		const d = detailStore.detail;
		if (!d) return [];
		return [...d.requests].sort(
			(a, b) => new Date(a.created_at).getTime() - new Date(b.created_at).getTime()
		);
	});

	const headerRequestCount = $derived(detailStore.detail?.total_requests ?? bands.length);

	const bandsById = $derived.by((): Map<number, RequestSummary> => {
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- transient local: built fresh and returned each recompute, never read by anything else while being mutated
		const m = new Map<number, RequestSummary>();
		for (const b of bands) m.set(b.id, b);
		return m;
	});
	function requestNumberFor(id: number): number | null {
		return bandsById.get(id)?.request_number ?? null;
	}
	function statusFor(id: number): string {
		return bandsById.get(id)?.status ?? 'Complete';
	}

	const operatorTurns = $derived(
		[...detailStore.operatorTurns].sort((a, b) => a.start_request_id - b.start_request_id)
	);

	function findTurnForId(id: number): OperatorTurnDto | null {
		return findTurnForRequestId(operatorTurns, id);
	}

	const turnIndexById = $derived.by((): Map<number, number> => {
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- transient local: built fresh and returned each recompute, never read by anything else while being mutated
		const m = new Map<number, number>();
		operatorTurns
			.filter((t) => !isEventPromptKind(t.prompt_kind))
			.forEach((t, i) => m.set(t.turn_id, i + 1));
		return m;
	});
	function turnIndexOf(turn: OperatorTurnDto): number {
		return turnIndexById.get(turn.turn_id) ?? 1;
	}

	const latestTurnId = $derived(
		operatorTurns.filter((t) => !isEventPromptKind(t.prompt_kind)).at(-1)?.turn_id ?? null
	);
	function defaultExpandedFor(turn: OperatorTurnDto): boolean {
		return turn.turn_id === latestTurnId;
	}

	// eslint-disable-next-line svelte/prefer-svelte-reactivity -- deliberately non-reactive: plain instance-scoped bookkeeping mutated only inside untrack() below, never read reactively by the template
	const seededTurnKeys = new Set<number>();
	$effect(() => {
		const turns = operatorTurns;
		const store = collapseStore;
		untrack(() => {
			if (!store) return;
			const latestId =
				turns.filter((t) => !isEventPromptKind(t.prompt_kind)).at(-1)?.turn_id ?? null;
			for (const t of turns) {
				if (seededTurnKeys.has(t.turn_id)) continue;
				seededTurnKeys.add(t.turn_id);
				store.setDefault(turnCollapseKey(t), t.turn_id !== latestId);
			}
		});
	});

	const turnsSignal = $derived(turnsRefetchSignal(detailStore.detail?.requests ?? []));
	const TURNS_REFETCH_DEBOUNCE_MS = 400;
	let turnsRefetchTimer: ReturnType<typeof setTimeout> | null = null;
	let lastTurnsSid: string | null = null;
	let lastTurnsSignal: string | null = null;
	let turnsBaselined = false;

	$effect(() => {
		const sid = detailStore.currentId;
		const signal = turnsSignal;
		const hasDetail = detailStore.detail !== null;
		const stillHydrating = detailStore.loadingOlder;
		untrack(() => {
			if (sid !== lastTurnsSid) {
				lastTurnsSid = sid;
				lastTurnsSignal = null;
				turnsBaselined = false;
				if (turnsRefetchTimer !== null) {
					clearTimeout(turnsRefetchTimer);
					turnsRefetchTimer = null;
				}
				return;
			}
			if (!sid || !hasDetail) return;
			if (stillHydrating) {
				lastTurnsSignal = signal;
				turnsBaselined = false;
				return;
			}
			if (!turnsBaselined) {
				lastTurnsSignal = signal;
				turnsBaselined = true;
				return;
			}
			if (signal === lastTurnsSignal) return;
			lastTurnsSignal = signal;
			if (turnsRefetchTimer !== null) clearTimeout(turnsRefetchTimer);
			turnsRefetchTimer = setTimeout(() => {
				turnsRefetchTimer = null;
				if (detailStore.currentId !== sid) return;
				detailStore.refreshOperatorTurns();
				void loadAllToolUses(sid);
			}, TURNS_REFETCH_DEBOUNCE_MS);
		});
		return () => {
			if (turnsRefetchTimer !== null) {
				clearTimeout(turnsRefetchTimer);
				turnsRefetchTimer = null;
			}
		};
	});

	let sessionToolUses = $state<ToolUseDto[]>([]);
	let toolUsesLoadError = $state<string | null>(null);
	let toolUsesSessionId: string | null = null;
	async function loadAllToolUses(sid: string): Promise<void> {
		try {
			const rows = await loadSessionToolUses(sid);
			if (detailStore.currentId !== sid) return;
			sessionToolUses = rows;
			toolUsesLoadError = null;
		} catch (err) {
			toolUsesLoadError = err instanceof ApiError ? err.message : String(err);
		}
	}

	function retryToolUses(): void {
		const sid = detailStore.currentId;
		if (!sid) return;
		toolUsesLoadError = null;
		invalidateSessionToolUses(sid);
		void loadAllToolUses(sid);
	}

	$effect(() => {
		const sid = detailStore.currentId;
		untrack(() => {
			if (sid === toolUsesSessionId) return;
			toolUsesSessionId = sid;
			sessionToolUses = [];
			toolUsesLoadError = null;
			if (sid) void loadAllToolUses(sid);
		});
	});

	const toolUsesByTurnId = $derived.by((): Map<number, ToolUseDto[]> => {
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- transient local: built fresh and returned each recompute, never read by anything else while being mutated
		const m = new Map<number, ToolUseDto[]>();
		for (const t of sessionToolUses) {
			const turn = findTurnForId(t.request_id);
			if (!turn) continue;
			const arr = m.get(turn.turn_id);
			if (arr) arr.push(t);
			else m.set(turn.turn_id, [t]);
		}
		for (const arr of m.values()) arr.sort((a, b) => a.request_id - b.request_id || a.ord - b.ord);
		return m;
	});

	const toolCountsAll = $derived.by((): Map<string, number> => {
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- transient local: built fresh and returned each recompute, never read by anything else while being mutated
		const m = new Map<string, number>();
		for (const t of sessionToolUses) {
			if (findTurnForId(t.request_id) === null) continue;
			m.set(t.tool_name, (m.get(t.tool_name) ?? 0) + 1);
		}
		return m;
	});
	const toolOptions = $derived(
		[...toolCountsAll.entries()]
			.sort((a, b) => a[0].localeCompare(b[0]))
			.map(([name, count]) => ({ value: name, label: `${name} (${count})` }))
	);

	const timelineBuild = $derived(buildTimelineRows(bands, findTurnForId));
	const mailboxMerge = $derived(mergeMailboxRows(timelineBuild.rows, inboxMessages, findTurnForId, bands));
	const anchorForRequestId = $derived(timelineBuild.anchorForRequestId);
	const steerMerge = $derived(mergeSteerRows(mailboxMerge.rows, firedSteers, anchorForRequestId));
	const timelineRows = $derived(steerMerge.rows);

	function resolveAnchorId(rid: number): number | null {
		const turn = findTurnForId(rid);
		if (turn) return turn.start_request_id;
		return anchorForRequestId.get(rid) ?? null;
	}

	let query = $state('');
	let role = $state<RoleFilter>('all');
	let tool = $state('all');
	const lowerQuery = $derived(query.trim().toLowerCase());
	const filtering = $derived(
		role !== 'all' || tool !== 'all' || lowerQuery.length > 0 || requestIdFilter !== null
	);
	const forcedOpen = $derived(role === 'tools' ? true : role === 'prompts' ? false : null);

	function exitForcingRole(): void {
		if (role === 'prompts' || role === 'tools') role = 'all';
	}

	function onTurnManualToggle(): void {
		exitForcingRole();
	}

	const errorTurnCount = $derived(operatorTurns.filter((t) => t.tool_error_count > 0).length);
	const dispatchTurnCount = $derived(operatorTurns.filter((t) => dispatchCountOf(t) > 0).length);

	function turnMatchesRequestFilter(turn: OperatorTurnDto): boolean {
		if (!requestIdFilter) return true;
		for (const band of bands) {
			if (band.id < turn.start_request_id || band.id > turn.end_request_id) continue;
			if (requestIdFilter.has(band.id)) return true;
		}
		return false;
	}

	const visibleRows = $derived.by((): TimelineRow[] => {
		const out: TimelineRow[] = [];
		for (const row of timelineRows) {
			switch (row.kind) {
				case 'housekeeping':
					if (!filtering && !requestIdFilter) out.push(row);
					break;
				case 'mailbox':
					out.push(row);
					break;
				case 'steer':
					out.push(row);
					break;
				case 'turn': {
					const calls = toolUsesByTurnId.get(row.turn.turn_id) ?? [];
					if (turnVisible(row.turn, calls, role, tool, lowerQuery) && turnMatchesRequestFilter(row.turn))
						out.push(row);
					break;
				}
				default:
					assertNeverTimelineRow(row);
			}
		}
		return out;
	});
	const visibleTurnCount = $derived(visibleRows.filter((r) => r.kind === 'turn').length);
	const resultNote = $derived(
		filtering && visibleTurnCount > 0
			? `${visibleTurnCount} of ${operatorTurns.length} turns match`
			: ''
	);
	const isNoMatch = $derived(filtering && bands.length > 0 && visibleTurnCount === 0);

	function clearFilters(): void {
		query = '';
		role = 'all';
		tool = 'all';
	}

	let hkOpen = $state<Set<number>>(new Set());
	function toggleHk(anchorId: number): void {
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- copy-mutate-reassign pattern: `next` is a throwaway local, never read while being mutated; reactivity comes from replacing `hkOpen` below, not from in-place mutation
		const next = new Set(hkOpen);
		if (next.has(anchorId)) next.delete(anchorId);
		else next.add(anchorId);
		hkOpen = next;
	}

	let flashAnchorId = $state<number | null>(null);
	let flashTimer: ReturnType<typeof setTimeout> | null = null;
	const FLASH_MS = 1300;

	let jumpTargetRequestId = $state<number | null>(null);

	function jumpToAnchor(id: number): void {
		flashAnchorId = id;
		if (flashTimer !== null) clearTimeout(flashTimer);
		flashTimer = setTimeout(() => {
			flashAnchorId = null;
		}, FLASH_MS);
		const scroller = scrollContainer;
		if (!scroller) return;
		const node = scroller.querySelector<HTMLElement>(`[data-band-id="${id}"]`);
		node?.scrollIntoView({ behavior: 'smooth', block: 'start' });
	}

	function onJump(n: number): string | null {
		const turn = operatorTurns.find(
			(t) => n >= t.start_request_number && n <= t.end_request_number
		);
		if (!turn) return `#${n} not in range`;
		const turnLoaded = bands.some(
			(b) => b.id >= turn.start_request_id && b.id <= turn.end_request_id
		);
		if (!turnLoaded)
			return `#${n} isn't loaded yet — scroll up to load earlier requests, then retry (a turn far back may take a few goes)`;
		if (collapseStore) collapseStore.setAll([turnCollapseKey(turn)], false);
		exitForcingRole();
		jumpTargetRequestId = bands.find((b) => b.request_number === n)?.id ?? null;
		void tick().then(() => jumpToAnchor(turn.start_request_id));
		return null;
	}

	function expandAll(): void {
		if (!collapseStore) return;
		collapseStore.setAll(operatorTurns.map(turnCollapseKey), false);
		exitForcingRole();
	}
	function collapseAll(): void {
		if (!collapseStore) return;
		collapseStore.setAll(operatorTurns.map(turnCollapseKey), true);
	}

	function onOpenDrawer(id: number, kind: DrawerKind): void {
		openDrawer(id, kind);
		onRequestActivate?.(id);
	}

	const hasOlderPage = $derived(detailStore.hasOlder);
	const loadingOlder = $derived(detailStore.loadingOlder);
	const loadOlderError = $derived(detailStore.loadOlderError);
	const olderCeilingReached = $derived(detailStore.olderCeilingReached);
	const loadedRequestCount = $derived(detailStore.detail?.requests.length ?? 0);

	const bodyState = $derived(
		timelineBodyState({
			notFound: detailStore.notFound,
			loading: detailStore.loading,
			hasDetail: detailStore.detail !== null,
			bandCount: bands.length
		})
	);

	const VIEWPORT_TOP_OFFSET = 44;
	let scrollRafQueued = false;

	function pickActiveBand(root: HTMLElement): number | null {
		const doc = document.documentElement;
		const atBottom = isAtBottom(window.scrollY, doc.scrollHeight, window.innerHeight, 4);
		const anchors = root.querySelectorAll<HTMLElement>('[data-band-id]');
		if (atBottom && anchors.length > 0) {
			const last = anchors[anchors.length - 1];
			const lastIdStr = last?.dataset.bandId;
			const lastId = lastIdStr ? Number(lastIdStr) : NaN;
			if (Number.isFinite(lastId)) return lastId;
		}

		const offset = VIEWPORT_TOP_OFFSET + cmdBarHeight;
		let bestId: number | null = null;
		let bestTop = -Infinity;
		let firstVisId: number | null = null;
		let firstVisTop = Infinity;
		for (const a of anchors) {
			const idStr = a.dataset.bandId;
			const id = idStr ? Number(idStr) : NaN;
			if (!Number.isFinite(id)) continue;
			const r = a.getBoundingClientRect();
			const top = r.top - offset;
			if (top <= 0) {
				if (top > bestTop) {
					bestTop = top;
					bestId = id;
				}
			} else if (r.bottom > offset && top < firstVisTop) {
				firstVisTop = top;
				firstVisId = id;
			}
		}
		return bestId ?? firstVisId;
	}

	const AUTO_LOAD_OLDER_PX = 80;
	let autoLoadOlderQueued = false;
	let olderStatusEl = $state<HTMLDivElement | undefined>(undefined);

	function maybeAutoLoadOlder(scrollTop: number): void {
		if (!hasOlderPage) return;
		if (olderCeilingReached) return;
		if (loadingOlder) return;
		if (autoLoadOlderQueued) return;
		const shouldLoad = shouldAutoLoadOlder({
			scrollTop,
			topThreshold: AUTO_LOAD_OLDER_PX,
			foldOffset: VIEWPORT_TOP_OFFSET + cmdBarHeight,
			viewportHeight: window.innerHeight,
			sentinelRect: olderStatusEl?.getBoundingClientRect()
		});
		if (!shouldLoad) return;
		autoLoadOlderQueued = true;
		queueMicrotask(() => {
			autoLoadOlderQueued = false;
			void detailStore.loadOlder();
		});
	}

	function handleScroll(): void {
		if (scrollRafQueued) return;
		scrollRafQueued = true;
		requestAnimationFrame(() => {
			scrollRafQueued = false;
			const root = scrollContainer;
			if (!root) return;
			maybeAutoLoadOlder(window.scrollY);
			if (performance.now() < pickActiveLockUntil) return;
			const next = pickActiveBand(root);
			if (next !== null) setActiveBandId(next);
		});
	}

	let pickActiveLockUntil = 0;
	const PICK_ACTIVE_LOCK_MS = 400;

	let scrollDrivenUrlChange = false;
	let urlWritebackTimer: ReturnType<typeof setTimeout> | null = null;
	const URL_WRITEBACK_DEBOUNCE_MS = 200;

	$effect(() => {
		void detailStore.currentId;
		const scroller = scrollContainer;
		untrack(() => {
			resetForRoute();
			if (!scroller) return;
			window.addEventListener('scroll', handleScroll, { passive: true });
			void tick().then(() => {
				const initial = pickActiveBand(scroller);
				if (initial !== null) setActiveBandId(initial);
			});
		});
		return () => {
			if (urlWritebackTimer !== null) {
				clearTimeout(urlWritebackTimer);
				urlWritebackTimer = null;
			}
			scrollDrivenUrlChange = false;
			window.removeEventListener('scroll', handleScroll);
		};
	});

	$effect(() => {
		const sid = detailStore.currentId;
		untrack(() => {
			if (!sid) return;
			void detailStore.loadAll();
		});
	});

	const activeBandId = $derived(timelineScroll.activeBandId);
	const urlRequestId = $derived.by(() => {
		const m = page.url.pathname.match(/^\/sessions\/[^/]+\/requests\/(\d+)/);
		return m && m[1] ? Number(m[1]) : null;
	});
	const queryRequestId = $derived(Number(page.url.searchParams.get('request')) || null);
	const focusRequestId = $derived(urlRequestId ?? queryRequestId);

	const ANCHOR_RETRY_FRAMES = 120;
	let lastAnchoredId = $state<number | null>(null);
	$effect(() => {
		const anchor = focusRequestId;
		const ready = bands.length > 0 && operatorTurns.length > 0;
		untrack(() => {
			const el = scrollContainer;
			if (!ready || !el || anchor === null) return;
			if (lastAnchoredId === anchor) return;
			if (scrollDrivenUrlChange) {
				lastAnchoredId = anchor;
				return;
			}
			if (timelineScroll.activeBandId === anchor) {
				lastAnchoredId = anchor;
				return;
			}
			lastAnchoredId = anchor;
			const turnForAnchor = findTurnForId(anchor);
			if (turnForAnchor && collapseStore) {
				seededTurnKeys.add(turnForAnchor.turn_id);
				collapseStore.setAll([turnCollapseKey(turnForAnchor)], false);
			}
			jumpTargetRequestId = anchor;
			const resolved = resolveAnchorId(anchor) ?? anchor;
			setActiveBandId(resolved);
			flashAnchorId = resolved;
			if (flashTimer !== null) clearTimeout(flashTimer);
			flashTimer = setTimeout(() => {
				flashAnchorId = null;
			}, FLASH_MS);
			pickActiveLockUntil = performance.now() + PICK_ACTIVE_LOCK_MS;
			const land = (framesLeft: number) => {
				const scroller = scrollContainer;
				if (!scroller) return;
				const node = scroller.querySelector<HTMLElement>(`[data-band-id="${resolved}"]`);
				if (!node) {
					if (framesLeft > 0) requestAnimationFrame(() => land(framesLeft - 1));
					return;
				}
				node.scrollIntoView({ behavior: 'auto', block: 'start' });
				setActiveBandId(resolved);
				pickActiveLockUntil = performance.now() + PICK_ACTIVE_LOCK_MS;
			};
			void tick().then(() => land(ANCHOR_RETRY_FRAMES));
		});
	});

	$effect(() => {
		const id = activeBandId;
		const sid = detailStore.currentId;
		const loadedBands = bands;
		const onDeepLinkRoute = urlRequestId !== null;
		untrack(() => {
			if (id === null || !sid) return;
			if (!onDeepLinkRoute) return;
			const m = page.url.pathname.match(/^\/sessions\/[^/]+\/requests\/(\d+)/);
			const currentRid = m && m[1] ? Number(m[1]) : null;
			if (currentRid === id) return;
			if (currentRid !== null && !loadedBands.some((b) => b.id === currentRid)) {
				return;
			}
			if (urlWritebackTimer !== null) clearTimeout(urlWritebackTimer);
			urlWritebackTimer = setTimeout(() => {
				urlWritebackTimer = null;
				lastAnchoredId = id;
				scrollDrivenUrlChange = true;
				void goto(
					resolve(
						`/sessions/${encodeURIComponent(sid)}/requests/${id}` as `/sessions/${string}/requests/${string}`
					),
					{
						replaceState: true,
						keepFocus: true,
						noScroll: true
					}
				).finally(() => {
					requestAnimationFrame(() => {
						scrollDrivenUrlChange = false;
					});
				});
			}, URL_WRITEBACK_DEBOUNCE_MS);
		});
	});

	let cmdBarHeight = $state(0);
</script>

<div
	class="session-timeline"
	data-slot="session-timeline"
	style="--timeline-cmdbar-h: {cmdBarHeight}px"
>
	<TimelineCommandBar
		bind:query
		bind:role
		bind:tool
		{toolOptions}
		{errorTurnCount}
		{dispatchTurnCount}
		requestCount={headerRequestCount}
		turnCount={turnIndexById.size}
		isLive={bands.some((b) => b.status === 'Pending' || b.status === 'Streaming')}
		{onJump}
		onExpandAll={expandAll}
		onCollapseAll={collapseAll}
		onHeightChange={(h) => (cmdBarHeight = h)}
		operatorOnly={detailStore.promptKindFilter === 'operator'}
		onToggleOperatorOnly={() =>
			detailStore.setPromptKindFilter(
				detailStore.promptKindFilter === 'operator' ? null : 'operator'
			)}
	/>

	{#if kickoffSystemBlocks.length > 0}
		<SystemPromptCard
			blocks={kickoffSystemBlocks}
			systemRaw={kickoffSystemRaw}
			sessionId={detailStore.currentId ?? ''}
			requestId={kickoffRequestId}
			sentBlocks={kickoffSentBlocks}
		/>
	{/if}

	<div class="session-timeline__scroll">
		{#if toolUsesLoadError !== null}
			<LoadErrorBanner
				title="Couldn't load the tool-call ledger"
				detail={toolUsesLoadError}
				hint="Turn tool calls show as unavailable until the ledger loads. This doesn't affect the rest of the timeline."
				onRetry={retryToolUses}
			/>
		{/if}

		{#if bodyState === 'not-found'}
			<p class="session-timeline__hint" role="status">Session not found — no timeline to show.</p>
		{:else if bodyState === 'skeleton'}
			<DetailSkeleton />
		{:else if bodyState === 'empty'}
			<p class="session-timeline__hint">No requests in this session yet.</p>
		{:else}
			{#if hasOlderPage}
				<div class="session-timeline__older-status" bind:this={olderStatusEl} aria-live="polite">
					{#if loadingOlder}
						<span class="session-timeline__older-status__txt">Loading earlier requests…</span>
					{:else if olderCeilingReached}
						<span class="session-timeline__older-status__txt"
							>The most recent {loadedRequestCount.toLocaleString('en-US')} requests are loaded. Earlier
							ones are not shown here.</span
						>
					{:else}
						<span class="session-timeline__older-status__txt">Scroll up for earlier requests</span>
					{/if}
				</div>
			{/if}
			{#if loadOlderError}
				<span class="session-timeline__sentinel-error" role="alert">
					{loadOlderError}
				</span>
			{/if}

			{#if isNoMatch}
				<div class="session-timeline__empty-filter">
					<SearchX size={28} class="text-[var(--text-faint)]" aria-hidden="true" />
					<p>No turns match the current filter.</p>
					<span class="session-timeline__clear-btn-backing">
						<button
							type="button"
							class="session-timeline__clear-btn"
							data-motion="hover"
							data-augmented-ui="tr-clip bl-clip border"
							onclick={clearFilters}
						>
							Clear filters
						</button>
					</span>
				</div>
			{:else}
				{#if resultNote}
					<div class="session-timeline__result-note">
						<Filter size={12} class="text-[var(--interactive-active-fg)]" aria-hidden="true" />
						<span>{resultNote}</span>
						<button type="button" onclick={clearFilters}>clear</button>
					</div>
				{/if}
				{#each visibleRows as row (timelineRowKey(row))}
					{#if row.kind === 'turn' && isEventPromptKind(row.turn.prompt_kind)}
						<SessionEventRow
							turn={row.turn}
							requestRange={{ first: row.firstRequestNumber, last: row.lastRequestNumber }}
							query={lowerQuery}
						/>
					{:else if row.kind === 'turn'}
						<TurnCard
							turn={row.turn}
							index={turnIndexOf(row.turn)}
							isFirst={row.turn.start_request_number === 1}
							toolCalls={toolUsesByTurnId.get(row.turn.turn_id) ?? []}
							ledgerUnavailable={toolUsesLoadError !== null}
							live={row.live}
							{now}
							defaultExpanded={defaultExpandedFor(row.turn)}
							query={lowerQuery}
							toolFilter={tool}
							{forcedOpen}
							flash={flashAnchorId === row.turn.start_request_id}
							{jumpTargetRequestId}
							{requestNumberFor}
							requestRange={{ first: row.firstRequestNumber, last: row.lastRequestNumber }}
							{statusFor}
							onManualToggle={onTurnManualToggle}
							{onOpenDrawer}
							onJumpSelf={() => jumpToAnchor(row.turn.start_request_id)}
						/>
					{:else if row.kind === 'housekeeping'}
						<HousekeepingFold
							anchorId={row.anchorId}
							items={row.items}
							open={hkOpen.has(row.anchorId)}
							onToggle={() => toggleHk(row.anchorId)}
							onOpenRequest={(id) => onOpenDrawer(id, 'request')}
						/>
					{:else if row.kind === 'mailbox'}
						<IncomingMailboxMessage
							message={row.message}
							variant={row.variant}
							sessionId={detailStore.currentId ?? ''}
							{now}
							turnIndex={row.turn ? turnIndexOf(row.turn) : null}
							query={lowerQuery}
						/>
					{:else if row.kind === 'steer'}
						<SteerMarker intervention={row.intervention} {sessionBasePath} {now} />
					{:else}
						{@const _never = assertNeverTimelineRow(row)}
					{/if}
				{/each}
			{/if}
		{/if}
	</div>
</div>

<style>
	.session-timeline {
		display: flex;
		flex-direction: column;
		flex: 0 0 auto;
		min-height: unset;
	}

	.session-timeline > :global(.spc-card) {
		margin: var(--space-4, 16px) 0;
	}

	.session-timeline__scroll {
		flex: 0 0 auto;
		min-height: unset;
		display: flex;
		flex-direction: column;
		gap: 0;
		background: var(--surface-card);
	}

	.session-timeline__hint {
		font-size: 12px;
		color: var(--text-3);
		font-style: italic;
		padding: 16px;
		text-align: center;
	}

	.session-timeline__older-status {
		display: flex;
		justify-content: center;
		padding: 8px 16px;
	}
	.session-timeline__older-status__txt {
		color: var(--text-3);
		font-family: var(--font-mono);
		font-size: 11px;
		letter-spacing: 0.04em;
		text-transform: uppercase;
	}
	.session-timeline__sentinel-error {
		color: var(--status-danger-fg);
		font-size: 11px;
		font-family: var(--font-mono);
		text-align: center;
		max-width: 80%;
		overflow-wrap: anywhere;
		word-break: break-word;
	}

	.session-timeline__result-note,
	.session-timeline__empty-filter {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: var(--text-caption);
		color: var(--text-3);
		padding: 4px 2px 8px;
	}
	.session-timeline__result-note button {
		background: none;
		border: none;
		color: var(--interactive-active-fg);
		cursor: pointer;
		font-family: inherit;
		font-size: inherit;
		text-decoration: underline;
	}
	.session-timeline__empty-filter {
		flex-direction: column;
		justify-content: center;
		height: 60vh;
		gap: 10px;
	}
	.session-timeline__clear-btn-backing {
		display: inline-flex;
		background: var(--surface-bg);
	}
	.session-timeline__clear-btn {
		--edge: 6px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		padding: calc(var(--edge) + 2px) calc(var(--edge) + 6px);
		background: transparent;
		color: var(--text-1);
		font-family: inherit;
		font-size: inherit;
		cursor: pointer;
		text-decoration: none;
	}
	.session-timeline__clear-btn:hover {
		color: var(--text-strong);
		--aug-border-bg: var(--interactive-active-border);
	}
	.session-timeline__clear-btn:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
</style>
