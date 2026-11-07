<script lang="ts">
	import '../app.css';
	import '@fontsource-variable/jetbrains-mono/index.css';
	import '@fontsource-variable/jetbrains-mono/wght-italic.css';
	import '@fontsource-variable/syne/index.css';
	import '@fontsource-variable/inter/index.css';
	import '@fontsource-variable/geist-mono/index.css';
	import 'augmented-ui/augmented-ui.min.css';
	import { onMount, setContext } from 'svelte';
	import { SvelteMap } from 'svelte/reactivity';
	import { fly } from 'svelte/transition';
	import { cubicIn } from 'svelte/easing';
	import { page } from '$app/state';
	import { setupNavigationMotion } from '$lib/navigationMotion';
	import { MAX_EVENTS, proxyEvents } from '$lib/ws.svelte';
	import { getStats } from '$lib/api';
	import type { ProxyEvent, StatsResponse } from '$lib/types';
	import { createSessionListStore } from '$lib/stores/sessions.svelte';
	import { createSessionDetailStore } from '$lib/stores/cache/sessionDetailStore.svelte';
	import { createSessionMailboxStore } from '$lib/stores/mailbox.svelte';
	import { COMMS_STORE_KEY, createCommsStore } from '$lib/comms/commsStore.svelte';
	import { GLOBAL_COMMS_STORE_KEY, createGlobalCommsStore } from '$lib/comms/globalComms.svelte';
	import { configureCacheFromSettings } from '$lib/stores/cache/cacheConfig';
	import { createCollapseStore } from '$lib/stores/collapseState.svelte';
	import { createFilterRulesStore } from '$lib/stores/filterRules.svelte';
	import { createSessionFlowStore } from '$lib/session-flow/sessionFlowStore.svelte';
	import { createSessionFlowLiveAdapter } from '$lib/session-flow/liveAdapter';
	import { shellState } from '$lib/stores/shellState.svelte';
	import {
		COLLAPSE_STORE_KEY,
		FILTER_RULES_STORE_KEY,
		MAILBOX_STORE_KEY,
		SESSION_DETAIL_STORE_KEY,
		SESSION_FLOW_STORE_KEY,
		SESSION_LIST_STORE_KEY,
		STATS_KEY
	} from '$lib/context-keys';
	import { CommandPalette, HelpDrawer } from '$lib/components';
	import EvictionOverlay from '$lib/components/shared/EvictionOverlay.svelte';
	import { ChevronsRight } from '@lucide/svelte';
	import TopBar from '$lib/components/shell/TopBar.svelte';
	import ConnectionBanner from '$lib/components/shell/ConnectionBanner.svelte';
	import DestinationNav from '$lib/components/shell/DestinationNav.svelte';
	import { COLUMN_STUB_WIDTH, setSessionsCollapsed } from '$lib/stores/shellState.svelte';

	let { children } = $props();
	setupNavigationMotion();

	const sessionStore = createSessionListStore();
	const sessionDetailStore = createSessionDetailStore();
	const mailboxStore = createSessionMailboxStore();
	const commsStore = createCommsStore();
	const globalCommsStore = createGlobalCommsStore();
	const collapseStore = createCollapseStore();
	const filterRulesStore = createFilterRulesStore();
	const sessionFlowStore = createSessionFlowStore({ continuationDelayMs: 1000 });
	const sessionFlowLiveAdapter = createSessionFlowLiveAdapter({
		flowStore: sessionFlowStore,
		resolveRequestSessionId(requestId) {
			const requests = sessionDetailStore.detail?.requests;
			return requests?.find((r) => r.id === requestId)?.session_id ?? null;
		}
	});
	setContext(SESSION_LIST_STORE_KEY, sessionStore);
	setContext(SESSION_DETAIL_STORE_KEY, sessionDetailStore);
	setContext(MAILBOX_STORE_KEY, mailboxStore);
	setContext(COMMS_STORE_KEY, commsStore);
	setContext(GLOBAL_COMMS_STORE_KEY, globalCommsStore);
	setContext(COLLAPSE_STORE_KEY, collapseStore);
	setContext(FILTER_RULES_STORE_KEY, filterRulesStore);
	setContext(SESSION_FLOW_STORE_KEY, sessionFlowStore);
	void filterRulesStore.refresh();

	const STATS_POLL_MS = 5_000;

	let stats = $state<StatsResponse | null>(null);
	let pollTimer: ReturnType<typeof setInterval> | null = null;

	async function loadStats(): Promise<void> {
		try {
			stats = await getStats();
		} catch (err) {
			console.warn('[layout] stats fetch failed', err);
		}
	}

	function startPolling(): void {
		if (pollTimer !== null) return;
		void loadStats();
		pollTimer = setInterval(loadStats, STATS_POLL_MS);
	}

	function stopPolling(): void {
		if (pollTimer !== null) {
			clearInterval(pollTimer);
			pollTimer = null;
		}
	}

	function handleVisibility(): void {
		if (document.visibilityState === 'visible') {
			startPolling();
		} else {
			stopPolling();
		}
	}

	let processedEvents = $state(0);
	let processedLagged = $state(0);

	function handleGlobalKeydown(event: KeyboardEvent): void {
		if ((event.metaKey || event.ctrlKey) && !event.altKey && event.key === 'k') {
			event.preventDefault();
			shellState.commandPaletteOpen = !shellState.commandPaletteOpen;
		}
	}

	onMount(() => {
		proxyEvents.connect();
		startPolling();
		void sessionStore.refresh();

		void fetch('/api/settings')
			.then((r) => (r.ok ? r.json() : null))
			.then((data: unknown) => {
				if (!data) return;
				const rows: Array<{ key?: unknown; value?: unknown }> = Array.isArray(data)
					? (data as Array<{ key?: unknown; value?: unknown }>)
					: Array.isArray((data as { settings?: unknown }).settings)
						? (data as { settings: Array<{ key?: unknown; value?: unknown }> }).settings
						: [];
				const byKey = new SvelteMap<string, unknown>();
				for (const row of rows) {
					if (typeof row?.key === 'string') byKey.set(row.key, row.value);
				}
				const partialConfig: Record<string, number | boolean> = {};
				const enabled = byKey.get('inspector_cache.enabled');
				if (typeof enabled === 'boolean') partialConfig.enabled = enabled;
				const memBudget = byKey.get('inspector_cache.memory_budget_bytes');
				if (typeof memBudget === 'number') partialConfig.bodyMemoryByteCap = memBudget;
				const idbBudget = byKey.get('inspector_cache.idb_budget_bytes');
				if (typeof idbBudget === 'number') partialConfig.idbBodyBudgetBytes = idbBudget;
				const apiBudget = byKey.get('inspector_cache.api_response_budget_bytes');
				if (typeof apiBudget === 'number') partialConfig.apiResponseBudgetBytes = apiBudget;
				const bodyCap = byKey.get('inspector_cache.body_entry_cap');
				if (typeof bodyCap === 'number') partialConfig.bodyMemoryEntryCap = bodyCap;
				const detailCap = byKey.get('inspector_cache.detail_entry_cap');
				if (typeof detailCap === 'number') partialConfig.detailMemoryEntryCap = detailCap;
				if (Object.keys(partialConfig).length > 0) {
					configureCacheFromSettings(partialConfig);
				}
			})
			.catch(() => {});

		processedEvents = proxyEvents.totalEvents;
		processedLagged = proxyEvents.laggedCount;

		document.addEventListener('visibilitychange', handleVisibility);
		document.addEventListener('keydown', handleGlobalKeydown);

		return () => {
			document.removeEventListener('visibilitychange', handleVisibility);
			document.removeEventListener('keydown', handleGlobalKeydown);
			stopPolling();
			proxyEvents.disconnect();
			sessionStore.dispose();
			sessionFlowLiveAdapter.dispose();
			sessionFlowStore.dispose();
		};
	});

	$effect(() => {
		sessionFlowLiveAdapter.observeConnectionState(proxyEvents.state);
	});

	$effect(() => {
		if (shellState.paused) return;
		const total = proxyEvents.totalEvents;
		if (total === processedEvents) return;
		const start = total < processedEvents ? 0 : Math.max(processedEvents, total - MAX_EVENTS);
		for (let i = start; i < total; i++) {
			const ev = proxyEvents.at(i);
			if (!ev) continue;
			sessionStore.handleEvent(ev);
			filterRulesStore.handleEvent(ev);
			sessionDetailStore.handleEvent(ev);
			mailboxStore.handleEvent(ev);
			commsStore.handleEvent(ev);
			globalCommsStore.handleEvent(ev);
			sessionFlowLiveAdapter.handleEvent(ev);
		}
		processedEvents = total;
	});

	$effect(() => {
		if (shellState.paused) return;
		const lagged = proxyEvents.laggedCount;
		if (lagged === processedLagged) return;
		const delta = lagged - processedLagged;
		processedLagged = lagged;
		if (delta > 0) {
			const laggedFrame: ProxyEvent = { type: 'Lagged', skipped: delta };
			sessionStore.handleEvent(laggedFrame);
			filterRulesStore.handleEvent(laggedFrame);
			sessionDetailStore.handleEvent(laggedFrame);
			mailboxStore.handleEvent(laggedFrame);
			commsStore.handleEvent(laggedFrame);
			globalCommsStore.handleEvent(laggedFrame);
			sessionFlowLiveAdapter.handleEvent(laggedFrame);
		}
	});

	setContext(STATS_KEY, {
		get value() {
			return stats;
		}
	});

	function handleRefresh(): void {
		void loadStats();
		void sessionStore.refresh();
	}
</script>

<a class="skip-link" href="#main-content">Skip to content</a>
<div class="shell">
	<TopBar {stats} onRefresh={handleRefresh} />
	<ConnectionBanner />

	<div
		class="app"
		style="--sessions-w: {shellState.sessionsCollapsed
			? COLUMN_STUB_WIDTH
			: shellState.sessionsWidth}px;"
	>
		<div class="nav-slot" style="min-width: 0;">
			{#if shellState.sessionsCollapsed}
				<button
					type="button"
					class="col col-stub"
					aria-label="Expand navigation column"
					title="Expand navigation column"
					onclick={() => setSessionsCollapsed(false)}
					out:fly={{ x: -8, duration: 120, easing: cubicIn }}
				>
					<span class="col-stub__peg" aria-hidden="true"></span>
					<span class="col-stub__label">Navigate</span>
					<span class="col-stub__icon">
						<ChevronsRight size={14} aria-hidden="true" />
					</span>
				</button>
			{:else}
				<div class="dn-retract" out:fly={{ x: -24, duration: 180, easing: cubicIn }}>
					<DestinationNav />
				</div>
			{/if}
		</div>

		<main id="main-content" class="route-content" tabindex="-1">
			{@render children()}
		</main>
	</div>
</div>

<HelpDrawer {stats} />

<CommandPalette />

{#if page.url.searchParams.get('debugEvictions') === '1'}
	<EvictionOverlay />
{/if}

<style>
	.route-content {
		display: flex;
		flex-direction: column;
		min-width: 0;
		min-height: 0;
	}
	.skip-link {
		position: fixed;
		top: var(--space-2);
		left: var(--space-2);
		z-index: 100;
		padding: var(--space-3) var(--space-4);
		background: var(--surface-card);
		color: var(--funk-cyan);
		border: var(--hairline) solid var(--funk-cyan-border);
		transform: translateY(calc(-100% - var(--space-4)));
	}
	.skip-link:focus {
		transform: translateY(0);
	}
</style>
