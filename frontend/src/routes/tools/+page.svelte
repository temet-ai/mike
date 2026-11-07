<script lang="ts">
	import { onMount, onDestroy, getContext, untrack } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { Wrench } from '@lucide/svelte';
	import { revealSections } from '$lib/actions/reveal';
	import { Button } from '$lib/components/ui/button';
	import { listToolUses, listToolUseFacets, ApiError } from '$lib/api';
	import type { ToolUsesFeedRow, ToolUseFacets } from '$lib/types';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import HelpHint from '$lib/components/shared/HelpHint.svelte';
	import FilterScopeStatus from '$lib/components/shared/FilterScopeStatus.svelte';
	import FilteredEmptyState from '$lib/components/shared/FilteredEmptyState.svelte';
	import ToolProjectGroup, {
		type ToolProjectGroupRow
	} from '$lib/components/tools/ToolProjectGroup.svelte';

	const PAGE_SIZE = 200;

	let rows = $state<ToolUsesFeedRow[]>([]);
	let totalCount = $state(0);
	let totalCapped = $state(false);
	let nextCursor = $state<string | null>(null);
	let loading = $state(false);
	let errorMessage = $state<string | null>(null);
	let loadingMore = $state(false);
	let loadMoreError = $state<string | null>(null);

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);

	// eslint-disable-next-line svelte/prefer-svelte-reactivity -- cloned, mutated, then reassigned wholesale; reactivity comes from the reassignment, not in-place mutation
	let expandedRowsLocal = $state<Set<string>>(new Set());
	let openedRowsOnce = $state<Set<string>>(new Set());

	function isRowExpanded(key: string): boolean {
		return collapseStore ? collapseStore.isExpanded(key) : expandedRowsLocal.has(key);
	}

	function toggleRow(key: string): void {
		if (!openedRowsOnce.has(key)) {
			const seen = new Set(openedRowsOnce);
			seen.add(key);
			openedRowsOnce = seen;
		}
		if (collapseStore) {
			collapseStore.toggle(key);
			return;
		}
		const next = new Set(expandedRowsLocal);
		if (next.has(key)) next.delete(key);
		else next.add(key);
		expandedRowsLocal = next;
	}

	function hasRowOpened(key: string): boolean {
		return openedRowsOnce.has(key) || isRowExpanded(key);
	}

	let collapsedProjectsLocal = $state<Set<string>>(new Set());

	function projectCollapseKey(project: string): string {
		return `sidebar:project:${project}`;
	}

	function isProjectCollapsed(project: string): boolean {
		const key = projectCollapseKey(project);
		return collapseStore ? collapseStore.isCollapsed(key) : collapsedProjectsLocal.has(key);
	}

	function toggleProjectCollapse(project: string): void {
		const key = projectCollapseKey(project);
		if (collapseStore) {
			collapseStore.toggle(key);
			return;
		}
		const next = new Set(collapsedProjectsLocal);
		if (next.has(key)) next.delete(key);
		else next.add(key);
		collapsedProjectsLocal = next;
	}

	const UNGROUPED_KEY = '__ungrouped__';

	const PROJECT_COLOR_CLASSES = [
		'tpg-chip--1',
		'tpg-chip--2',
		'tpg-chip--3',
		'tpg-chip--4',
		'tpg-chip--5'
	];

	interface ToolBucket {
		label: string;
		path: string;
		isUngrouped: boolean;
		rows: ToolUsesFeedRow[];
	}

	const projectGroups = $derived.by((): ToolProjectGroupRow[] => {
		if (rows.length === 0) return [];
		const order: string[] = [];
		const buckets = new Map<string, ToolBucket>();
		for (const row of rows) {
			const isUngrouped = row.project === null;
			const bucketKey = isUngrouped ? UNGROUPED_KEY : row.project!.toLowerCase();
			let bucket = buckets.get(bucketKey);
			if (!bucket) {
				bucket = {
					label: isUngrouped ? 'Ungrouped' : row.project!,
					path: isUngrouped ? '' : row.project!,
					isUngrouped,
					rows: []
				};
				buckets.set(bucketKey, bucket);
				order.push(bucketKey);
			}
			bucket.rows.push(row);
		}
		const named = order.filter((k) => k !== UNGROUPED_KEY);
		const finalOrder = buckets.has(UNGROUPED_KEY) ? [...named, UNGROUPED_KEY] : named;
		let colorIdx = 0;
		return finalOrder.map((bucketKey) => {
			const bucket = buckets.get(bucketKey)!;
			const segs = bucket.isUngrouped ? [] : bucket.label.split('/').filter(Boolean);
			const name = bucket.isUngrouped ? 'Ungrouped' : (segs[segs.length - 1] ?? bucket.label);
			const colorClass = bucket.isUngrouped
				? 'tpg-chip--muted'
				: PROJECT_COLOR_CLASSES[colorIdx++ % PROJECT_COLOR_CLASSES.length]!;
			return {
				key: bucketKey,
				project: bucket.label,
				isUngrouped: bucket.isUngrouped,
				name,
				path: bucket.path,
				rows: bucket.rows,
				failedCount: bucket.rows.filter((r) => r.is_error === true).length,
				colorClass
			};
		});
	});

	let facets = $state<ToolUseFacets>({ projects: [], tool_names: [] });

	let projectFilter = $state('');
	let appliedFilterActive = $state(false);
	let toolNameFilter = $state('');
	let timeRangeValue = $state('');
	let appliedProjectFilter = $state('');
	let appliedToolNameFilter = $state('');
	let appliedTimeRangeValue = $state('');
	let errorsOnly = $state(page.url.hash === '#errors');
	$effect(() => {
		const wantHash = errorsOnly ? '#errors' : '';
		untrack(() => {
			const url = new URL(page.url.href);
			if (url.hash === wantHash) return;
			url.hash = wantHash;
			const target = `${url.pathname}${url.search}${url.hash}`;
			// eslint-disable-next-line svelte/no-navigation-without-resolve -- rewriting the current page's own pathname/query with only the #errors fragment toggled; the combined search+hash shape isn't expressible as a single resolve() route-id cast (RouteIdWithSearchOrHash has no "?search#hash" member)
			void goto(target, { replace: true, reset: false });
		});
	});

	const RANGE_OPTIONS: Array<{ label: string; value: string; ms: number | null }> = [
		{ label: 'Any time', value: '', ms: null },
		{ label: 'Last hour', value: '1h', ms: 60 * 60 * 1000 },
		{ label: 'Last 24 hours', value: '24h', ms: 24 * 60 * 60 * 1000 },
		{ label: 'Last 7 days', value: '7d', ms: 7 * 24 * 60 * 60 * 1000 },
		{ label: 'Last 30 days', value: '30d', ms: 30 * 24 * 60 * 60 * 1000 }
	];

	let now = $state(Date.now());
	let tickTimer: ReturnType<typeof setInterval> | null = null;

	function sinceFromRange(): string | undefined {
		const opt = RANGE_OPTIONS.find((o) => o.value === timeRangeValue);
		if (!opt || opt.ms === null) return undefined;
		return new Date(Date.now() - opt.ms).toISOString();
	}

	function currentParams(cursor: string | null) {
		const params: {
			limit: number;
			cursor?: string;
			project?: string;
			tool_name?: string;
			since?: string;
			errors_only?: boolean;
		} = { limit: PAGE_SIZE };
		if (cursor !== null) params.cursor = cursor;
		if (projectFilter) params.project = projectFilter;
		if (toolNameFilter) params.tool_name = toolNameFilter;
		const since = sinceFromRange();
		if (since !== undefined) params.since = since;
		if (errorsOnly) params.errors_only = true;
		return params;
	}

	async function load(): Promise<void> {
		loading = true;
		errorMessage = null;
		loadMoreError = null;
		try {
			const params = currentParams(null);
			appliedFilterActive = Boolean(
				params.project || params.tool_name || params.since || params.errors_only
			);
			appliedProjectFilter = projectFilter;
			appliedToolNameFilter = toolNameFilter;
			appliedTimeRangeValue = timeRangeValue;
			const envelope = await listToolUses(params);
			rows = envelope.rows;
			totalCount = envelope.total_count;
			totalCapped = envelope.total_count_capped ?? false;
			nextCursor = envelope.next_cursor ?? null;
		} catch (err) {
			errorMessage = err instanceof ApiError ? err.message : String(err);
			rows = [];
			totalCount = 0;
			totalCapped = false;
			nextCursor = null;
		} finally {
			loading = false;
		}
	}

	async function loadMore(): Promise<void> {
		if (loadingMore || nextCursor === null) return;
		loadingMore = true;
		loadMoreError = null;
		try {
			const envelope = await listToolUses(currentParams(nextCursor));
			rows = [...rows, ...envelope.rows];
			totalCount = envelope.total_count;
			totalCapped = envelope.total_count_capped ?? false;
			nextCursor = envelope.next_cursor ?? null;
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
		toolNameFilter = '';
		timeRangeValue = '';
		errorsOnly = false;
		void load();
	}

	function toggleErrorsOnly(event: Event): void {
		errorsOnly = (event.currentTarget as HTMLInputElement).checked;
		void load();
	}

	function shortProject(p: string | null): string {
		if (!p) return '—';
		const segs = p.split('/').filter(Boolean);
		return segs.length > 0 ? (segs[segs.length - 1] ?? p) : p;
	}

	const remaining = $derived(Math.max(0, totalCount - rows.length));
	const knownTotal = $derived(Math.max(totalCount, rows.length));

	const filtersDirty = $derived(
		projectFilter !== appliedProjectFilter ||
			toolNameFilter !== appliedToolNameFilter ||
			timeRangeValue !== appliedTimeRangeValue
	);

	function rangeLabel(value: string): string {
		return RANGE_OPTIONS.find((opt) => opt.value === value)?.label ?? 'Any time';
	}

	const appliedScopeLabel = $derived.by((): string => {
		const parts: string[] = [];
		if (appliedProjectFilter) parts.push(`project "${shortProject(appliedProjectFilter)}"`);
		if (appliedToolNameFilter) parts.push(`tool "${appliedToolNameFilter}"`);
		if (appliedTimeRangeValue) parts.push(rangeLabel(appliedTimeRangeValue).toLowerCase());
		if (errorsOnly) parts.push('errors only');
		return parts.length > 0 ? parts.join(', ') : 'every captured tool call';
	});

	onMount(() => {
		const urlToolName = page.url.searchParams.get('tool_name');
		const urlProject = page.url.searchParams.get('project');
		if (urlToolName) toolNameFilter = urlToolName;
		if (urlProject) projectFilter = urlProject;

		void load();
		listToolUseFacets()
			.then((f) => {
				facets = f;
			})
			.catch(() => {});
		tickTimer = setInterval(() => {
			now = Date.now();
		}, 1000);
	});

	onDestroy(() => {
		if (tickTimer !== null) clearInterval(tickTimer);
		tickTimer = null;
	});
</script>

<svelte:head>
	<title>Tools — mike</title>
</svelte:head>

<section class="tu-page" use:revealSections>
	<header class="tu-page__head" data-reveal>
		<PageIntro title="Tools" level="h1" learnMoreHref="/help/guides/inspecting-requests">
			<span class="flex items-center gap-1.5">
				<span>Cross-session feed of every captured</span>
				<span class="inline-flex items-center gap-1">
					<span>tool call</span>
					<HelpHint slug="tool-call" />
				</span>
				<span
					>, grouped by project. A failed call carries a FAILED badge; expand any row for the full
					input JSON.</span
				>
			</span>
		</PageIntro>
	</header>

	<form
		class="tu-filters"
		data-reveal
		data-augmented-ui="tl-clip br-clip border"
		onsubmit={applyFilters}
	>
		<div class="tu-filter-guide">
			<span>Scope your investigation</span>
			<p>
				Choose where to look. Expand a call to read its full input and jump straight to the source
				request.
			</p>
		</div>
		<label class="tu-filter">
			<span class="tu-filter__label">Project</span>
			<select bind:value={projectFilter} class="tu-filter__select">
				<option value="">All projects</option>
				{#each facets.projects as proj (proj)}
					<option value={proj} title={proj}>{shortProject(proj)}</option>
				{/each}
			</select>
		</label>
		<label class="tu-filter">
			<span class="tu-filter__label">Tool name</span>
			<select bind:value={toolNameFilter} class="tu-filter__select">
				<option value="">All tools</option>
				{#each facets.tool_names as name (name)}
					<option value={name}>{name}</option>
				{/each}
			</select>
		</label>
		<label class="tu-filter">
			<span class="tu-filter__label">Time range</span>
			<select bind:value={timeRangeValue} class="tu-filter__select">
				{#each RANGE_OPTIONS as opt (opt.value)}
					<option value={opt.value}>{opt.label}</option>
				{/each}
			</select>
		</label>
		<label class="tu-filter tu-filter--toggle">
			<input type="checkbox" class="tu-checkbox" checked={errorsOnly} onchange={toggleErrorsOnly} />
			<span class="tu-filter__toggle-label">Errors only</span>
		</label>
		<div class="tu-filter__actions">
			<Button type="submit" disabled={loading}>Apply</Button>
			<Button variant="outline" onclick={clearFilters} disabled={loading}>Clear</Button>
		</div>
		<FilterScopeStatus
			scopeLabel={appliedScopeLabel}
			shownCount={errorMessage ? undefined : rows.length}
			totalCount={errorMessage || totalCapped ? undefined : totalCount}
			itemNoun="tool use"
			dirty={filtersDirty}
		/>
	</form>

	{#if errorMessage}
		<div class="tu-status tu-status--error" role="alert">
			<span>Couldn't load tool uses: {errorMessage}</span>
			<Button variant="outline" onclick={load} disabled={loading}
				>{loading ? 'Retrying' : 'Retry'}</Button
			>
		</div>
	{:else if loading && rows.length === 0}
		<div class="tu-status" role="status" aria-live="polite">Loading…</div>
	{:else if rows.length === 0}
		<FilteredEmptyState
			icon={Wrench}
			filtered={appliedFilterActive}
			emptyText="No tool uses recorded"
			emptySubtext="Tool calls appear here once sessions make their first tool use."
			filteredText="No calls match these filters"
			filteredSubtext="Try another project or tool, widen the time range, or clear your filters to see every captured call."
			onClearFilters={clearFilters}
		/>
	{:else}
		<div class="tu-groups">
			{#each projectGroups as group (group.key)}
				<ToolProjectGroup
					{group}
					{now}
					collapsed={isProjectCollapsed(group.project)}
					onToggleCollapse={() => toggleProjectCollapse(group.project)}
					{isRowExpanded}
					onToggleRow={toggleRow}
					{hasRowOpened}
				/>
			{/each}
		</div>
		{#if nextCursor !== null}
			<div class="tu-footer" data-augmented-ui="tl-clip br-clip border">
				<span class="tu-footer__count" role="status">
					{#if totalCapped}
						Showing {rows.length} of {knownTotal.toLocaleString()}+; more not loaded.
					{:else}
						Showing {rows.length} of {totalCount.toLocaleString()}; {remaining.toLocaleString()} more
						not loaded.
					{/if}
				</span>
				<span class="tu-footer__load-more-backing">
					<button
						type="button"
						class="tu-footer__load-more"
						data-augmented-ui="tr-clip bl-clip border"
						disabled={loadingMore}
						onclick={loadMore}
					>
						{loadingMore ? 'Loading…' : 'Load more tool uses'}
					</button>
				</span>
				{#if loadMoreError}
					<span class="tu-footer__error" role="alert">{loadMoreError}</span>
				{/if}
			</div>
		{/if}
	{/if}
</section>

<style>
	.tu-page__head {
		min-width: 0;
	}
	.tu-filter-guide {
		flex-basis: 100%;
		min-width: 0;
		padding-bottom: var(--space-3);
		border-bottom: var(--hairline) solid var(--border-subtle);
	}
	.tu-filter-guide > span {
		font: var(--weight-display) var(--text-md) var(--font-display);
		color: var(--text-1);
	}
	.tu-filter-guide p {
		margin: var(--space-1) 0 0;
		max-width: 80ch;
		color: var(--text-2);
		font-size: var(--text-sm);
		line-height: var(--leading-body);
	}
	.tu-page {
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 16px;
		width: 100%;
		max-width: 1200px;
		margin: 0 auto;
		flex: 1 1 auto;
		min-width: 0;
		min-height: 0;
		overflow-y: auto;
	}
	.tu-filters {
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
		min-width: 0;
		padding: calc(var(--edge-tl) + 4px) calc(var(--edge-br) + 4px) calc(var(--edge-br) + 4px)
			calc(var(--edge-tl) + 4px);
		background: var(--surface-card);
	}
	.tu-filter {
		display: flex;
		flex-direction: column;
		gap: 4px;
		min-width: 180px;
		flex: 1 1 180px;
	}
	.tu-filter__label {
		font-size: var(--text-label);
		color: var(--text-3);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	.tu-filter__select {
		padding: 6px 8px;
		border: 1px solid var(--border-default);
		background: var(--surface-bg);
		color: var(--text-1);
		font-size: 12px;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		appearance: none;
		background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6' viewBox='0 0 10 6'%3E%3Cpath d='M1 1l4 4 4-4' stroke='%23adb5be' stroke-width='1.5' fill='none' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E");
		background-repeat: no-repeat;
		background-position: right 8px center;
		padding-right: 28px;
		cursor: pointer;
	}
	.tu-filter__select:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}
	.tu-filter--toggle {
		flex-direction: row;
		align-items: center;
		gap: 6px;
		min-width: 0;
		flex: 0 0 auto;
		padding-bottom: 7px;
	}
	.tu-checkbox {
		width: 14px;
		height: 14px;
		accent-color: var(--interactive-active-fg);
		cursor: pointer;
	}
	.tu-filter__toggle-label {
		font-size: var(--text-label);
		color: var(--text-2);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		cursor: pointer;
	}
	.tu-filter__actions {
		display: flex;
		gap: 8px;
	}
	.tu-status {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 16px 18px;
		font-size: 12px;
		color: var(--text-3);
	}
	.tu-status--error {
		color: var(--status-danger-fg);
	}
	.tu-groups {
		display: flex;
		flex-direction: column;
		min-width: 0;
		gap: 16px;
		--tpg-chip-1: var(--agent-1-fg);
		--tpg-chip-2: var(--agent-2-fg);
		--tpg-chip-3: var(--agent-3-fg);
		--tpg-chip-4: var(--agent-4-fg);
		--tpg-chip-5: var(--agent-5-fg);
	}
	.tu-footer {
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
	.tu-footer__count {
		font-size: var(--text-caption);
		color: var(--text-3);
		text-align: center;
	}
	.tu-footer__load-more-backing {
		display: inline-flex;
		align-self: center;
		background: var(--surface-card);
	}
	.tu-footer__load-more {
		--edge: 5px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 6px 12px;
		background: var(--surface-card);
		color: var(--text-2);
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		cursor: pointer;
	}
	.tu-footer__load-more:hover {
		--aug-border-bg: var(--interactive-active-fg);
		color: var(--text-strong);
	}
	.tu-footer__load-more:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}
	.tu-footer__load-more:disabled {
		cursor: not-allowed;
		opacity: 0.6;
	}
	.tu-footer__error {
		font-size: var(--text-caption);
		color: var(--status-danger-fg);
		text-align: center;
	}
</style>
