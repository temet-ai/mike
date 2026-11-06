<script lang="ts">
	import type { Snippet } from 'svelte';
	import { Search } from '@lucide/svelte';
	import { Button } from '$lib/components/ui/button';
	import FilterScopeStatus from '$lib/components/shared/FilterScopeStatus.svelte';
	import FilteredEmptyState from '$lib/components/shared/FilteredEmptyState.svelte';
	import SearchField from '$lib/components/shared/SearchField.svelte';
	import ProjectDirectory from '$lib/components/projects/ProjectDirectory.svelte';
	import { isUnknownProjectPath, projectMatchesQuery } from '$lib/fleet/projectTree';
	import type { FleetProjectWithCounts } from '$lib/fleet/types';
	import CommsThreadFilters from './CommsThreadFilters.svelte';
	import CommsThreadRows from './CommsThreadRows.svelte';
	import type { ResolvedFilter } from './commsStore.svelte';
	import type { ThreadSummaryDto, ThreadTagDto } from './types';

	interface Props {
		projects: FleetProjectWithCounts[];
		threadsByPath: Map<string, ThreadSummaryDto[]>;
		tags: ThreadTagDto[];
		selectedThreadUuid: string | null;
		filterTag: string | null;
		filterParticipant: string | null;
		filterResolved: ResolvedFilter;
		projectQuery: string;
		expandedProjectPath: string | null;
		openDuration?: number;
		now: number;
		loading: boolean;
		newThread?: Snippet;
		onSelect: (uuid: string) => void;
		onFilterTag: (tag: string | null) => void;
		onClearParticipant: () => void;
		onFilterResolved: (v: ResolvedFilter) => void;
		onProjectQuery: (query: string) => void;
		onToggleProject: (path: string) => void;
		onOpenSettings: (path: string) => void;
		onTogglePin: (path: string, next: boolean) => void;
		onRefresh: () => void;
	}

	const {
		projects,
		threadsByPath,
		tags,
		selectedThreadUuid,
		filterTag,
		filterParticipant,
		filterResolved,
		projectQuery,
		expandedProjectPath,
		openDuration = 0,
		now,
		loading,
		newThread,
		onSelect,
		onFilterTag,
		onClearParticipant,
		onFilterResolved,
		onProjectQuery,
		onToggleProject,
		onOpenSettings,
		onTogglePin,
		onRefresh
	}: Props = $props();

	const trimmedQuery = $derived(projectQuery.trim());
	const normalizedQuery = $derived(trimmedQuery.toLowerCase());
	const isSearching = $derived(normalizedQuery.length > 0);
	const matchCount = $derived(
		projects.filter((p) => projectMatchesQuery(p, normalizedQuery)).length
	);
	const pinnedCount = $derived(
		projects.filter((p) => p.pinned && !isUnknownProjectPath(p.project_path)).length
	);
	const hasActiveFilter = $derived(
		filterTag !== null || filterParticipant !== null || filterResolved !== 'all'
	);

	function clearSearch(): void {
		onProjectQuery('');
	}

	function clearFilters(): void {
		onFilterTag(null);
		onClearParticipant();
		onFilterResolved('all');
	}
</script>

{#snippet projectThreads(p: FleetProjectWithCounts)}
	<CommsThreadRows
		threads={threadsByPath.get(p.project_path) ?? []}
		{selectedThreadUuid}
		{now}
		{onSelect}
		id="thread-group-body-{p.project_uuid ?? p.project_path}"
	/>
{/snippet}

<div class="project-list">
	<div class="project-list__controls">
		<div class="project-list__search-row">
			<SearchField
				class="project-list__search"
				label="Search projects by name or path"
				placeholder="Search projects"
				value={projectQuery}
				oninput={(e) => onProjectQuery(e.currentTarget.value)}
				onClear={clearSearch}
			/>
			<Button variant="outline" size="sm" onclick={onRefresh}>Refresh</Button>
		</div>
		<FilterScopeStatus
			scopeLabel={isSearching
				? `search "${trimmedQuery}"`
				: pinnedCount > 0
					? `every project · ${pinnedCount} pinned`
					: 'every project'}
			shownCount={isSearching ? matchCount : undefined}
			totalCount={isSearching ? projects.length : undefined}
			itemNoun="project"
		/>
		{#if newThread}
			<div class="project-list__new">{@render newThread()}</div>
		{/if}
		<CommsThreadFilters
			{tags}
			{filterTag}
			{filterParticipant}
			{filterResolved}
			{onFilterTag}
			{onClearParticipant}
			{onFilterResolved}
		/>
	</div>

	{#if loading}
		<div class="project-list__skeleton" aria-hidden="true">
			<div class="project-list__skel-row"></div>
			<div class="project-list__skel-row"></div>
			<div class="project-list__skel-row project-list__skel-row--short"></div>
		</div>
		<p class="sr-only" role="status">Loading threads…</p>
	{:else if isSearching && matchCount === 0}
		<FilteredEmptyState
			icon={Search}
			filtered={true}
			emptyText="No threads yet"
			filteredText={`No projects match "${trimmedQuery}"`}
			filteredSubtext="Try a different name or path, or clear the search."
			onClearFilters={clearSearch}
		/>
	{:else if projects.length === 0}
		{#if hasActiveFilter}
			<p class="project-list__empty">
				No threads match the current filters.
				<button type="button" class="project-list__clear-filters" onclick={clearFilters}
					>Clear filters</button
				>
			</p>
		{:else}
			<p class="project-list__empty">
				No threads yet. Threads are conversations exchanged with other sessions — start one above to
				see it here.
			</p>
		{/if}
	{:else}
		<ProjectDirectory
			{projects}
			query={projectQuery}
			expandedPath={expandedProjectPath}
			{openDuration}
			{now}
			unknownExpandable
			reconcileOrder={false}
			showRowCounts
			alwaysShowPath
			rowMode="toggle"
			indentStep={12}
			primaryNoun="thread"
			secondaryNoun="message"
			activityLabel="updated"
			unknownNote="Threads from scratch folders, and threads with no project."
			{onToggleProject}
			{onOpenSettings}
			{onTogglePin}
			body={projectThreads}
		/>
	{/if}
</div>

<style>
	.project-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		min-width: 0;
	}

	.project-list__controls {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.project-list__search-row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-3);
	}

	.project-list__empty {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-sm);
		line-height: var(--leading-body);
		color: var(--text-3);
	}

	.project-list__clear-filters {
		padding: 0;
		border: none;
		background: none;
		color: var(--status-info-fg);
		font-family: var(--font-mono, monospace);
		font-size: var(--text-sm);
		text-decoration: underline;
		text-underline-offset: 2px;
		cursor: pointer;
	}

	.project-list__clear-filters:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.project-list__skeleton {
		display: flex;
		flex-direction: column;
	}

	.project-list__skel-row {
		height: 38px;
		border-bottom: var(--hairline) solid var(--border-subtle);
		background: linear-gradient(
			90deg,
			var(--surface-2) 0%,
			var(--surface-shine) 50%,
			var(--surface-2) 100%
		);
		background-size: 200% 100%;
	}

	.project-list__skel-row--short {
		height: 30px;
		opacity: 0.7;
	}
</style>
