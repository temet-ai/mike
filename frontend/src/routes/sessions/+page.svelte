<script lang="ts">
	import { getContext, onMount, onDestroy, untrack } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { SearchX } from '@lucide/svelte';
	import { revealSections } from '$lib/actions/reveal';
	import { discloseHeight } from '$lib/components/ui/motion';
	import { SESSION_LIST_STORE_KEY, COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { SessionListStore } from '$lib/stores/sessions.svelte';
	import {
		SESSION_SEARCH_MIN_QUERY,
		SESSION_SEARCH_DEBOUNCE_MS,
		looksLikeSessionId
	} from '$lib/stores/sessions.svelte';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import { groupByProject, compareByFirstSeenDesc } from '$lib/grouping';
	import { shellState } from '$lib/stores/shellState.svelte';
	import type { SessionSummary, UnifiedTextMatch, SearchMatchSnippet } from '$lib/types';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import SearchField from '$lib/components/shared/SearchField.svelte';
	import OverviewBand from '$lib/components/shared/OverviewBand.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import EmptyStateOnboarding from '$lib/components/shared/EmptyStateOnboarding.svelte';
	import FilteredEmptyState from '$lib/components/shared/FilteredEmptyState.svelte';
	import SessionTableHead from '$lib/components/sessions/SessionTableHead.svelte';
	import SessionRow from '$lib/components/sessions/SessionRow.svelte';
	import TimeSpentPanel from '$lib/components/sessions/TimeSpentPanel.svelte';
	import { createDetailMetricsLoader } from '$lib/components/sessions/detailMetrics.svelte';
	import { listProjects } from '$lib/api';
	import ProjectSessionGroup, {
		type ProjectGroupRow
	} from '$lib/components/sessions/ProjectSessionGroup.svelte';

	const sessionStore = getContext<SessionListStore>(SESSION_LIST_STORE_KEY);
	const collapseStore = getContext<CollapseStore>(COLLAPSE_STORE_KEY);

	function openHelp(): void {
		shellState.helpDrawerOpen = true;
	}

	let now = $state(Date.now());
	onMount(() => {
		const id = setInterval(() => {
			now = Date.now();
		}, 1_000);
		return () => clearInterval(id);
	});

	const sessions = $derived(sessionStore?.sessions ?? []);
	const loading = $derived(sessionStore?.loading ?? false);
	const error = $derived(sessionStore?.error ?? null);

	let filterQuery = $state(page.url.searchParams.get('q') ?? '');
	const trimmedQuery = $derived(filterQuery.trim());
	const isSearchMode = $derived(trimmedQuery.length >= SESSION_SEARCH_MIN_QUERY);
	const idShaped = $derived(looksLikeSessionId(trimmedQuery));

	let urlSyncTimer: ReturnType<typeof setTimeout> | null = null;

	$effect(() => {
		const q = filterQuery;
		untrack(() => sessionStore?.setSearch(q));

		if (urlSyncTimer !== null) clearTimeout(urlSyncTimer);
		urlSyncTimer = setTimeout(() => {
			urlSyncTimer = null;
			const trimmed = q.trim();
			const url = new URL(page.url.href);
			if (trimmed.length > 0) {
				url.searchParams.set('q', trimmed);
			} else {
				url.searchParams.delete('q');
			}
			const qs = url.searchParams.toString();
			const next = resolve(qs.length > 0 ? `/sessions?${qs}` : '/sessions');
			if (next === `${page.url.pathname}${page.url.search}`) return;
			void goto(next, { replace: true, reset: false });
		}, SESSION_SEARCH_DEBOUNCE_MS);
	});

	onDestroy(() => {
		if (urlSyncTimer !== null) clearTimeout(urlSyncTimer);
	});

	const searchHits = $derived(sessionStore?.searchHits ?? []);
	const searchLoading = $derived(sessionStore?.searchLoading ?? false);
	const searchError = $derived(sessionStore?.searchError ?? null);
	const bodyHits = $derived(sessionStore?.bodyHits ?? []);
	const bodySearchLoading = $derived(sessionStore?.bodySearchLoading ?? false);
	const bodySearchError = $derived(sessionStore?.bodySearchError ?? null);
	const bodyFullMatchCount = $derived(sessionStore?.bodyFullMatchCount ?? 0);
	const bodyMatchesTruncated = $derived(sessionStore?.bodyMatchesTruncated ?? false);
	const bodyWindowNote = $derived(sessionStore?.bodyWindowNote ?? null);
	const contentHits = $derived(sessionStore?.contentHits ?? []);
	const contentSearchLoading = $derived(sessionStore?.contentSearchLoading ?? false);
	const contentSearchError = $derived(sessionStore?.contentSearchError ?? null);
	const contentTotalSessions = $derived(sessionStore?.contentTotalSessions ?? 0);
	const contentHasMore = $derived(sessionStore?.contentHasMore ?? false);

	function toBodySnippet(m: UnifiedTextMatch): SearchMatchSnippet {
		return {
			request_id: m.request_id,
			line: m.line,
			column: m.column,
			before: m.before,
			matched: m.matched,
			after: m.after,
			provenance: m.source,
			provenanceTitle: 'Matched in the captured response body'
		};
	}

	function toContentSnippet(m: {
		request_id: number;
		role: 'prompt' | 'assistant';
		line: number;
		column: number;
		before: string;
		matched: string;
		after: string;
	}): SearchMatchSnippet {
		return {
			request_id: m.request_id,
			line: m.line,
			column: m.column,
			before: m.before,
			matched: m.matched,
			after: m.after,
			provenance: m.role,
			provenanceTitle:
				m.role === 'prompt' ? 'Matched in the operator prompt' : 'Matched in the assistant reply'
		};
	}

	interface BodySearchSessionGroup {
		sessionId: string;
		matches: SearchMatchSnippet[];
		matchCount: number;
		requestCount: number;
	}
	const bodyHitGroups = $derived.by((): BodySearchSessionGroup[] => {
		const order: string[] = [];
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- local grouping map inside $derived.by; built once and returned, never mutated after recomputation
		const bySession = new Map<string, UnifiedTextMatch[]>();
		for (const m of bodyHits) {
			let group = bySession.get(m.session_id);
			if (!group) {
				group = [];
				bySession.set(m.session_id, group);
				order.push(m.session_id);
			}
			group.push(m);
		}
		return order
			.map((sessionId) => {
				const raw = bySession.get(sessionId)!;
				return {
					sessionId,
					matches: raw.map(toBodySnippet),
					matchCount: raw.length,
					requestCount: new Set(raw.map((m) => m.request_id)).size
				};
			})
			.sort((a, b) => b.matchCount - a.matchCount);
	});

	interface ContentSearchGroupView {
		sessionId: string;
		matches: SearchMatchSnippet[];
		matchCount: number;
		turnCount: number;
		truncated: boolean;
	}
	const contentGroupViews = $derived.by((): ContentSearchGroupView[] =>
		contentHits.map((g) => ({
			sessionId: g.session_id,
			matches: g.matches.map(toContentSnippet),
			matchCount: g.match_count,
			turnCount: g.turn_count,
			truncated: g.matches_truncated
		}))
	);

	const hydratingSessions = $derived(sessionStore?.hydratingSessions ?? false);
	const hydrationError = $derived(sessionStore?.hydrationError ?? null);
	const anySearchLoading = $derived(
		searchLoading || bodySearchLoading || contentSearchLoading || hydratingSessions
	);

	function clearQuery(): void {
		filterQuery = '';
	}

	interface SearchResultRow {
		session: SessionSummary;
		titleHit: boolean;
		conversationCount: number;
		conversationTurnCount: number;
		conversationTruncated: boolean;
		conversationSnippets: SearchMatchSnippet[];
		bodyCount: number;
		bodySnippets: SearchMatchSnippet[];
		totalMatchCount: number;
	}

	const matchedSessionIds = $derived.by((): Set<string> => {
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- local set inside $derived.by; built once and returned, never mutated after recomputation
		const ids = new Set<string>();
		for (const s of searchHits) ids.add(s.id);
		for (const g of contentGroupViews) ids.add(g.sessionId);
		for (const g of bodyHitGroups) ids.add(g.sessionId);
		return ids;
	});

	const searchResultRows = $derived.by((): SearchResultRow[] => {
		if (matchedSessionIds.size === 0) return [];
		const titleIds = new Set(searchHits.map((s) => s.id));
		const contentById = new Map(contentGroupViews.map((g) => [g.sessionId, g]));
		const bodyById = new Map(bodyHitGroups.map((g) => [g.sessionId, g]));
		const rows: SearchResultRow[] = [];
		for (const id of matchedSessionIds) {
			const session = sessionStore?.searchSessionById(id) ?? null;
			if (!session) continue;
			const c = contentById.get(id);
			const b = bodyById.get(id);
			const conversationCount = c?.matchCount ?? 0;
			const bodyCount = b?.matchCount ?? 0;
			rows.push({
				session,
				titleHit: titleIds.has(id),
				conversationCount,
				conversationTurnCount: c?.turnCount ?? 0,
				conversationTruncated: c?.truncated ?? false,
				conversationSnippets: c?.matches ?? [],
				bodyCount,
				bodySnippets: b?.matches ?? [],
				totalMatchCount: conversationCount + bodyCount
			});
		}
		return rows;
	});

	interface SearchProjectGroupRow {
		key: string;
		project: string;
		isUngrouped: boolean;
		name: string;
		path: string;
		rows: SearchResultRow[];
		colorClass: string;
	}

	const searchProjectRows = $derived.by((): SearchProjectGroupRow[] => {
		if (searchResultRows.length === 0) return [];
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- local grouping map inside $derived.by; built once and returned, never mutated after recomputation
		const buckets = new Map<string, { label: string; rows: SearchResultRow[] }>();
		for (const row of searchResultRows) {
			const isNull = row.session.project === null;
			const key = isNull ? UNGROUPED_KEY : row.session.project!.toLowerCase();
			const label = isNull ? 'Ungrouped' : row.session.project!;
			const bucket = buckets.get(key);
			if (bucket) {
				bucket.rows.push(row);
			} else {
				buckets.set(key, { label, rows: [row] });
			}
		}
		const entries = Array.from(buckets.entries()).map(([key, bucket]) => {
			const sortedRows = [...bucket.rows].sort((a, b) => {
				if (b.totalMatchCount !== a.totalMatchCount) return b.totalMatchCount - a.totalMatchCount;
				return compareByFirstSeenDesc(a.session, b.session);
			});
			const bestMatchCount = sortedRows[0]?.totalMatchCount ?? 0;
			return { key, label: bucket.label, rows: sortedRows, bestMatchCount };
		});
		entries.sort((a, b) => {
			if (a.key === UNGROUPED_KEY) return 1;
			if (b.key === UNGROUPED_KEY) return -1;
			return b.bestMatchCount - a.bestMatchCount;
		});
		return entries.map((entry, idx) => {
			const isUngrouped = entry.key === UNGROUPED_KEY;
			const segs = isUngrouped ? [] : entry.label.split('/').filter(Boolean);
			const name = isUngrouped ? 'Ungrouped' : (segs[segs.length - 1] ?? entry.label);
			return {
				key: entry.key,
				project: entry.label,
				isUngrouped,
				name,
				path: isUngrouped ? '' : entry.label,
				rows: entry.rows,
				colorClass: isUngrouped
					? 'ssn-chip--muted'
					: PROJECT_COLOR_CLASSES[idx % PROJECT_COLOR_CLASSES.length]!
			};
		});
	});

	const noMatchesFound = $derived(
		matchedSessionIds.size === 0 &&
			!anySearchLoading &&
			!searchError &&
			!bodySearchError &&
			!contentSearchError
	);

	let matchExpandedIds = $state<Set<string>>(new Set());
	function toggleMatchExpand(id: string): void {
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- cloned, mutated, then reassigned wholesale; reactivity comes from the reassignment below, not in-place mutation
		const next = new Set(matchExpandedIds);
		if (next.has(id)) {
			next.delete(id);
		} else {
			next.add(id);
		}
		matchExpandedIds = next;
	}

	const UNGROUPED_KEY = '__ungrouped__';

	const PROJECT_COLOR_CLASSES = [
		'ssn-chip--1',
		'ssn-chip--2',
		'ssn-chip--3',
		'ssn-chip--4',
		'ssn-chip--5'
	];

	const projectRows = $derived.by((): ProjectGroupRow[] => {
		const source = sessions.filter((s) => s.request_count > 0);
		if (source.length === 0) return [];

		return groupByProject(source).map((group, idx) => {
			const isUngrouped = group.sessions[0]?.project === null;
			const segs = isUngrouped ? [] : group.project.split('/').filter(Boolean);
			const name = isUngrouped ? 'Ungrouped' : (segs[segs.length - 1] ?? group.project);
			return {
				key: isUngrouped ? UNGROUPED_KEY : group.project.toLowerCase(),
				project: group.project,
				isUngrouped,
				name,
				path: isUngrouped ? '' : group.project,
				sessions: group.sessions,
				colorClass: isUngrouped
					? 'ssn-chip--muted'
					: PROJECT_COLOR_CLASSES[idx % PROJECT_COLOR_CLASSES.length]!
			};
		});
	});

	function collapseKey(project: string): string {
		return `sidebar:project:${project}`;
	}
	function isProjectCollapsed(project: string): boolean {
		return collapseStore?.isCollapsed(collapseKey(project)) ?? false;
	}
	function toggleProject(project: string): void {
		collapseStore?.toggle(collapseKey(project));
	}

	const SHOW_MORE_INITIAL = 10;
	let expandedProjects = $state<Record<string, boolean>>({});
	function toggleShowMore(key: string): void {
		expandedProjects = { ...expandedProjects, [key]: !expandedProjects[key] };
	}

	function togglePin(session: SessionSummary): void {
		void sessionStore?.togglePin(session.id, !(session.pinned === true));
	}
	function toggleProjectPin(project: ProjectGroupRow): void {
		const allPinned = project.sessions.every((s) => s.pinned === true);
		void sessionStore?.toggleProjectPin(project.project, !allPinned);
	}

	let timeSpentProjects = $state<{ value: string; label: string }[]>([]);
	onMount(() => {
		void listProjects()
			.then((rows) => {
				timeSpentProjects = rows.map((p) => ({
					value: p.project_path,
					label: p.custom_name ?? p.project_path.split('/').filter(Boolean).pop() ?? p.project_path
				}));
			})
			.catch(() => {
				timeSpentProjects = [];
			});
	});

	const detailMetricsLoader = createDetailMetricsLoader();
	const detailMetrics = $derived(detailMetricsLoader.metrics);
	const scheduleDetailLoad = detailMetricsLoader.scheduleLoad;
	const cancelDetailLoad = detailMetricsLoader.cancelLoad;

	onDestroy(() => {
		detailMetricsLoader.dispose();
	});
</script>

<svelte:head>
	<title>Sessions — mike</title>
</svelte:head>

<section class="col detail" aria-label="Sessions" use:revealSections>
	<div class="col-body ssn-body">
		<div class="ssn-head-row" data-reveal>
			<PageIntro
				title="Sessions"
				level="h1"
				description="Pick a project, open a session, follow the conversation."
			>
				{#snippet actions()}
					<div class="ssn-toolbar" data-slot="sessions-toolbar">
						<SearchField
							bind:value={filterQuery}
							label="Search sessions or content"
							placeholder="Search sessions or content"
						/>
						{#if isSearchMode}
							<Tooltip
								text="Matches session title/id, conversation (operator prompts + assistant replies), and raw response bodies (tool results, thinking blocks, everything else captured)."
							>
								<span class="ssn-search-scope" data-motion="hover"
									>titles · conversation · bodies</span
								>
							</Tooltip>
						{/if}
					</div>
				{/snippet}
			</PageIntro>
		</div>
		<div data-reveal>
			{#if isSearchMode}
				<OverviewBand
					label="Search result scope"
					facts={[
						{ label: 'Sessions matched', value: matchedSessionIds.size },
						{ label: 'Projects matched', value: searchProjectRows.length }
					]}
				>
					Matching "{trimmedQuery}" against titles, conversation and response bodies. Clear the
					search to browse every session.
				</OverviewBand>
			{:else}
				<OverviewBand
					label="Loaded session overview"
					facts={[
						{ label: 'Sessions loaded', value: sessions.length },
						{ label: 'Project groups', value: projectRows.length }
					]}
				>
					Search a title, session ID, or anything said in a conversation. Expand a project to
					explore its history.
				</OverviewBand>
			{/if}
		</div>

		{#if !isSearchMode}
			<div class="ssn-time" data-reveal>
				<TimeSpentPanel scope="list" projects={timeSpentProjects} />
			</div>
		{/if}

		<div class="ssn-content">
			{#if error}
				<p class="ssn-error" role="alert">{error}</p>
			{/if}

			{#if isSearchMode}
				{#if noMatchesFound}
					<div class="ssn-search-results">
						<FilteredEmptyState
							icon={SearchX}
							filtered={true}
							emptyText="No sessions found"
							filteredText="No sessions or content match the search."
							filteredSubtext={!idShaped && bodyWindowNote
								? 'Searched response content from the last 7 days — older content is outside the default window.'
								: undefined}
							onClearFilters={clearQuery}
							clearLabel="Clear search"
						/>
					</div>
				{:else}
					<div class="ssn-groups">
						{#if searchError}
							<p class="ssn-error" role="alert">Session search failed: {searchError}</p>
						{/if}
						{#if !idShaped && contentSearchError}
							<p class="ssn-error" role="alert">Conversation search failed: {contentSearchError}</p>
						{/if}
						{#if !idShaped && bodySearchError}
							<p class="ssn-error" role="alert">Raw body search failed: {bodySearchError}</p>
						{/if}
						{#if hydrationError}
							<p class="ssn-error" role="alert">
								Some matching sessions failed to load: {hydrationError}
							</p>
						{/if}

						{#each searchProjectRows as group (group.key)}
							{@render searchProjectGroup(group)}
						{/each}

						{#if anySearchLoading}
							<p class="ssn-state ssn-state--busy">
								<span
									class="ssn-spinner"
									data-motion="wait wait-spin"
									role="status"
									aria-hidden="true"
								></span>
								Searching…
							</p>
						{/if}
						{#if !idShaped && !contentSearchLoading && contentHasMore}
							<p class="ssn-state ssn-state--hint">
								Showing {contentGroupViews.length} of {contentTotalSessions} sessions with conversation
								matches — refine the search to narrow.
							</p>
						{/if}
						{#if !idShaped && !bodySearchLoading && bodyMatchesTruncated}
							<p class="ssn-state ssn-state--hint">
								Showing {bodyHits.length} of {bodyFullMatchCount} raw-body matches — refine the search
								to narrow.
							</p>
						{/if}
						{#if !idShaped && !bodySearchLoading && !bodySearchError && bodyWindowNote}
							<p class="ssn-state ssn-state--hint">
								Searched response content from the last 7 days.
							</p>
						{/if}
					</div>
				{/if}
			{:else if trimmedQuery.length > 0}
				<p class="ssn-state ssn-state--hint">
					Type at least {SESSION_SEARCH_MIN_QUERY} characters to search.
				</p>
			{:else if sessions.length === 0 && loading}
				<p class="ssn-state">Loading sessions</p>
			{:else if sessions.length === 0 && !error}
				<div class="ssn-empty" data-slot="sessions-empty">
					<EmptyStateOnboarding
						headline="No sessions yet"
						body="Sessions appear here once Claude Code routes its first request through the proxy."
						confirmation="New sessions will appear here as Claude Code routes traffic through the proxy."
						onOpenHelp={openHelp}
					/>
				</div>
			{:else}
				<div class="ssn-groups">
					{#each projectRows as group (group.key)}
						<ProjectSessionGroup
							{group}
							{now}
							collapsed={isProjectCollapsed(group.project)}
							onToggleCollapse={() => toggleProject(group.project)}
							expanded={expandedProjects[group.key] ?? false}
							onToggleShowMore={() => toggleShowMore(group.key)}
							showMoreInitial={SHOW_MORE_INITIAL}
							{detailMetrics}
							onRowHoverIntent={scheduleDetailLoad}
							onRowHoverCancel={cancelDetailLoad}
							latestWsEventForSession={(id) => sessionStore?.latestWsEventForSession(id) ?? null}
							onTogglePin={togglePin}
							onToggleProjectPin={() => toggleProjectPin(group)}
							isProjectExhausted={sessionStore?.isProjectExhausted(group.project) ?? false}
							isProjectLoadingMore={sessionStore?.isProjectLoadingMore(group.project) ?? false}
							projectLoadMoreError={sessionStore?.projectLoadMoreError(group.project) ?? null}
							onLoadMoreForProject={() => sessionStore?.loadMoreForProject(group.project)}
						/>
					{/each}
				</div>
			{/if}
		</div>
	</div>
</section>

{#snippet searchProjectGroup(group: SearchProjectGroupRow)}
	{@const collapsed = isProjectCollapsed(group.project)}
	<div class="ssn-project" data-reveal>
		<div
			class="ssn-project-head"
			data-augmented-ui="tl-clip border"
			data-motion="disclose-cut"
			data-collapsed={collapsed}
		>
			<span class="ssn-chip {group.colorClass}" aria-hidden="true"></span>
			{#if !group.isUngrouped && group.path}
				<a
					class="ssn-project-name ssn-project-name--link"
					href={resolve(`projects/${encodeURIComponent(group.path)}`)}
				>
					{group.name}
				</a>
			{:else}
				<span class="ssn-project-name">{group.name}</span>
			{/if}
			<button
				type="button"
				class="ssn-project-toggle"
				aria-expanded={!collapsed}
				aria-controls="ssn-search-project-body-{group.key}"
				aria-label="{collapsed ? 'Expand' : 'Collapse'} {group.name}"
				onclick={() => toggleProject(group.project)}
			>
				{#if group.path}
					<span class="ssn-project-path">{group.path}</span>
				{/if}
				<span class="ssn-project-fill"></span>
				<span class="ssn-project-meta"
					>{group.rows.length} match{group.rows.length === 1 ? '' : 'es'}</span
				>
			</button>
		</div>
		{#if !collapsed}
			<div
				class="ssn-table"
				id="ssn-search-project-body-{group.key}"
				data-augmented-ui="bl-clip br-clip border"
				transition:discloseHeight
			>
				<div class="ssn-thead-wrap">
					<SessionTableHead />
				</div>
				<div class="ssn-tbody">
					{#each group.rows as row (row.session.id)}
						<SessionRow
							session={row.session}
							{now}
							detail={detailMetrics.get(row.session.id)}
							latestWsEventForSession={(id) => sessionStore?.latestWsEventForSession(id) ?? null}
							onHoverIntent={scheduleDetailLoad}
							onHoverCancel={cancelDetailLoad}
							onTogglePin={togglePin}
							matchInfo={row}
							matchExpanded={matchExpandedIds.has(row.session.id)}
							onToggleMatchExpand={toggleMatchExpand}
						/>
					{/each}
				</div>
			</div>
		{/if}
	</div>
{/snippet}

<style>
	.ssn-body {
		display: flex;
		flex-direction: column;
		padding: 0;
		background: var(--surface-bg);
		--ssn-chip-1: var(--agent-1-fg);
		--ssn-chip-2: var(--agent-2-fg);
		--ssn-chip-3: var(--agent-3-fg);
		--ssn-chip-4: var(--agent-4-fg);
		--ssn-chip-5: var(--agent-5-fg);
	}

	.ssn-head-row {
		padding: var(--space-5) var(--space-5) var(--space-4);
		flex-shrink: 0;
	}
	.ssn-toolbar {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		flex-wrap: wrap;
		min-width: 0;
		max-width: 100%;
	}

	.ssn-content {
		flex: 1 1 auto;
		min-height: 0;
	}
	.ssn-error {
		margin: 0;
		padding: 10px 24px;
		font-size: var(--text-label);
		color: var(--status-danger-fg);
		line-height: 1.5;
		border-bottom: 1px solid var(--border-subtle);
	}
	.ssn-state {
		margin: 0;
		padding: 16px;
		font-size: var(--text-label);
		color: var(--text-3);
		line-height: 1.6;
	}
	.ssn-empty {
		padding: 16px;
	}
	.ssn-state--hint {
		padding: 12px 24px;
		font-style: italic;
	}
	.ssn-state--muted {
		padding: 6px 24px;
		color: var(--text-faint);
		font-size: var(--text-caption);
	}
	.ssn-state--busy {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.ssn-spinner {
		display: inline-block;
		width: 12px;
		height: 12px;
		border-radius: 50%;
		border: 1.5px solid var(--border-subtle);
		border-top-color: var(--interactive-active-fg);
		flex-shrink: 0;
	}

	.ssn-search-results {
		display: flex;
		flex-direction: column;
		gap: 20px;
		padding: 16px 24px 32px;
		max-width: 1360px;
	}

	.ssn-search-scope {
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-faint);
		white-space: nowrap;
		text-decoration: underline dotted;
		text-decoration-color: var(--border-strong);
		text-underline-offset: 3px;
	}
	:global(.tooltip-trigger:hover) .ssn-search-scope,
	:global(.tooltip-trigger:focus-visible) .ssn-search-scope {
		color: var(--text-2);
		text-decoration-color: currentColor;
	}

	.ssn-time {
		padding: var(--space-5) var(--space-5) 0;
		max-width: 1360px;
	}
	.ssn-groups {
		display: flex;
		flex-direction: column;
		gap: 0;
		padding: 20px 24px 16px;
		max-width: 1360px;
	}
	.ssn-project-head {
		display: flex;
		align-items: center;
		gap: 10px;
		min-width: 0;
		padding: 3px 4px 5px 18px;
		color: var(--text-1);
		--motion-cut: 14px;
		--aug-tl: var(--motion-cut);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
	}
	.ssn-project-head[data-collapsed='true'] {
		--motion-cut: 4px;
	}
	.ssn-project-toggle {
		display: flex;
		align-items: center;
		gap: 10px;
		flex: 1 1 auto;
		min-width: 0;
		text-align: left;
		background: transparent;
		border: 0;
		padding: 0;
		font: inherit;
		color: inherit;
		cursor: pointer;
	}
	.ssn-project-toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
		border-radius: 2px;
	}
	.ssn-chip {
		width: 7px;
		height: 7px;
		border-radius: 2px;
		flex-shrink: 0;
		background: var(--text-faint);
	}
	.ssn-chip--1 {
		background: var(--ssn-chip-1);
	}
	.ssn-chip--2 {
		background: var(--ssn-chip-2);
	}
	.ssn-chip--3 {
		background: var(--ssn-chip-3);
	}
	.ssn-chip--4 {
		background: var(--ssn-chip-4);
	}
	.ssn-chip--5 {
		background: var(--ssn-chip-5);
	}
	.ssn-chip--muted {
		background: var(--text-faint);
	}
	.ssn-project-name {
		font-size: 13px;
		font-weight: 600;
		color: var(--text-strong);
		letter-spacing: -0.01em;
		white-space: nowrap;
	}
	.ssn-project-name--link {
		text-decoration: none;
		border-radius: 2px;
	}
	.ssn-project-name--link:hover {
		text-decoration: underline;
		text-decoration-color: var(--text-faint);
	}
	.ssn-project-name--link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.ssn-project-path {
		font-size: var(--text-caption);
		color: var(--text-faint);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		min-width: 0;
	}
	.ssn-project-fill {
		flex: 1 1 auto;
	}
	.ssn-project-meta {
		flex-shrink: 0;
		font-size: var(--text-caption);
		color: var(--text-3);
		min-width: 64px;
		text-align: right;
		font-variant-numeric: tabular-nums;
	}

	.ssn-table {
		--edge: 12px;
		--aug-bl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		background: var(--surface-card);
		overflow: hidden;
		box-shadow:
			inset 0 1px 0 var(--surface-shine),
			var(--shadow-sm);
		container-type: inline-size;
		container-name: session-table;
	}
	.ssn-tbody {
		display: flex;
		flex-direction: column;
	}
	@container session-table (max-width: 799.98px) {
		.ssn-thead-wrap {
			display: none;
		}
	}
</style>
