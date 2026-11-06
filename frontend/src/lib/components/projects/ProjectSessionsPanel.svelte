<script lang="ts">
	import { onDestroy, onMount, untrack } from 'svelte';
	import { Search, X } from '@lucide/svelte';
	import { SESSION_LIST_STORE_KEY } from '$lib/context-keys';
	import { getContext } from 'svelte';
	import type { SessionListStore } from '$lib/stores/sessions.svelte';
	import { listSessions, patchSessionPin, ApiError } from '$lib/api';
	import { createDetailMetricsLoader } from '$lib/components/sessions/detailMetrics.svelte';
	import { formatCount } from '$lib/time';
	import type { SessionSummary, ProxyEvent } from '$lib/types';
	import type { FleetProjectWithCounts } from '$lib/fleet/types';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import SessionTableHead from '$lib/components/sessions/SessionTableHead.svelte';
	import SessionRow from '$lib/components/sessions/SessionRow.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';

	interface Props {
		project: FleetProjectWithCounts;
		nested?: boolean;
	}

	const { project, nested = false }: Props = $props();
	const projectPath = $derived(project.project_path);

	const sessionListStore = getContext<SessionListStore | undefined>(SESSION_LIST_STORE_KEY);

	const sectionKey = $derived(`project:${projectPath}:sessions`);

	let now = $state(Date.now());
	onMount(() => {
		const id = setInterval(() => {
			now = Date.now();
		}, 1_000);
		return () => clearInterval(id);
	});

	function latestWsEventForSession(sessionId: string): ProxyEvent | null {
		return sessionListStore?.latestWsEventForSession(sessionId) ?? null;
	}

	const detailLoader = createDetailMetricsLoader();
	onDestroy(() => detailLoader.dispose());

	const PAGE_LIMIT = 15;
	const SEARCH_MIN_QUERY = 2;
	const SEARCH_DEBOUNCE_MS = 250;

	let sessions = $state<SessionSummary[]>([]);
	let loading = $state(false);
	let loadingMore = $state(false);
	let error = $state<string | null>(null);
	let loadMoreError = $state<string | null>(null);
	let hasFetchedOnce = $state(false);
	let hasMore = $state(false);
	let rawOffset = $state(0);
	let fetchSeq = 0;

	let activeWindow = $state<'week' | 'all'>('week');
	let rawQuery = $state('');
	let appliedSearch = $state('');
	const trimmedRawQuery = $derived(rawQuery.trim());
	const isSearchActive = $derived(appliedSearch.length >= SEARCH_MIN_QUERY);

	function dedupeById(rows: SessionSummary[]): SessionSummary[] {
		const seen = new Set<string>();
		const out: SessionSummary[] = [];
		for (const row of rows) {
			if (seen.has(row.id)) continue;
			seen.add(row.id);
			out.push(row);
		}
		return out;
	}

	async function fetchPage(opts: { reset: boolean }): Promise<void> {
		if (!opts.reset && loadingMore) return;
		const seq = ++fetchSeq;
		if (opts.reset) {
			loading = true;
			error = null;
		} else {
			loadingMore = true;
			loadMoreError = null;
		}
		const offsetToUse = opts.reset ? 0 : rawOffset;
		const searchParam = isSearchActive ? appliedSearch : undefined;
		const dateFrom = activeWindow === 'all' ? project.first_seen : undefined;
		try {
			const raw = await listSessions({
				project: projectPath,
				offset: offsetToUse,
				limit: PAGE_LIMIT,
				search: searchParam,
				date_from: dateFrom
			});
			if (seq !== fetchSeq) return;
			const exact = raw.filter((s) => s.project === projectPath);
			sessions = dedupeById(opts.reset ? exact : [...sessions, ...exact]);
			rawOffset = offsetToUse + raw.length;
			hasMore = raw.length === PAGE_LIMIT;
		} catch (err) {
			if (seq !== fetchSeq) return;
			const message = err instanceof ApiError ? err.message : String(err);
			if (opts.reset) {
				sessions = [];
				error = message;
			} else {
				loadMoreError = message;
			}
		} finally {
			if (seq === fetchSeq) {
				loading = false;
				loadingMore = false;
				if (opts.reset) hasFetchedOnce = true;
			}
		}
	}

	$effect(() => {
		const q = rawQuery;
		const t = setTimeout(() => {
			appliedSearch = q.trim();
		}, SEARCH_DEBOUNCE_MS);
		return () => clearTimeout(t);
	});

	$effect(() => {
		void projectPath;
		void activeWindow;
		void appliedSearch;
		untrack(() => {
			void fetchPage({ reset: true });
		});
	});

	function loadOlder(): void {
		void fetchPage({ reset: false });
	}

	function retry(): void {
		void fetchPage({ reset: true });
	}

	function switchToAllTime(): void {
		activeWindow = 'all';
	}

	async function togglePin(session: SessionSummary): Promise<void> {
		const next = !(session.pinned === true);
		const idx = sessions.findIndex((s) => s.id === session.id);
		if (idx === -1) return;
		const previous = sessions[idx]!.pinned;
		sessions = sessions.map((s, i) => (i === idx ? { ...s, pinned: next } : s));
		try {
			const res = await patchSessionPin(session.id, next);
			sessions = sessions.map((s) => (s.id === session.id ? { ...s, pinned: res.pinned } : s));
		} catch {
			sessions = sessions.map((s) => (s.id === session.id ? { ...s, pinned: previous } : s));
		}
	}

	function clearSearch(): void {
		rawQuery = '';
	}

	const allTimeLabel = $derived(formatCount(project.session_count));
	const countLine = $derived.by((): string => {
		if (!hasFetchedOnce) return 'Loading…';
		if (isSearchActive) {
			const noun = sessions.length === 1 ? 'session' : 'sessions';
			const scope = activeWindow === 'week' ? ' in the last 7 days' : '';
			return `${formatCount(sessions.length)} matching ${noun}${scope} · ${allTimeLabel} all time`;
		}
		if (activeWindow === 'week') {
			if (hasMore) {
				return `Showing the ${formatCount(sessions.length)} most recent from the last 7 days · ${allTimeLabel} all time`;
			}
			const noun = sessions.length === 1 ? 'session' : 'sessions';
			return `${formatCount(sessions.length)} ${noun} in the last 7 days · ${allTimeLabel} all time`;
		}
		if (hasMore) {
			return `Showing ${formatCount(sessions.length)} of ${allTimeLabel} sessions, newest first`;
		}
		return `All ${allTimeLabel} sessions`;
	});

	const isSearchEmpty = $derived(hasFetchedOnce && sessions.length === 0 && isSearchActive);
	const isWindowEmptyWithMore = $derived(
		hasFetchedOnce &&
			sessions.length === 0 &&
			!isSearchActive &&
			activeWindow === 'week' &&
			project.session_count > 0
	);
	const isGenuinelyEmpty = $derived(
		hasFetchedOnce && sessions.length === 0 && !isSearchActive && !isWindowEmptyWithMore
	);
</script>

<div class="psp-scope" data-slot="project-sessions-panel">
	<SectionCard
		title="Sessions"
		collapsible
		bordered={!nested}
		{sectionKey}
		defaultCollapsed={false}
		titleTooltip="Sessions captured for this project. The last-7-days view is what the API returns by default; switch to all time to reach older ones."
	>
		{#snippet headerActions()}
			<span class="psp-meta" data-slot="project-sessions-count">{countLine}</span>
		{/snippet}
		{#snippet children()}
			<div class="psp-controls">
				<div class="psp-search-wrap" data-augmented-ui="tl-clip border">
					<Search class="psp-search-icon" size={13} aria-hidden="true" />
					<input
						bind:value={rawQuery}
						type="search"
						class="psp-search-input"
						placeholder="Search by session title or full id"
						autocomplete="off"
						autocapitalize="off"
						spellcheck="false"
						aria-label="Search this project's sessions by title or full id"
						data-slot="project-sessions-search"
					/>
					{#if trimmedRawQuery.length > 0}
						<button
							type="button"
							class="psp-search-clear"
							aria-label="Clear search"
							onclick={clearSearch}
						>
							<X size={12} aria-hidden="true" />
						</button>
					{/if}
				</div>

				<div class="psp-seg" role="group" aria-label="Which sessions to show">
					<div class="psp-seg-item">
						<button
							type="button"
							class="psp-seg-btn"
							class:psp-seg-btn--active={activeWindow === 'week'}
							aria-pressed={activeWindow === 'week'}
							onclick={() => (activeWindow = 'week')}
							data-slot="project-sessions-window-week"
						>
							<span class="psp-seg-label">Last 7 days</span>
						</button>
					</div>
					<div class="psp-seg-item">
						<button
							type="button"
							class="psp-seg-btn"
							class:psp-seg-btn--active={activeWindow === 'all'}
							aria-pressed={activeWindow === 'all'}
							onclick={switchToAllTime}
							data-slot="project-sessions-window-all"
						>
							<span class="psp-seg-label">All time</span>
						</button>
					</div>
				</div>

				<Tooltip
					text="Matches a session's title, or a session id pasted in full — partial ids don't match. It doesn't search prompts, conversation or response bodies; the Sessions page search covers those."
				>
					<span class="psp-search-scope">titles · full ids</span>
				</Tooltip>
			</div>

			<div class="psp-table" data-slot="project-sessions-table">
				{#if error}
					<div class="psp-state">
						<LoadErrorBanner
							title="Couldn't load this project's sessions"
							detail={error}
							hint="The inspect API may not be reachable on :9989."
							onRetry={retry}
						/>
					</div>
				{:else if loading && sessions.length === 0}
					<div class="psp-skeleton" aria-hidden="true" data-slot="project-sessions-loading">
						<div class="psp-skel-row"></div>
						<div class="psp-skel-row"></div>
						<div class="psp-skel-row psp-skel-row--short"></div>
					</div>
				{:else if isSearchEmpty}
					<div class="psp-state" data-slot="project-sessions-empty">
						<p class="psp-empty">
							No sessions match "{appliedSearch}"{activeWindow === 'week'
								? ' in the last 7 days'
								: ''}.
						</p>
						<p class="psp-empty">Search covers session titles and full session ids only.</p>
						{#if activeWindow === 'week'}
							<button type="button" class="psp-inline-action" onclick={switchToAllTime}>
								switch to all time
							</button>
						{/if}
					</div>
				{:else if isWindowEmptyWithMore}
					<div class="psp-state" data-slot="project-sessions-empty">
						<p class="psp-empty">No sessions in the last 7 days.</p>
						<p class="psp-empty">This project has {allTimeLabel} older ones.</p>
						<button
							type="button"
							class="psp-seg-btn psp-seg-btn--active psp-show-all"
							onclick={switchToAllTime}
						>
							<span class="psp-seg-label">Show all {allTimeLabel}</span>
						</button>
					</div>
				{:else if isGenuinelyEmpty}
					<div class="psp-state" data-slot="project-sessions-empty">
						<p class="psp-empty">No sessions captured for this project.</p>
					</div>
				{:else}
					<div class="psp-thead"><SessionTableHead /></div>
					<div class="psp-tbody">
						{#each sessions as s (s.id)}
							<SessionRow
								session={s}
								{now}
								detail={detailLoader.metrics.get(s.id)}
								{latestWsEventForSession}
								onHoverIntent={detailLoader.scheduleLoad}
								onHoverCancel={detailLoader.cancelLoad}
								onTogglePin={(session) => void togglePin(session)}
							/>
						{/each}
					</div>
					{#if hasMore}
						<button
							type="button"
							class="psp-load-older"
							disabled={loadingMore}
							onclick={loadOlder}
							data-slot="project-sessions-load-older"
						>
							{loadingMore ? 'Loading' : 'Load older sessions'}
						</button>
						{#if loadMoreError}
							<p class="psp-load-error" role="alert" data-slot="project-sessions-load-older-error">
								{loadMoreError}
							</p>
						{/if}
					{/if}
				{/if}
			</div>
		{/snippet}
	</SectionCard>
</div>

<style>
	.psp-scope {
		min-width: 0;
		container-type: inline-size;
		container-name: session-table;
	}

	.psp-meta {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption, 11px);
		color: var(--text-3);
		white-space: nowrap;
	}

	.psp-controls {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: var(--space-2);
		min-width: 0;
		margin-bottom: var(--space-3, 12px);
	}
	.psp-search-wrap {
		position: relative;
		display: flex;
		align-items: center;
		height: var(--control-h);
		flex: 1 1 220px;
		min-width: 0;
		background: var(--surface-2);
		--aug-tl: var(--edge-sm);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		transition: filter var(--dur-1) var(--ease-io);
	}
	.psp-search-wrap:hover {
		--aug-border-bg: var(--border-strong);
	}
	.psp-search-wrap:focus-within {
		--aug-border-bg: var(--interactive-focus-ring);
		filter: drop-shadow(0 0 2px var(--interactive-focus-ring))
			drop-shadow(0 0 12px var(--interactive-focus-ring));
	}
	.psp-search-wrap :global(.psp-search-icon) {
		position: absolute;
		left: 9px;
		color: var(--text-faint);
		pointer-events: none;
	}
	.psp-search-input {
		width: 100%;
		height: 100%;
		font-family: var(--font-mono);
		font-size: var(--text-label);
		color: var(--text-1);
		background: transparent;
		border: none;
		border-radius: 0;
		padding: 0 28px 0 28px;
		outline: none;
	}
	.psp-search-input::placeholder {
		color: var(--text-faint);
	}
	.psp-search-input::-webkit-search-cancel-button,
	.psp-search-input::-webkit-search-decoration {
		-webkit-appearance: none;
	}
	.psp-search-clear {
		position: absolute;
		right: 6px;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 18px;
		height: 18px;
		padding: 0;
		background: transparent;
		border: none;
		border-radius: 2px;
		color: var(--text-faint);
		cursor: pointer;
		transition:
			color var(--dur-1) var(--ease-snap),
			background var(--dur-1) var(--ease-snap);
	}
	.psp-search-clear:hover {
		color: var(--text-strong);
		background: var(--surface-3);
	}
	.psp-search-clear:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 1px;
	}

	.psp-seg {
		display: flex;
		align-items: center;
		gap: 2px;
		border-radius: var(--r-tol);
		border: 1px solid var(--border-default);
		background: var(--surface-bg);
		padding: 2px;
		flex-shrink: 0;
	}
	.psp-seg-item {
		position: relative;
		display: inline-flex;
	}
	.psp-seg-btn {
		display: inline-flex;
		align-items: center;
		height: var(--control-h);
		gap: var(--space-1, 4px);
		white-space: nowrap;
		border-radius: var(--r-tol);
		border: 1px solid transparent;
		padding-inline: var(--space-2, 8px);
		background: transparent;
		color: var(--text-2);
		cursor: pointer;
		transition:
			color var(--dur-1) var(--ease-snap),
			background-color var(--dur-1) var(--ease-snap),
			border-color var(--dur-1) var(--ease-snap);
	}
	.psp-seg-btn:not(.psp-seg-btn--active):hover {
		color: var(--text-1);
	}
	.psp-seg-btn--active {
		color: var(--data-teal-fg);
		background: var(--data-teal-bg);
		border-color: var(--data-teal-border);
	}
	.psp-seg-btn:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}
	.psp-seg-label {
		font-family: var(--font-display);
		font-size: var(--text-label);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: var(--track-machine, 0.08em);
	}
	.psp-show-all {
		border-color: var(--data-teal-border);
		align-self: flex-start;
	}

	.psp-search-scope {
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-faint);
		white-space: nowrap;
		text-decoration: underline dotted;
		text-decoration-color: var(--border-strong);
		text-underline-offset: 3px;
		transition: color var(--dur-1) var(--ease-snap);
	}
	:global(.tooltip-trigger:hover) .psp-search-scope,
	:global(.tooltip-trigger:focus-visible) .psp-search-scope {
		color: var(--text-2);
		text-decoration-color: currentColor;
	}

	.psp-table {
		min-width: 0;
	}
	.psp-tbody {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	@container session-table (max-width: 799.98px) {
		.psp-thead {
			display: none;
		}
	}

	.psp-state {
		padding: var(--space-2) 0;
	}
	.psp-skeleton {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.psp-skel-row {
		height: 14px;
		border-radius: var(--r-tol);
		background: var(--surface-2);
	}
	.psp-skel-row--short {
		width: 60%;
	}
	.psp-empty {
		margin: 0;
		font-size: var(--text-sm);
		font-style: italic;
		color: var(--text-2);
	}
	.psp-empty + .psp-empty {
		margin-top: var(--space-1);
	}
	.psp-inline-action {
		margin-top: var(--space-2);
		border: none;
		background: transparent;
		padding: 0;
		font-size: var(--text-sm);
		color: var(--interactive-active-fg);
		text-decoration: underline dotted;
		text-underline-offset: 3px;
		cursor: pointer;
	}
	.psp-inline-action:hover {
		text-decoration-style: solid;
	}
	.psp-inline-action:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
		border-radius: 2px;
	}

	.psp-load-older {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 100%;
		height: var(--control-h);
		border: 0;
		border-top: 1px solid var(--border-subtle);
		background: var(--surface-2);
		cursor: pointer;
		font-family: var(--font-display);
		font-size: var(--text-label);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: var(--track-machine, 0.08em);
		color: var(--text-3);
		transition:
			background var(--dur-1) var(--ease-snap),
			color var(--dur-1) var(--ease-snap);
	}
	.psp-load-older:hover:not(:disabled) {
		background: var(--surface-3);
		color: var(--interactive-active-fg);
	}
	.psp-load-older:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}
	.psp-load-older:disabled {
		opacity: 0.6;
		cursor: default;
	}
	.psp-load-error {
		margin: 4px 0 0;
		font-size: var(--text-label);
		color: var(--status-danger-fg);
		line-height: 1.5;
	}
</style>
