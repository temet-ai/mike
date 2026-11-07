<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { slide } from 'svelte/transition';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { Search, X } from '@lucide/svelte';
	import { getFleetProjectsWithCounts, FleetApiError } from '$lib/fleet/api';
	import {
		createProjectFor as createProjectIn,
		mergeProject as mergeProjectIn,
		renameProject as renameProjectIn,
		togglePin as togglePinIn,
		type ProjectListAccess
	} from '$lib/fleet/projectMutations';
	import type { FleetProjectWithCounts } from '$lib/fleet/types';
	import { isUnknownProjectPath, projectMatchesQuery } from '$lib/fleet/projectTree';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import EmptyStateOnboarding from '$lib/components/shared/EmptyStateOnboarding.svelte';
	import FilterScopeStatus from '$lib/components/shared/FilterScopeStatus.svelte';
	import FilteredEmptyState from '$lib/components/shared/FilteredEmptyState.svelte';
	import ProjectDirectory from '$lib/components/projects/ProjectDirectory.svelte';
	import ProjectSettingsDialog from '$lib/components/projects/ProjectSettingsDialog.svelte';
	import ProjectDetailPanel from '$lib/components/projects/ProjectDetailPanel.svelte';
	import ProjectComms from '$lib/comms/ProjectComms.svelte';
	import ProjectSessionsPanel from '$lib/components/projects/ProjectSessionsPanel.svelte';
	import ProjectEmbeddingPanel from '$lib/components/graph/ProjectEmbeddingPanel.svelte';
	import { shellState } from '$lib/stores/shellState.svelte';

	let projects = $state<FleetProjectWithCounts[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let expandedProjectPath = $state<string | null>(null);
	let hydrated = $state(false);
	let mounted = $state(false);

	const openDuration = $derived(mounted ? 180 : 0);

	async function refresh(): Promise<void> {
		loading = true;
		error = null;
		try {
			projects = await getFleetProjectsWithCounts();
		} catch (err) {
			projects = [];
			error = err instanceof FleetApiError ? err.message : String(err);
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		void refresh();
		const p = page.url.searchParams.get('p');
		if (p) expandedProjectPath = p;
		hydrated = true;
		mounted = true;
	});

	function openHelp(): void {
		shellState.helpDrawerOpen = true;
	}

	let nonEmptyProjects = $derived(
		projects.filter((p) => p.session_count > 0 || p.request_count > 0)
	);
	let hiddenEmptyCount = $derived(projects.length - nonEmptyProjects.length);

	let searchQuery = $state('');
	let trimmedSearchQuery = $derived(searchQuery.trim());
	let normalizedSearchQuery = $derived(trimmedSearchQuery.toLowerCase());
	let isSearching = $derived(normalizedSearchQuery.length > 0);

	let searchedProjects = $derived(
		nonEmptyProjects.filter((p) => projectMatchesQuery(p, normalizedSearchQuery))
	);
	let matchCount = $derived(searchedProjects.length);
	let hasNoMatches = $derived(isSearching && matchCount === 0);

	function clearSearch(): void {
		searchQuery = '';
	}

	let pinnedProjectCount = $derived(
		nonEmptyProjects.filter((p) => p.pinned && !isUnknownProjectPath(p.project_path)).length
	);

	function toggleProject(path: string): void {
		expandedProjectPath = expandedProjectPath === path ? null : path;
	}

	function syncUrl(sel: string | null): void {
		const url = new URL(page.url.href);
		if (sel) url.searchParams.set('p', sel);
		else url.searchParams.delete('p');
		const qs = url.searchParams.toString();
		const next = resolve(qs.length > 0 ? `/projects?${qs}` : '/projects');
		const current = `${page.url.pathname}${page.url.search}`;
		if (next === current) return;
		void goto(next, { replace: true, reset: false });
	}

	$effect(() => {
		const sel = expandedProjectPath;
		untrack(() => {
			if (!hydrated) return;
			syncUrl(sel);
		});
	});

	const projectList: ProjectListAccess = {
		get: () => projects,
		set: (next) => (projects = next)
	};

	function renameProject(path: string, customName: string | null): Promise<void> {
		return renameProjectIn(projectList, path, customName);
	}

	async function mergeProject(path: string, intoProjectUuid: string): Promise<void> {
		expandedProjectPath = await mergeProjectIn(projectList, path, intoProjectUuid);
	}

	let settingsPath = $state<string | null>(null);
	const settingsProject = $derived(projects.find((p) => p.project_path === settingsPath));

	function openSettings(path: string): void {
		settingsPath = path;
	}

	async function createProjectFor(path: string, name: string): Promise<void> {
		expandedProjectPath = await createProjectIn(projectList, path, name);
	}

	function togglePin(path: string, next: boolean): Promise<void> {
		return togglePinIn(projectList, path, next);
	}
</script>

<svelte:head>
	<title>Projects — mike</title>
</svelte:head>

{#if settingsProject}
	{#key settingsProject.project_path}
		<ProjectSettingsDialog
			project={settingsProject}
			{projects}
			onRenameProject={renameProject}
			onMergeProject={mergeProject}
			onCreateProject={createProjectFor}
			onClose={() => (settingsPath = null)}
		/>
	{/key}
{/if}

<section class="flex h-full flex-col overflow-hidden" data-slot="projects-page">
	<header class="pt-toolbar" data-slot="projects-toolbar">
		<div class="pt-toolbar-search-group" data-slot="projects-toolbar-search">
			{#if !loading && error === null && projects.length > 0}
				<span class="pt-search-backing">
					<div class="pt-search-wrap" data-augmented-ui="tl-clip border">
						<Search class="pt-search-icon" size={13} aria-hidden="true" />
						<input
							type="search"
							class="pt-search-input"
							placeholder="Search projects"
							aria-label="Search projects by name or path"
							bind:value={searchQuery}
						/>
						{#if isSearching}
							<button
								type="button"
								class="pt-search-clear"
								aria-label="Clear search"
								onclick={clearSearch}
							>
								<X size={12} aria-hidden="true" />
							</button>
						{/if}
					</div>
				</span>
				<FilterScopeStatus
					scopeLabel={isSearching
						? `search "${trimmedSearchQuery}"`
						: pinnedProjectCount > 0
							? `every captured project · ${pinnedProjectCount} pinned`
							: 'every captured project'}
					shownCount={isSearching ? matchCount : undefined}
					totalCount={isSearching ? nonEmptyProjects.length : undefined}
					itemNoun="project"
				/>
			{/if}
		</div>
		<span class="pt-toolbar-btn-backing">
			<button
				type="button"
				class="pt-toolbar-btn"
				data-augmented-ui="tr-clip bl-clip border"
				onclick={() => void refresh()}
			>
				refresh
			</button>
		</span>
	</header>

	<div class="flex min-h-0 flex-1 flex-col overflow-y-auto" data-slot="projects-stage">
		{#if loading}
			<div class="flex min-h-0 flex-1 flex-col gap-3 p-6" data-slot="projects-loading">
				<div class="h-12 animate-pulse rounded bg-[var(--surface-2)]"></div>
				<div class="h-12 animate-pulse rounded bg-[var(--surface-2)]"></div>
				<div class="h-12 animate-pulse rounded bg-[var(--surface-2)]"></div>
			</div>
		{:else if error !== null}
			<div class="flex min-h-0 flex-1 items-center overflow-y-auto p-6" data-slot="projects-error">
				<LoadErrorBanner
					title="Couldn't load projects"
					detail={error}
					hint="Check that the inspect API is reachable on :9989."
					onRetry={() => void refresh()}
				/>
			</div>
		{:else if projects.length === 0}
			<div class="flex min-h-0 flex-1 items-center overflow-y-auto p-6" data-slot="projects-empty">
				<EmptyStateOnboarding
					headline="No projects captured yet"
					body="Projects appear here once traffic from a codebase or workspace has been captured."
					command={null}
					confirmation="New projects will appear here as sessions are captured."
					onOpenHelp={openHelp}
				/>
			</div>
		{:else}
			{#snippet expandedBody(p: FleetProjectWithCounts)}
				<div class="pt-expanded-row" data-slot="project-row-expanded-top">
					<ProjectDetailPanel project={p} onRenameProject={renameProject} nested />
					<ProjectEmbeddingPanel project={p} nested />
				</div>
				<ProjectSessionsPanel project={p} nested />
				<ProjectComms projectPath={p.project_path} nested />
			{/snippet}

			{#if hasNoMatches}
				<div
					class="flex min-h-0 flex-1 items-center overflow-y-auto p-6"
					data-slot="projects-no-match"
				>
					<FilteredEmptyState
						icon={Search}
						filtered={true}
						emptyText="No projects captured yet"
						filteredText={`No projects match "${trimmedSearchQuery}"`}
						filteredSubtext="Try a different name or path, or clear the search."
						onClearFilters={clearSearch}
					/>
				</div>
			{:else}
				<div class="pt-list-col flex flex-col gap-6 px-6 py-4" data-slot="projects-list">
					<ProjectDirectory
						projects={nonEmptyProjects}
						query={searchQuery}
						expandedPath={expandedProjectPath}
						{openDuration}
						onToggleProject={toggleProject}
						onOpenSettings={openSettings}
						onTogglePin={(path, next) => void togglePin(path, next)}
						body={expandedBody}
					/>
					{#if hiddenEmptyCount > 0}
						<p class="pt-empty-hidden-note" data-slot="projects-empty-hidden-note">
							{hiddenEmptyCount} empty project{hiddenEmptyCount === 1 ? '' : 's'} hidden (no captured
							sessions or requests)
						</p>
					{/if}
				</div>
			{/if}
		{/if}
	</div>
</section>

<style>
	.pt-toolbar-btn-backing {
		display: inline-flex;
		background: var(--surface-bg);
	}
	.pt-toolbar-btn {
		--edge: 8px;
		--aug-tl: 0;
		--aug-tr: var(--edge);
		--aug-br: 0;
		--aug-bl: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		padding: 7px 14px;
		background: var(--surface-card);
		color: var(--text-2);
		font-size: var(--text-label);
		font-weight: 600;
		cursor: pointer;
		transition: color 0.12s ease;
	}
	.pt-toolbar-btn:hover {
		color: var(--text-strong);
		--aug-border-bg: var(--interactive-active-border);
	}
	.pt-toolbar-btn:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.pt-toolbar {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 12px;
		padding: 16px 24px 0;
	}
	.pt-toolbar-search-group {
		display: flex;
		min-width: 0;
		flex: 1 1 auto;
		flex-wrap: wrap;
		align-items: center;
		gap: 12px;
	}

	.pt-search-backing {
		display: inline-flex;
		min-width: 0;
		background: var(--surface-bg);
	}
	.pt-search-wrap {
		position: relative;
		display: flex;
		width: min(20rem, 100%);
		align-items: center;
		height: var(--control-h-lg);
		background: var(--surface-2);
		--aug-tl: var(--edge-sm);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		transition: filter var(--dur-1) var(--ease-io);
		filter: drop-shadow(0 0 0 transparent) drop-shadow(0 0 0 transparent);
	}
	.pt-search-wrap:hover {
		--aug-border-bg: var(--border-strong);
	}
	.pt-search-wrap:focus-within {
		--aug-border-bg: var(--interactive-focus-ring);
		filter: drop-shadow(0 0 2px var(--interactive-focus-ring))
			drop-shadow(0 0 12px var(--interactive-focus-ring));
	}
	.pt-search-wrap :global(.pt-search-icon) {
		position: absolute;
		left: 9px;
		color: var(--text-faint);
		pointer-events: none;
	}
	.pt-search-input {
		width: 100%;
		min-width: 0;
		height: 100%;
		font-family: var(--font-mono);
		font-size: var(--text-label);
		color: var(--text-1);
		background: transparent;
		border: none;
		border-radius: 0;
		padding: 0 28px;
		outline: none;
	}
	.pt-search-input::placeholder {
		color: var(--text-faint);
	}
	.pt-search-input::-webkit-search-cancel-button,
	.pt-search-input::-webkit-search-decoration {
		-webkit-appearance: none;
	}
	.pt-search-clear {
		position: absolute;
		right: 6px;
		display: inline-flex;
		width: 18px;
		height: 18px;
		align-items: center;
		justify-content: center;
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
	.pt-search-clear:hover {
		color: var(--text-strong);
		background: var(--surface-3);
	}
	.pt-search-clear:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 1px;
	}

	.pt-empty-hidden-note {
		margin-top: 4px;
		font-size: 11px;
		color: var(--text-3);
	}

	.pt-list-col {
		min-width: 0;
	}

	.pt-expanded-row {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-3, 12px);
	}
	.pt-expanded-row > :global(*) {
		flex: 1 1 300px;
		min-width: 0;
	}
</style>
