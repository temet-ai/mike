<script lang="ts">
	import { onMount, onDestroy, getContext } from 'svelte';
	import { Users } from '@lucide/svelte';
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import { listAgentDispatches, ApiError } from '$lib/api';
	import type { AgentDispatchDto } from '$lib/types';
	import { formatRelative } from '$lib/time';
	import { shortSessionId } from '$lib/format';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import HelpHint from '$lib/components/shared/HelpHint.svelte';
	import { onApiCacheUpdate } from '$lib/stores/cache/apiCache.svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import FilteredEmptyState from '$lib/components/shared/FilteredEmptyState.svelte';
	import FilterScopeStatus from '$lib/components/shared/FilterScopeStatus.svelte';

	const PAGE_SIZE = 50;

	let rows = $state<AgentDispatchDto[]>([]);
	let loading = $state(false);
	let errorMessage = $state<string | null>(null);
	let loadingMore = $state(false);
	let loadMoreError = $state<string | null>(null);
	let lastPageFull = $state(false);
	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);
	let expandedLocal = $state<Set<string>>(new Set());
	let openedOnce = $state<Set<string>>(new Set());

	let projectFilter = $state(page.url.searchParams.get('project') ?? '');
	let sinceFilter = $state('');
	let sinceError = $state<string | null>(null);

	let appliedProject = $state<string | null>(null);
	let appliedSince = $state<string | null>(null);

	const hasAppliedFilters = $derived(appliedProject !== null || appliedSince !== null);
	const scopeLabel = $derived(
		hasAppliedFilters ? describeApplied(appliedProject, appliedSince) : 'every captured dispatch'
	);
	const isScopeDirty = $derived(
		(projectFilter.trim() || null) !== appliedProject ||
			(sinceFilter.trim() || null) !== appliedSince
	);
	const scopeShownCount = $derived(loading || errorMessage ? undefined : rows.length);
	const scopeTotalCount = $derived(
		loading || errorMessage || lastPageFull ? undefined : rows.length
	);

	function describeApplied(project: string | null, since: string | null): string {
		const parts: string[] = [];
		if (project) parts.push(`project "${project}"`);
		if (since) parts.push(`since ${since}`);
		return parts.join(', ');
	}

	let now = $state(Date.now());
	let tickTimer: ReturnType<typeof setInterval> | null = null;

	function currentParams(offset: number) {
		const params: { offset: number; limit: number; project?: string; since?: string } = {
			offset,
			limit: PAGE_SIZE
		};
		if (projectFilter.trim()) params.project = projectFilter.trim();
		if (sinceFilter.trim()) params.since = sinceFilter.trim();
		return params;
	}

	async function load(): Promise<void> {
		const trimmedProject = projectFilter.trim();
		const trimmedSince = sinceFilter.trim();
		sinceError = null;
		if (trimmedSince && Number.isNaN(Date.parse(trimmedSince))) {
			sinceError =
				'Enter a date and time this browser can parse, for example 2026-05-01T00:00:00Z or 2026-05-01.';
			return;
		}
		loading = true;
		errorMessage = null;
		loadMoreError = null;
		try {
			const page0 = await listAgentDispatches(currentParams(0));
			rows = page0;
			lastPageFull = page0.length === PAGE_SIZE;
			appliedProject = trimmedProject || null;
			appliedSince = trimmedSince || null;
		} catch (err) {
			errorMessage = err instanceof ApiError ? err.message : String(err);
			rows = [];
			lastPageFull = false;
		} finally {
			loading = false;
		}
	}

	async function loadMore(): Promise<void> {
		if (loadingMore) return;
		loadingMore = true;
		loadMoreError = null;
		try {
			const next = await listAgentDispatches(currentParams(rows.length));
			rows = [...rows, ...next];
			lastPageFull = next.length === PAGE_SIZE;
		} catch (err) {
			loadMoreError = err instanceof ApiError ? err.message : String(err);
		} finally {
			loadingMore = false;
		}
	}

	function applyFilters(event: SubmitEvent): void {
		event.preventDefault();
		void load();
	}

	function clearFilters(): void {
		projectFilter = '';
		sinceFilter = '';
		void load();
	}

	function isExpanded(key: string): boolean {
		return collapseStore ? collapseStore.isExpanded(key) : expandedLocal.has(key);
	}

	function toggle(key: string): void {
		if (!openedOnce.has(key)) {
			// eslint-disable-next-line svelte/prefer-svelte-reactivity -- cloned, mutated, then reassigned wholesale; reactivity comes from the reassignment
			const seen = new Set(openedOnce);
			seen.add(key);
			openedOnce = seen;
		}
		if (collapseStore) {
			collapseStore.toggle(key);
			return;
		}
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- cloned, mutated, then reassigned wholesale; reactivity comes from the reassignment below, not in-place mutation
		const next = new Set(expandedLocal);
		if (next.has(key)) next.delete(key);
		else next.add(key);
		expandedLocal = next;
	}

	function hasOpened(key: string): boolean {
		return openedOnce.has(key) || isExpanded(key);
	}

	function shortToolId(id: string): string {
		return id.length <= 12 ? id : `${id.slice(0, 12)}…`;
	}

	onMount(() => {
		void load();
		tickTimer = setInterval(() => {
			now = Date.now();
		}, 1000);
		const unsub = onApiCacheUpdate(
			'/api/agent-dispatches',
			() => {
				void load();
			},
			{ prefix: true }
		);
		return () => {
			unsub();
		};
	});

	onDestroy(() => {
		if (tickTimer !== null) clearInterval(tickTimer);
		tickTimer = null;
	});
</script>

<svelte:head>
	<title>Agent dispatches — mike</title>
</svelte:head>

<section class="ad-page">
	<PageIntro title="Agent dispatches" learnMoreHref="/help/glossary#subagent">
		<span class="flex items-center gap-1.5">
			<span>Drills into every</span>
			<span class="inline-flex items-center gap-1">
				<span>subagent</span>
				<HelpHint slug="subagent" />
			</span>
			<span
				>dispatch captured from <code class="font-mono text-[var(--text-2)]">Agent</code> tool calls.
				The full brief lives in the parent request body — open it via the "View brief →" link.</span
			>
		</span>
	</PageIntro>

	<form class="ad-filters" data-augmented-ui="tl-clip br-clip border" onsubmit={applyFilters}>
		<label class="ad-filter">
			<span class="ad-filter__label">Project</span>
			<input
				type="text"
				bind:value={projectFilter}
				placeholder="exact project path"
				class="ad-filter__input"
			/>
		</label>
		<label class="ad-filter">
			<span class="ad-filter__label">Since</span>
			<input
				type="text"
				bind:value={sinceFilter}
				placeholder="2026-05-01T00:00:00Z"
				class="ad-filter__input"
				class:ad-filter__input--error={!!sinceError}
				aria-invalid={!!sinceError}
				aria-describedby="ad-since-hint"
				oninput={() => {
					sinceError = null;
				}}
			/>
			{#if sinceError}
				<p class="ad-field-error" id="ad-since-hint" role="alert">{sinceError}</p>
			{:else}
				<p class="ad-field-hint" id="ad-since-hint">
					Any date and time format the browser can parse, for example an ISO 8601 timestamp
					(2026-05-01T00:00:00Z) or a bare date (2026-05-01).
				</p>
			{/if}
		</label>
		<div class="ad-filter__actions">
			<button type="submit" class="ad-btn ad-btn--primary" disabled={loading}>Apply</button>
			<button type="button" class="ad-btn" onclick={clearFilters} disabled={loading}>Clear</button>
		</div>
		<FilterScopeStatus
			{scopeLabel}
			shownCount={scopeShownCount}
			totalCount={scopeTotalCount}
			itemNoun="dispatch"
			itemNounPlural="dispatches"
			dirty={isScopeDirty}
		/>
	</form>

	{#if errorMessage}
		<LoadErrorBanner
			title="Couldn't load agent dispatches"
			detail={errorMessage}
			onRetry={() => void load()}
		/>
	{:else if loading && rows.length === 0}
		<div class="ad-status" role="status" aria-live="polite">Loading…</div>
	{:else if rows.length === 0}
		<FilteredEmptyState
			icon={Users}
			filtered={hasAppliedFilters}
			emptyText="No agent dispatches recorded"
			emptySubtext="Sub-agent spawns appear here once a session dispatches a Task tool call."
			filteredText={`No dispatches match ${describeApplied(appliedProject, appliedSince)}.`}
			onClearFilters={clearFilters}
		/>
	{:else}
		<ul class="ad-list" role="list">
			{#each rows as row (`${row.parent_request_id}:${row.tool_id}:${row.ord}`)}
				{@const key = `agent-dispatch:${row.parent_request_id}:${row.tool_id}:${row.ord}`}
				{@const isOpen = isExpanded(key)}
				{@const label = `${row.parent_request_id}:${row.tool_id}:${row.ord}`}
				{@const bodyId = `ad-body-${row.parent_request_id}-${row.tool_id}-${row.ord}`}
				<li class="ad-row" class:ad-row--expanded={isOpen}>
					<div class="ad-row__head">
						<button
							type="button"
							class="ad-row__disclose"
							aria-expanded={isOpen}
							aria-controls={bodyId}
							aria-label={isOpen ? `Collapse dispatch ${label}` : `Expand dispatch ${label}`}
							onclick={() => toggle(key)}
						>
							<span class="ad-row__line">
								<DisclosureMarker open={isOpen} size={12} />
								<span class="ad-type-backing">
									<span class="ad-type" data-augmented-ui="tl-clip br-clip border"
										>{row.subagent_type ?? '—'}</span
									>
								</span>
								<span class="ad-ts" title={row.created_at}
									>{formatRelative(row.created_at, now)}</span
								>
								<span class="ad-meta">{row.brief_char_len} chars</span>
								<span class="ad-meta">#{row.ord + 1}</span>
							</span>
							{#if row.description}
								<span class="ad-row__desc-preview">{row.description}</span>
							{/if}
						</button>
						<div class="ad-row__meta">
							<a class="ad-link" href={resolve(`sessions/${encodeURIComponent(row.session_id)}`)}>
								{shortSessionId(row.session_id)}
							</a>
							<a
								class="ad-link"
								href={resolve(
									`sessions/${encodeURIComponent(row.session_id)}/requests/${row.parent_request_id}`
								)}
								aria-label="Open parent request #{row.parent_request_id}"
							>
								req #{row.parent_request_id}
							</a>
							<Tooltip text="Anthropic tool call ID — full id: {row.tool_id}">
								<span class="ad-meta ad-meta--id">{shortToolId(row.tool_id)}</span>
							</Tooltip>
						</div>
					</div>
					<div class="ad-row__body" class:ad-row__body--open={isOpen} id={bodyId}>
						<div class="ad-row__body-clip" inert={!isOpen}>
							{#if hasOpened(key)}
								<div class="ad-detail">
									{#if row.description}
										<div class="ad-block">
											<div class="ad-block__label">Description</div>
											<!-- svelte-ignore a11y_no_noninteractive_tabindex (axe scrollable-region-focusable requires the overflow container be keyboard-focusable) -->
											<pre
												class="ad-pre"
												tabindex="0"
												role="region"
												aria-label="Agent dispatch description (scrollable)">{row.description}</pre>
										</div>
									{:else}
										<div class="ad-block__label">No description captured.</div>
									{/if}
									<a
										class="ad-brief-link"
										href={resolve(
											`sessions/${encodeURIComponent(row.session_id)}/requests/${row.parent_request_id}?drawer=request`
										)}
										aria-label="View the full agent brief in the parent request body"
									>
										View brief →
									</a>
								</div>
							{/if}
						</div>
					</div>
				</li>
			{/each}
		</ul>
		{#if lastPageFull}
			<div class="ad-footer" data-augmented-ui="tl-clip br-clip border">
				<span class="ad-footer__count" role="status">
					A full page arrived — there may be more.
				</span>
				<button
					type="button"
					class="ad-footer__load-more"
					disabled={loadingMore}
					onclick={loadMore}
				>
					{loadingMore ? 'Loading…' : 'Load older dispatches'}
				</button>
				{#if loadMoreError}
					<span class="ad-footer__error" role="alert">{loadMoreError}</span>
				{/if}
			</div>
		{/if}
	{/if}
</section>

<style>
	.ad-page {
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 16px;
		max-width: 1200px;
		margin: 0 auto;
		flex: 1 1 auto;
		min-height: 0;
		overflow-y: auto;
	}
	.ad-filters {
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
		display: flex;
		gap: 12px;
		flex-wrap: wrap;
		align-items: flex-end;
		padding: calc(var(--edge-tl) + 4px) calc(var(--edge-br) + 4px) calc(var(--edge-br) + 4px)
			calc(var(--edge-tl) + 4px);
		background: var(--surface-card);
	}
	.ad-filter {
		display: flex;
		flex-direction: column;
		gap: 4px;
		min-width: 180px;
		flex: 1 1 180px;
	}
	.ad-filter__label {
		font-size: var(--text-label);
		color: var(--text-3);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	.ad-filter__input {
		padding: 6px 8px;
		border: 1px solid var(--border-default);
		border-radius: 0;
		background: var(--surface-bg);
		color: var(--text-1);
		font-size: 12px;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
	}
	.ad-filter__input:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}
	.ad-filter__input--error {
		border-color: var(--status-danger-border);
	}
	.ad-field-hint {
		margin: 0;
		font-size: 11px;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-3);
	}
	.ad-field-error {
		margin: 0;
		font-size: 11px;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--status-danger-fg);
	}
	.ad-filter__actions {
		display: flex;
		gap: 8px;
	}
	.ad-btn {
		padding: 6px 12px;
		border: 1px solid var(--border-default);
		border-radius: 0;
		background: var(--surface-card);
		color: var(--text-2);
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		cursor: pointer;
	}
	.ad-btn:hover {
		border-color: var(--interactive-active-fg);
		color: var(--text-strong);
	}
	.ad-btn:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}
	.ad-btn:disabled {
		cursor: not-allowed;
		opacity: 0.6;
	}
	.ad-btn--primary {
		border-color: var(--status-info-border);
		color: var(--status-info-fg);
	}
	.ad-status {
		padding: 16px 18px;
		font-size: 12px;
		color: var(--text-3);
	}
	.ad-list {
		--edge-tl: 8px;
		--edge-br: 12px;
		--aug-tl: var(--edge-tl);
		--aug-br: var(--edge-br);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--data-teal-fg) 0%,
			var(--data-teal-border) 45%,
			var(--border-default) 80%
		);
		list-style: none;
		margin: 0;
		padding: calc(var(--edge-tl) + 4px) calc(var(--edge-br) + 4px) calc(var(--edge-br) + 4px)
			calc(var(--edge-tl) + 4px);
		display: flex;
		flex-direction: column;
		background: var(--surface-card);
	}
	.ad-row {
		--row-bg: var(--surface-card);
		display: flex;
		flex-direction: column;
		border-bottom: 1px solid var(--border-subtle);
		border-left: 2px solid var(--data-teal-border);
		background: var(--row-bg);
		transition: background var(--dur-1) var(--ease-snap);
	}
	.ad-row__head {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		flex-wrap: wrap;
		min-width: 0;
		box-sizing: border-box;
		padding: 4px 8px;
		min-height: var(--control-h);
		border-bottom: 1px solid transparent;
	}
	.ad-row--expanded .ad-row__head {
		border-bottom-color: var(--border-subtle);
	}
	.ad-row__meta {
		display: flex;
		align-items: center;
		gap: 10px;
		flex: 0 0 auto;
		flex-wrap: wrap;
		min-height: 24px;
	}
	.ad-row:last-child {
		border-bottom: none;
	}
	.ad-row:hover {
		--row-bg: var(--surface-2);
	}
	.ad-row--expanded {
		--row-bg: var(--surface-2);
	}
	.ad-row__disclose {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 4px;
		flex: 1 1 auto;
		min-width: 0;
		background: none;
		border: none;
		border-radius: 0;
		padding: 0;
		margin: 0;
		text-align: left;
		cursor: pointer;
		color: inherit;
		font: inherit;
	}
	.ad-row__disclose:focus-visible {
		outline: none;
		box-shadow: inset 0 0 0 2px var(--interactive-focus-ring);
	}
	.ad-row__line {
		display: flex;
		align-items: center;
		gap: 10px;
		flex-wrap: wrap;
		min-width: 0;
		min-height: 24px;
	}
	.ad-row__body {
		display: grid;
		grid-template-rows: 0fr;
		transition: grid-template-rows var(--dur-2) var(--ease-io);
	}
	.ad-row__body--open {
		grid-template-rows: 1fr;
	}
	.ad-row__body-clip {
		overflow: hidden;
	}
	.ad-type-backing {
		display: inline-flex;
		background: var(--row-bg, var(--surface-card));
	}
	.ad-type {
		--aug-tl: 4px;
		--aug-br: 4px;
		--aug-border-all: 1px;
		--aug-border-bg: var(--data-teal-border);
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		font-size: var(--text-label);
		letter-spacing: 0.04em;
		color: var(--data-teal-fg);
		white-space: nowrap;
		padding: 2px 7px;
		background:
			linear-gradient(0deg, var(--data-teal-bg), var(--data-teal-bg)),
			var(--row-bg, var(--surface-card));
	}
	.ad-ts,
	.ad-meta {
		font-size: var(--text-caption);
		color: var(--text-3);
		white-space: nowrap;
	}
	.ad-meta {
		font-variant-numeric: tabular-nums;
	}
	.ad-meta--id {
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
	}
	.ad-link {
		font-size: var(--text-caption);
		color: var(--status-info-fg);
		text-decoration: underline;
		text-decoration-color: var(--status-info-border);
		text-underline-offset: 2px;
		white-space: nowrap;
	}
	.ad-link:hover {
		color: var(--text-strong);
		text-decoration-color: var(--status-info-fg);
	}
	.ad-link:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
		border-radius: 2px;
	}
	.ad-row__desc-preview {
		font-size: 12px;
		color: var(--text-2);
		word-break: break-word;
		overflow-wrap: anywhere;
		min-width: 0;
	}
	.ad-detail {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 8px;
	}
	.ad-block {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.ad-block__label {
		font-size: var(--text-label);
		color: var(--text-3);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	.ad-pre {
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: var(--text-label);
		color: var(--text-1);
		white-space: pre-wrap;
		word-break: break-word;
		overflow-wrap: anywhere;
		line-height: 1.55;
		margin: 0;
		padding: calc(var(--edge-sm) + var(--space-1));
		background: var(--surface-bg);
		border-radius: 0;
		max-height: 480px;
		overflow-y: auto;
	}
	.ad-brief-link {
		align-self: flex-start;
		font-size: var(--text-caption);
		color: var(--status-info-fg);
		text-decoration: underline;
		text-decoration-color: var(--status-info-border);
		text-underline-offset: 2px;
	}
	.ad-brief-link:hover {
		color: var(--text-strong);
		text-decoration-color: var(--status-info-fg);
	}
	.ad-brief-link:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
		border-radius: 2px;
	}
	.ad-footer {
		--edge-tl: 8px;
		--edge-br: 10px;
		--aug-tl: var(--edge-tl);
		--aug-br: var(--edge-br);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--border-default) 0%,
			var(--data-teal-border) 100%
		);
		display: flex;
		flex-direction: column;
		align-items: stretch;
		gap: 8px;
		padding: calc(var(--edge-tl) + 4px) calc(var(--edge-br) + 8px) calc(var(--edge-br) + 4px)
			calc(var(--edge-tl) + 8px);
		background: var(--surface-card);
	}
	.ad-footer__count {
		font-size: var(--text-caption);
		color: var(--text-3);
		text-align: center;
	}
	.ad-footer__load-more {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 6px 12px;
		border: 1px solid var(--border-default);
		border-radius: 0;
		background: var(--surface-card);
		color: var(--text-2);
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		cursor: pointer;
	}
	.ad-footer__load-more:hover {
		border-color: var(--interactive-active-fg);
		color: var(--text-strong);
	}
	.ad-footer__load-more:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}
	.ad-footer__load-more:disabled {
		cursor: not-allowed;
		opacity: 0.6;
	}
	.ad-footer__error {
		font-size: var(--text-caption);
		color: var(--status-danger-fg);
		text-align: center;
	}
</style>
