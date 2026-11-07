<script lang="ts">
	import { onMount, onDestroy, getContext, tick } from 'svelte';
	import { slide } from 'svelte/transition';
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import { MAX_EVENTS, proxyEvents } from '$lib/ws.svelte';
	import { createDiagnosticsStore } from '$lib/stores/diagnostics.svelte';
	import {
		createFindingsStore,
		groupFindings,
		type CategoryGroup,
		type SessionGroup,
		type RequestGroup
	} from '$lib/stores/findings.svelte';
	import { getStats, ApiError } from '$lib/api';
	import { onApiCacheUpdate } from '$lib/stores/cache/apiCache.svelte';
	import { pluralRequests } from '$lib/i18n';
	import { shortSessionId } from '$lib/format';
	import { formatRelative } from '$lib/time';
	import {
		CATEGORY_META,
		ALL_CATEGORIES,
		severityOf,
		checkLabel,
		type Severity
	} from '$lib/diagnostics';
	import type { DiagnosticCategory, Finding, StatsResponse } from '$lib/types';
	import { cn } from '$lib/utils';
	import { StatusPill } from '$lib/components';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import EmptyStateOnboarding from '$lib/components/shared/EmptyStateOnboarding.svelte';
	import HelpHint from '$lib/components/shared/HelpHint.svelte';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import { shellState } from '$lib/stores/shellState.svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import { ArrowRight, X as XIcon } from '@lucide/svelte';

	function openHelp(): void {
		shellState.helpDrawerOpen = true;
	}

	const statsStore = createDiagnosticsStore();
	const findingsStore = createFindingsStore();
	const POLL_MS = 10_000;

	let proxyStats = $state<StatsResponse | null>(null);
	let statsPollTimer: ReturnType<typeof setInterval> | null = null;
	let processedEvents = $state(0);
	let processedLagged = $state(0);
	let now = $state(Date.now());

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);
	let localExpanded = $state<Set<string>>(new Set());

	function isOpen(key: string): boolean {
		return collapseStore ? collapseStore.isExpanded(key) : localExpanded.has(key);
	}

	function toggle(key: string): void {
		if (collapseStore) {
			collapseStore.toggle(key);
			return;
		}
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- cloned, mutated, then reassigned wholesale; reactivity comes from the reassignment below, not in-place mutation
		const next = new Set(localExpanded);
		if (next.has(key)) {
			next.delete(key);
		} else {
			next.add(key);
		}
		localExpanded = next;
	}

	async function toggleAndScroll(key: string, event: MouseEvent | KeyboardEvent): Promise<void> {
		const opening = !isOpen(key);
		const target = event.currentTarget as HTMLElement;
		toggle(key);
		if (opening) {
			await tick();
			target.closest('li')?.scrollIntoView({ behavior: 'instant', block: 'start' });
		}
	}

	function handleRowKey(event: KeyboardEvent, key: string): void {
		if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			toggle(key);
		}
	}

	function catKey(category: DiagnosticCategory): string {
		return `analysis:cat:${category}`;
	}
	function sessKey(category: DiagnosticCategory, sessionId: string): string {
		return `analysis:sess:${category}:${sessionId}`;
	}
	function reqKey(category: DiagnosticCategory, sessionId: string, requestId: number): string {
		return `analysis:req:${category}:${sessionId}:${requestId}`;
	}

	async function loadStats(): Promise<void> {
		try {
			proxyStats = await getStats();
		} catch (err) {
			if (!(err instanceof ApiError)) {
				console.warn('[analysis] failed to fetch /api/stats', err);
			}
		}
	}

	onMount(() => {
		proxyEvents.connect();
		void statsStore.refresh();
		statsStore.startPolling(POLL_MS);
		void findingsStore.refresh();
		findingsStore.startPolling(POLL_MS);
		void loadStats();
		statsPollTimer = setInterval(loadStats, POLL_MS);
		const unsub = onApiCacheUpdate(
			'/api/stats',
			() => {
				void loadStats();
			},
			{ prefix: true }
		);
		processedEvents = proxyEvents.totalEvents;
		processedLagged = proxyEvents.laggedCount;
		const clockTimer = setInterval(() => (now = Date.now()), 1_000);
		return () => {
			unsub();
			clearInterval(clockTimer);
		};
	});

	onDestroy(() => {
		statsStore.stopPolling();
		findingsStore.dispose();
		if (statsPollTimer !== null) {
			clearInterval(statsPollTimer);
			statsPollTimer = null;
		}
	});

	$effect(() => {
		const total = proxyEvents.totalEvents;
		if (total === processedEvents) return;
		const start = total < processedEvents ? 0 : Math.max(processedEvents, total - MAX_EVENTS);
		for (let i = start; i < total; i++) {
			const ev = proxyEvents.at(i);
			if (ev) {
				statsStore.handleEvent(ev);
				findingsStore.handleEvent(ev);
			}
		}
		processedEvents = total;
	});

	$effect(() => {
		const lagged = proxyEvents.laggedCount;
		if (lagged === processedLagged) return;
		processedLagged = lagged;
		void statsStore.refresh();
	});

	const stats = $derived(statsStore.stats);
	const statsLoading = $derived(statsStore.loading);
	const statsErr = $derived(statsStore.error);
	const findingsList = $derived(findingsStore.findings);
	const findingsTotal = $derived(findingsStore.total);
	const findingsErr = $derived(findingsStore.error);
	const findingsHasMore = $derived(findingsStore.hasMore);
	const findingsLoadingMore = $derived(findingsStore.loadingMore);
	const findingsLoadMoreError = $derived(findingsStore.loadMoreError);

	function handleLoadOlderFindings(): void {
		void findingsStore.loadOlder();
	}

	const categoryTotals = $derived(stats?.totals_by_category ?? {});
	const analyzerTotals = $derived(
		Object.entries(stats?.totals_by_analyzer ?? {}) as Array<[string, number]>
	);
	const analyzerFilter = $derived(findingsStore.analyzerFilter);

	function selectAnalyzer(name: string): void {
		const next = analyzerFilter === name ? null : name;
		findingsStore.setAnalyzerFilter(next);
	}

	const last24h = $derived(stats?.last_24h ?? 0);
	const totalFindings = $derived(
		Object.values(categoryTotals).reduce((sum, v) => sum + (v ?? 0), 0) as number
	);
	const activeCategories = $derived(
		Object.values(categoryTotals).filter((v) => (v ?? 0) > 0).length
	);

	const tree = $derived<CategoryGroup[]>(findingsList === null ? [] : groupFindings(findingsList));

	const sessionParam = $derived(page.url.searchParams.get('session'));
	const categoryParam = $derived(
		page.url.searchParams.get('category') as DiagnosticCategory | null
	);

	$effect(() => {
		findingsStore.setSessionFilter(sessionParam);
	});

	const displayTree = $derived.by((): CategoryGroup[] => {
		let base = tree;
		if (sessionParam !== null) {
			const out: CategoryGroup[] = [];
			for (const cat of base) {
				const sessions = cat.sessions.filter((s) => s.session_id === sessionParam);
				if (sessions.length === 0) continue;
				const count = sessions.reduce((n, s) => n + s.count, 0);
				out.push({ ...cat, sessions, count });
			}
			base = out;
		}
		if (categoryParam !== null) {
			return base.filter((cat) => cat.category === categoryParam);
		}
		return base;
	});

	let seededFor = $state<string | null>(null);
	$effect(() => {
		const sp = sessionParam;
		const cp = categoryParam;
		const dt = displayTree;
		const seedKey = [sp ?? '', cp ?? ''].join(':');
		if (sp === null && cp === null) {
			seededFor = null;
			return;
		}
		if (seededFor === seedKey) return;
		if (dt.length === 0) return;
		const ensureOpen = (k: string): void => {
			if (collapseStore) {
				if (!collapseStore.isExpanded(k)) collapseStore.toggle(k);
			} else if (!localExpanded.has(k)) {
				// eslint-disable-next-line svelte/prefer-svelte-reactivity -- cloned, mutated, then reassigned wholesale; reactivity comes from the reassignment, not in-place mutation
				localExpanded = new Set(localExpanded).add(k);
			}
		};
		for (const cat of dt) {
			ensureOpen(catKey(cat.category));
			if (sp !== null) {
				ensureOpen(sessKey(cat.category, sp));
			}
		}
		seededFor = seedKey;
	});
	const loadedFindingsCount = $derived(findingsList?.length ?? 0);
	const showingLabel = $derived.by((): string | null => {
		if (findingsTotal === null) return null;
		return `Showing ${loadedFindingsCount} of ${findingsTotal}`;
	});

	const maxCategoryCount = $derived(
		ALL_CATEGORIES.reduce((m, c) => {
			const v = categoryTotals[c] ?? 0;
			return v > m ? v : m;
		}, 0)
	);

	const showingSkeleton = $derived(
		stats === null && findingsList === null && (statsLoading || findingsStore.loading)
	);
	const showingError = $derived(!!statsErr || !!findingsErr);
	const showingEmpty = $derived(
		stats !== null && findingsList !== null && !showingError && totalFindings === 0
	);
	const showingData = $derived(stats !== null && findingsList !== null && totalFindings > 0);

	const hasObservedTraffic = $derived(proxyStats !== null && proxyStats.request_count > 0);
	const scannedLabel = $derived(
		proxyStats !== null ? pluralRequests(proxyStats.request_count) : ''
	);

	const errorMessage = $derived(statsErr ?? findingsErr ?? '');

	const SEVERITY_BAR_CLASS: Record<Severity, string> = {
		error: 'bg-[var(--status-danger-fg)]',
		warning: 'bg-[var(--status-warning-fg)]',
		muted: 'bg-[var(--text-3)]'
	};
	function barClassFor(category: DiagnosticCategory): string {
		return SEVERITY_BAR_CLASS[severityOf(category)];
	}
	function barWidthPercent(count: number, max: number): number {
		return max <= 0 ? 0 : Math.round((count / max) * 100);
	}

	function handleRetry(): void {
		void statsStore.refresh();
		void findingsStore.refresh();
	}

	function formatAgo(ts: number | null, wallNow: number): string {
		if (ts === null) return 'never';
		const diffSeconds = Math.max(0, Math.floor((wallNow - ts) / 1_000));
		if (diffSeconds < 1) return 'just now';
		if (diffSeconds < 60) return `${diffSeconds}s ago`;
		const minutes = Math.floor(diffSeconds / 60);
		if (minutes < 60) return `${minutes}m ago`;
		return `${Math.floor(minutes / 60)}h ago`;
	}
	const updatedLabel = $derived(formatAgo(statsStore.lastFetchedAt, now));

	function requestHref(
		session: SessionGroup,
		req: RequestGroup
	): `sessions/${string}/requests/${string}` {
		return `sessions/${encodeURIComponent(session.session_id)}/requests/${req.request_id}?drawer=findings`;
	}
	function sessionHref(session: SessionGroup): `sessions/${string}` {
		return `sessions/${encodeURIComponent(session.session_id)}`;
	}

	function coerceStatus(status: string): 'Pending' | 'Streaming' | 'Complete' | { Error: string } {
		if (status === 'Pending' || status === 'Streaming' || status === 'Complete') {
			return status;
		}
		if (status.startsWith('Error: ')) {
			return { Error: status.slice('Error: '.length) };
		}
		return { Error: status };
	}

	function matchSpanLabel(range: Finding['match_range']): string {
		const [start, end] = range;
		const width = Math.max(0, end - start);
		return `${start}–${end} (${width} bytes)`;
	}
</script>

<svelte:head>
	<title>Flagged responses — mike</title>
</svelte:head>

<section class="analysis-page flex flex-col gap-6" data-slot="analysis-page">
	<div class="flex items-baseline justify-between gap-4">
		<PageIntro title="Flagged responses">
			<span class="flex items-center gap-1.5">
				<span>Flagged</span>
				<span class="inline-flex items-center gap-1">
					<span>findings</span>
					<HelpHint slug="finding" />
				</span>
				<span>across captured request bodies.</span>
			</span>
		</PageIntro>
		<p class="text-xs text-[var(--text-3)]">Last updated: {updatedLabel}</p>
	</div>

	{#if showingError}
		<LoadErrorBanner
			title="Couldn't load diagnostics"
			detail={errorMessage}
			hint="Check the inspect API at :9989. Diagnostics depend on the flagging checks — if capture is running but findings are unavailable, check the database connection."
			onRetry={handleRetry}
		/>
	{:else if showingSkeleton}
		<div class="grid grid-cols-1 gap-4 sm:grid-cols-3">
			{#each [0, 1, 2] as i (i)}
				<div class="an-kpi-card an-skeleton" data-augmented-ui="tl-clip br-clip border">
					<div class="an-skeleton-bar an-skeleton-bar--label"></div>
					<div class="an-skeleton-bar an-skeleton-bar--value"></div>
				</div>
			{/each}
		</div>
		<div class="an-panel an-skeleton" data-augmented-ui="tl-clip br-clip border">
			<div class="an-skeleton-bar an-skeleton-bar--title"></div>
			<div class="mt-4 flex flex-col gap-3">
				{#each [0, 1, 2, 3, 4] as i (i)}
					<div class="an-skeleton-bar an-skeleton-bar--row"></div>
				{/each}
			</div>
		</div>
	{:else if showingEmpty}
		<div
			class="an-panel text-center text-sm text-[var(--text-2)]"
			data-augmented-ui="tl-clip br-clip border"
		>
			{#if hasObservedTraffic}
				<p class="font-medium text-[var(--text-strong)]">
					Scanned {scannedLabel}, no diagnostics found.
				</p>
				<p class="mt-2 text-xs text-[var(--text-3)]">
					Every captured response was checked and nothing matched. New findings will appear here as
					they land.
				</p>
			{:else}
				<EmptyStateOnboarding
					headline="No diagnostics recorded yet"
					body="Route a Claude Code session through ANTHROPIC_BASE_URL to start collecting findings."
					confirmation="Findings will appear here as each captured response is checked."
					onOpenHelp={openHelp}
				/>
			{/if}
		</div>
	{:else if showingData}
		{@const summaryCards = [
			['Total findings', totalFindings, 'all time'],
			['Last 24 hours', last24h, 'findings'],
			['Categories active', activeCategories, `of ${ALL_CATEGORIES.length} total`]
		] as const}
		<div class="grid grid-cols-1 gap-4 sm:grid-cols-3">
			{#each summaryCards as [label, value, hint] (label)}
				<div class="an-kpi-card" data-augmented-ui="tl-clip br-clip border">
					<p class="text-label uppercase tracking-wider text-[var(--text-3)]">
						{label}
					</p>
					<p class="mt-2 font-mono text-3xl text-[var(--text-strong)]">
						{value}
					</p>
					<p class="mt-1 text-label text-[var(--text-3)]">{hint}</p>
				</div>
			{/each}
		</div>

		<section
			class="an-panel"
			data-augmented-ui="tl-clip br-clip border"
			aria-label="Findings by check and category"
			data-slot="findings-panel"
		>
			{#if analyzerTotals.length > 0}
				<div data-slot="checks-band">
					<span class="an-heading-backing">
						<h3 class="an-heading" data-augmented-ui="tr-clip bl-clip border">By check</h3>
					</span>
					<table class="mt-3 w-full" data-slot="analyzer-table">
						<tbody>
							{#each analyzerTotals as [name, count] (name)}
								<tr
									role="button"
									tabindex="0"
									class="cursor-pointer text-sm hover:bg-[var(--surface-2)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--interactive-focus-ring)]"
									aria-label="Filter by {checkLabel(name)}, {count} {count === 1
										? 'finding'
										: 'findings'}"
									aria-pressed={analyzerFilter === name}
									onclick={() => selectAnalyzer(name)}
									onkeydown={(e) => {
										if (e.key === 'Enter' || e.key === ' ') {
											e.preventDefault();
											selectAnalyzer(name);
										}
									}}
								>
									<td class="py-1 pr-4 font-mono text-xs text-[var(--text-1)]"
										>{checkLabel(name)}</td
									>
									<td class="py-1 text-right font-mono text-xs text-[var(--text-2)]">{count}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
				<div class="my-5 border-t border-[var(--border-subtle)]" aria-hidden="true"></div>
			{/if}

			<div data-slot="findings-tree">
				<div class="flex flex-wrap items-center justify-between gap-3">
					<div class="flex items-baseline gap-2">
						<span class="an-heading-backing">
							<h3 class="an-heading" data-augmented-ui="tr-clip bl-clip border">
								Findings by category
							</h3>
						</span>
						{#if showingLabel !== null}
							<span class="text-label text-[var(--text-3)]" data-slot="findings-showing">
								{showingLabel}
							</span>
						{/if}
					</div>
					<div class="flex items-center gap-2">
						{#if analyzerFilter !== null}
							<span class="an-chip-backing">
								<span
									class="an-chip an-chip--warning"
									data-augmented-ui="tl-clip br-clip border"
									data-slot="analyzer-filter-chip"
								>
									<span class="font-mono">filter: check={checkLabel(analyzerFilter)}</span>
									<button
										type="button"
										class="an-chip-clear outline-none"
										aria-label="Clear check filter"
										onclick={() => findingsStore.setAnalyzerFilter(null)}
									>
										<XIcon class="size-3" aria-hidden="true" />
									</button>
								</span>
							</span>
						{/if}
						{#if categoryParam !== null}
							<span class="an-chip-backing">
								<span
									class="an-chip an-chip--warning"
									data-augmented-ui="tl-clip br-clip border"
									data-slot="category-filter-chip"
								>
									<span>category: {categoryParam.replace(/_/g, ' ')}</span>
									<a
										href={resolve(
											sessionParam !== null
												? `/analysis?session=${encodeURIComponent(sessionParam)}`
												: '/analysis'
										)}
										class="an-chip-clear outline-none"
										aria-label="Clear category filter"
									>
										<XIcon class="size-3" aria-hidden="true" />
									</a>
								</span>
							</span>
						{/if}
						{#if sessionParam !== null}
							<span class="an-chip-backing">
								<span
									class="an-chip an-chip--info"
									data-augmented-ui="tl-clip br-clip border"
									data-slot="session-filter-chip"
								>
									<span class="font-mono">session: {shortSessionId(sessionParam)}</span>
									<a
										href={resolve(
											categoryParam !== null
												? `/analysis?category=${encodeURIComponent(categoryParam)}`
												: '/analysis'
										)}
										class="an-chip-clear outline-none"
										aria-label="Clear session filter"
									>
										<XIcon class="size-3" aria-hidden="true" />
									</a>
								</span>
							</span>
						{/if}
					</div>
				</div>

				{#if displayTree.length === 0}
					<p class="mt-4 text-sm text-[var(--text-3)]">
						{#if sessionParam !== null}
							No findings for session {shortSessionId(sessionParam)}.
						{:else}
							No findings match the current filter.
						{/if}
					</p>
				{:else}
					<ul class="mt-4 flex flex-col gap-3">
						{#each displayTree as cat (cat.category)}
							{@const ck = catKey(cat.category)}
							{@const catExpanded = isOpen(ck)}
							{@const meta = CATEGORY_META[cat.category]}
							{@const catControlsId = `cat-panel-${cat.category}`}
							<li
								class="border border-[var(--border-subtle)] bg-[var(--surface-bg)]"
								data-slot="category-row"
							>
								<div
									role="button"
									tabindex="0"
									aria-expanded={catExpanded}
									aria-controls={catExpanded ? catControlsId : undefined}
									aria-label={`${meta.label} — ${cat.count} ${cat.count === 1 ? 'finding' : 'findings'}, ${catExpanded ? 'collapse' : 'expand'}`}
									class="flex cursor-pointer flex-col gap-1 px-3 py-2 outline-none transition-colors hover:bg-[var(--surface-2)] focus-visible:ring-1 focus-visible:ring-[var(--interactive-focus-ring)]"
									onclick={(e) => toggleAndScroll(ck, e)}
									onkeydown={(e) => {
										if (e.key === 'Enter' || e.key === ' ') {
											e.preventDefault();
											void toggleAndScroll(ck, e);
										}
									}}
								>
									<div class="flex items-center justify-between gap-3">
										<div class="flex items-center gap-2">
											<DisclosureMarker open={catExpanded} size={12} />
											<span class={cn(meta.badgeClass, 'an-badge')}>{meta.label}</span>
										</div>
										<span class="font-mono text-xs text-[var(--text-1)]">{cat.count}</span>
									</div>
									<div class="h-2 w-full overflow-hidden bg-[var(--surface-2)]" role="presentation">
										<div
											class={cn('h-full', barClassFor(cat.category))}
											style="width: {barWidthPercent(cat.count, maxCategoryCount)}%"
										></div>
									</div>
								</div>

								{#if catExpanded}
									<ul
										id={catControlsId}
										class="flex flex-col gap-2 border-t border-[var(--border-subtle)] px-3 py-3"
										transition:slide={{ duration: 240 }}
									>
										{#each cat.sessions as session (session.session_id)}
											{@const sk = sessKey(cat.category, session.session_id)}
											{@const sessExpanded = isOpen(sk)}
											{@const sessPanelId = `sess-panel-${cat.category}-${session.session_id}`}
											<li
												class="border border-[var(--border-subtle)] bg-[var(--surface-card)]"
												data-slot="session-row"
											>
												<div class="flex items-stretch">
													<div
														role="button"
														tabindex="0"
														aria-expanded={sessExpanded}
														aria-controls={sessExpanded ? sessPanelId : undefined}
														aria-label={`Session ${shortSessionId(session.session_id)} — ${session.count} ${session.count === 1 ? 'finding' : 'findings'}, ${sessExpanded ? 'collapse' : 'expand'}`}
														class="flex min-w-0 flex-1 cursor-pointer items-center gap-3 px-3 py-2 outline-none transition-colors hover:bg-[var(--surface-2)] focus-visible:ring-1 focus-visible:ring-[var(--interactive-focus-ring)]"
														onclick={() => toggle(sk)}
														onkeydown={(e) => handleRowKey(e, sk)}
													>
														<DisclosureMarker open={sessExpanded} size={12} />
														<span
															class="font-mono text-xs text-[var(--text-strong)]"
															title={session.session_id}>{shortSessionId(session.session_id)}</span
														>
														{#if session.project}
															<a
																href={resolve(`projects/${encodeURIComponent(session.project)}`)}
																class="text-label text-[var(--text-2)] whitespace-pre-wrap break-words underline decoration-[var(--border-default)] underline-offset-2 hover:text-[var(--text-strong)] hover:decoration-[var(--text-strong)] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--interactive-focus-ring)]"
																onclick={(e) => e.stopPropagation()}>{session.project}</a
															>
														{/if}
														{#if session.model}
															<span
																class="font-mono text-label text-[var(--text-3)] whitespace-pre-wrap break-words"
																>{session.model}</span
															>
														{/if}
														<span class="ml-auto font-mono text-xs text-[var(--text-1)]"
															>{session.count}</span
														>
														<span class="text-label text-[var(--text-3)]"
															>{formatRelative(session.newest_created_at, now)}</span
														>
													</div>
													<a
														href={resolve(sessionHref(session))}
														class="flex items-center justify-center border-l border-[var(--border-subtle)] px-3 outline-none transition-colors hover:bg-[var(--status-info-bg)] hover:text-[var(--status-info-fg)] focus-visible:ring-1 focus-visible:ring-[var(--interactive-focus-ring)]"
														aria-label={`Open session ${shortSessionId(session.session_id)}`}
														onclick={(e) => e.stopPropagation()}
													>
														<ArrowRight class="size-3.5 text-[var(--text-3)]" aria-hidden="true" />
													</a>
												</div>

												{#if sessExpanded}
													<ul
														id={sessPanelId}
														class="flex flex-col gap-2 border-t border-[var(--border-subtle)] px-3 py-3"
														transition:slide={{ duration: 240 }}
													>
														{#each session.requests as req (req.request_id)}
															{@const rk = reqKey(cat.category, session.session_id, req.request_id)}
															{@const reqExpanded = isOpen(rk)}
															{@const reqPanelId = `req-panel-${cat.category}-${session.session_id}-${req.request_id}`}
															<li
																class="border border-[var(--border-subtle)] bg-[var(--surface-bg)]"
																data-slot="request-row"
															>
																<div class="flex items-stretch">
																	<div
																		role="button"
																		tabindex="0"
																		aria-expanded={reqExpanded}
																		aria-controls={reqExpanded ? reqPanelId : undefined}
																		aria-label={`Request #${req.request_id}, ${req.findings.length} ${req.findings.length === 1 ? 'finding' : 'findings'}, ${reqExpanded ? 'collapse' : 'expand'}`}
																		class="flex min-w-0 flex-1 cursor-pointer items-center gap-3 px-3 py-2 outline-none transition-colors hover:bg-[var(--surface-2)] focus-visible:ring-1 focus-visible:ring-[var(--interactive-focus-ring)]"
																		onclick={() => toggle(rk)}
																		onkeydown={(e) => handleRowKey(e, rk)}
																	>
																		<DisclosureMarker open={reqExpanded} size={12} />
																		<span class="font-mono text-xs text-[var(--text-strong)]"
																			>#{req.request_id}</span
																		>
																		<StatusPill status={coerceStatus(req.request.status)} />
																		<span
																			class="text-xs text-[var(--text-2)] whitespace-pre-wrap break-words"
																			>{req.request.human_prompt}</span
																		>
																		<span class="ml-auto font-mono text-xs text-[var(--text-1)]"
																			>{req.findings.length}</span
																		>
																		<span class="text-label text-[var(--text-3)]"
																			>{formatRelative(req.newest_created_at, now)}</span
																		>
																	</div>
																	<a
																		href={resolve(requestHref(session, req))}
																		class="flex items-center justify-center border-l border-[var(--border-subtle)] px-3 outline-none transition-colors hover:bg-[var(--status-info-bg)] hover:text-[var(--status-info-fg)] focus-visible:ring-1 focus-visible:ring-[var(--interactive-focus-ring)]"
																		aria-label={`Open request #${req.request_id}`}
																		onclick={(e) => e.stopPropagation()}
																	>
																		<ArrowRight
																			class="size-3.5 text-[var(--text-3)]"
																			aria-hidden="true"
																		/>
																	</a>
																</div>

																{#if reqExpanded}
																	<ul
																		id={reqPanelId}
																		class="flex flex-col gap-3 border-t border-[var(--border-subtle)] px-3 py-3"
																		transition:slide={{ duration: 240 }}
																	>
																		{#each req.findings as finding, idx (`${finding.analyzer}-${idx}`)}
																			<li
																				class="border border-[var(--border-subtle)] bg-[var(--surface-card)] p-3"
																				data-slot="finding-row"
																			>
																				<div class="flex flex-wrap items-center gap-2">
																					<span class="font-mono text-label text-[var(--text-2)]"
																						>{checkLabel(finding.analyzer)}</span
																					>
																					{#if finding.other_hits > 0}
																						<span
																							class="inline-block bg-[var(--surface-2)] px-2 py-0.5 text-caption font-medium text-[var(--text-2)]"
																							title="Additional matches dedup'd into this finding"
																						>
																							+{finding.other_hits} more
																						</span>
																					{/if}
																					<span
																						class="ml-auto font-mono text-caption text-[var(--text-3)]"
																						title="Matched byte range in the scanned text"
																						>{matchSpanLabel(finding.match_range)}</span
																					>
																				</div>
																				<p class="mt-2 text-xs italic text-[var(--text-2)]">
																					{finding.advice}
																				</p>
																				<code
																					class="mt-2 block whitespace-pre-wrap break-all bg-[var(--surface-bg)] px-2 py-1.5 font-mono text-label text-[var(--text-strong)]"
																					>{finding.matched_text}</code
																				>
																				<div class="mt-2">
																					<a
																						href={resolve(
																							`sessions/${encodeURIComponent(session.session_id)}/requests/${req.request_id}?drawer=response`
																						)}
																						class="inline-flex items-center gap-1 text-label text-[var(--status-info-fg)] hover:underline focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--interactive-focus-ring)]"
																						aria-label={`Open full response for request #${req.request_id}`}
																					>
																						Open full response
																						<ArrowRight class="size-3" aria-hidden="true" />
																					</a>
																				</div>
																			</li>
																		{/each}
																	</ul>
																{/if}
															</li>
														{/each}
													</ul>
												{/if}
											</li>
										{/each}
									</ul>
								{/if}
							</li>
						{/each}
					</ul>
				{/if}

				{#if findingsHasMore}
					<div
						class="mt-4 flex flex-col items-stretch border-t border-[var(--border-subtle)] pt-3"
						data-slot="findings-load-more"
					>
						<span class="an-load-more-backing">
							<button
								type="button"
								class="an-load-more"
								data-augmented-ui="tl-clip br-clip border"
								disabled={findingsLoadingMore}
								onclick={handleLoadOlderFindings}
							>
								{findingsLoadingMore ? 'Loading…' : 'Load older findings'}
							</button>
						</span>
						{#if findingsLoadMoreError}
							<span class="mt-2 text-caption text-[var(--status-danger-fg)]" role="alert">
								{findingsLoadMoreError}
							</span>
						{/if}
					</div>
				{/if}
			</div>
		</section>
	{/if}
</section>

<style>
	.analysis-page {
		padding: 24px;
		flex: 1 1 auto;
		min-height: 0;
		overflow-y: auto;
	}

	.an-kpi-card,
	.an-panel {
		--edge-tl: 8px;
		--edge-br: 10px;
		--aug-tl: var(--edge-tl);
		--aug-br: var(--edge-br);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--data-teal-fg) 0%,
			var(--data-teal-border) 45%,
			var(--border-default) 80%
		);
		background: var(--surface-card);
	}
	.an-kpi-card {
		padding: calc(var(--edge-tl) + 16px) calc(var(--edge-br) + 16px) calc(var(--edge-br) + 16px)
			calc(var(--edge-tl) + 16px);
	}
	.an-panel {
		padding: calc(var(--edge-tl) + 16px) calc(var(--edge-br) + 16px) calc(var(--edge-br) + 16px)
			calc(var(--edge-tl) + 16px);
	}

	.an-skeleton-bar {
		background: var(--surface-2);
	}
	.an-skeleton-bar--label {
		height: 10px;
		width: 80px;
	}
	.an-skeleton-bar--value {
		height: 30px;
		width: 64px;
		margin-top: 12px;
	}
	.an-skeleton-bar--title {
		height: 14px;
		width: 128px;
	}
	.an-skeleton-bar--row {
		height: 8px;
		width: 100%;
	}

	.an-heading-backing {
		align-self: flex-start;
		display: inline-flex;
		background: var(--surface-card);
	}
	.an-heading {
		--aug-border-all: 1px;
		--aug-border-bg: var(--data-teal-border);
		--aug-tl: 0;
		--aug-tr: 6px;
		--aug-br: 0;
		--aug-bl: 6px;
		margin: 0;
		padding: 6px 10px;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--data-teal-fg);
		background:
			linear-gradient(0deg, var(--data-teal-bg), var(--data-teal-bg)), var(--surface-card);
	}

	.an-chip-backing {
		display: inline-flex;
		background: var(--surface-card);
	}
	.an-chip {
		--aug-border-all: 1px;
		--aug-tl: 6px;
		--aug-br: 6px;
		display: inline-flex;
		align-items: center;
		gap: 8px;
		padding: 3px 8px;
		font-size: var(--text-label);
	}
	.an-chip--warning {
		--aug-border-bg: var(--status-warning-border);
		color: var(--status-warning-fg);
		background:
			linear-gradient(0deg, var(--status-warning-bg), var(--status-warning-bg)), var(--surface-card);
	}
	.an-chip--info {
		--aug-border-bg: var(--status-info-border);
		color: var(--status-info-fg);
		background:
			linear-gradient(0deg, var(--status-info-bg), var(--status-info-bg)), var(--surface-card);
	}
	.an-chip-clear {
		display: inline-flex;
		align-items: center;
		background: none;
		border: none;
		padding: 0;
		margin: 0;
		color: inherit;
		text-decoration: none;
		cursor: pointer;
	}
	.an-chip-clear:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 1px;
	}

	.an-badge {
		padding: 2px 8px;
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}

	.an-load-more-backing {
		align-self: center;
		display: inline-flex;
		background: var(--surface-bg);
	}
	.an-load-more {
		--edge: 6px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		padding: 7px 14px;
		background: var(--surface-card);
		color: var(--text-2);
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.06em;
		font-weight: 600;
		cursor: pointer;
		transition: color 0.12s ease;
	}
	.an-load-more:hover:not(:disabled) {
		color: var(--text-strong);
		--aug-border-bg: var(--interactive-active-border);
	}
	.an-load-more:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.an-load-more:disabled {
		cursor: not-allowed;
		opacity: 0.6;
	}
</style>
