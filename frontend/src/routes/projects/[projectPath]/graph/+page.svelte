<script lang="ts">
	import { untrack } from 'svelte';
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	import { FoldHorizontal } from '@lucide/svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import EmbeddingGraph from '$lib/components/graph/EmbeddingGraph.svelte';
	import { EmbeddingGraphStore } from '$lib/graph/embeddingGraphStore.svelte';
	import { ApiError } from '$lib/api';
	import { getProjectEmbeddingGraph, type ProjectEmbeddingGraph } from '$lib/api/embeddingGraph';
	import { getFleetProjects, FleetApiError } from '$lib/fleet/api';
	import type { FleetProjectSummary } from '$lib/fleet/types';

	const FULL_VIEWPORT = { width: 960, height: 560 };

	const projectPath = $derived(page.params.projectPath ?? '');
	const backHref = $derived(
		resolve(`projects/${encodeURIComponent(projectPath)}` as `projects/${string}`)
	);

	let projects = $state<FleetProjectSummary[]>([]);
	let projectsLoading = $state(true);
	let projectsError = $state<string | null>(null);

	async function loadProjects(): Promise<void> {
		projectsLoading = true;
		projectsError = null;
		try {
			projects = await getFleetProjects();
		} catch (err) {
			projects = [];
			projectsError = err instanceof FleetApiError ? err.message : String(err);
		} finally {
			projectsLoading = false;
		}
	}

	onMount(() => {
		void loadProjects();
	});

	const project = $derived(projects.find((p) => p.project_path === projectPath) ?? null);
	const projectNotFound = $derived(!projectsLoading && projectsError === null && project === null);

	let data = $state<ProjectEmbeddingGraph | null>(null);
	let graphLoading = $state(false);
	let graphError = $state<string | null>(null);
	let loadedForId = $state<number | null>(null);
	const store = new EmbeddingGraphStore();

	async function loadGraph(id: number): Promise<void> {
		graphLoading = true;
		graphError = null;
		try {
			data = await getProjectEmbeddingGraph(id, { limit: 100 });
			store.load(data, FULL_VIEWPORT);
			store.similarityThreshold = 0.6;
		} catch (err) {
			graphError = err instanceof ApiError ? err.message : String(err);
		} finally {
			loadedForId = id;
			graphLoading = false;
		}
	}

	$effect(() => {
		const id = project?.project_id;
		untrack(() => {
			if (id == null) return;
			if (loadedForId === id || graphLoading) return;
			void loadGraph(id);
		});
	});

	function retry(): void {
		const id = project?.project_id;
		if (id != null) void loadGraph(id);
	}

	function goToSession(id: string): void {
		void goto(resolve(`sessions/${encodeURIComponent(id)}` as `sessions/${string}`));
	}
	function goToProject(path: string): void {
		void goto(resolve(`projects/${encodeURIComponent(path)}` as `projects/${string}`));
	}
</script>

<svelte:head>
	<title>Session map — {projectPath || 'Project'} — mike</title>
</svelte:head>

<section class="peg-page" aria-label="Project embedding graph">
	<header class="peg-header">
		<span class="peg-back-backing">
			<a
				href={backHref}
				class="peg-back"
				data-augmented-ui="tr-clip bl-clip border"
				aria-label="Back to project"
			>
				<FoldHorizontal size={14} aria-hidden="true" />
			</a>
		</span>
		<div class="peg-header__titles">
			<h2 class="peg-title">Session map</h2>
			<span class="peg-subtitle">
				{projectPath}
				{#if data && data.centroid.present}
					· {data.nodes.length} of {data.centroid.session_count} sessions plotted
				{/if}
			</span>
		</div>
	</header>

	<div class="peg-body">
		{#if projectsLoading}
			<div class="peg-skeleton" aria-hidden="true">
				<div class="peg-skel-row"></div>
				<div class="peg-skel-row peg-skel-row--short"></div>
			</div>
		{:else if projectsError !== null}
			<LoadErrorBanner
				title="Couldn't load this project"
				detail={projectsError}
				hint="Check that the inspect API is reachable on :9989."
				onRetry={() => void loadProjects()}
				backHref="/projects"
				backLabel="Back to projects"
			/>
		{:else if projectNotFound}
			<LoadErrorBanner
				title="Project not found"
				detail={`No captured traffic matches project path "${projectPath}".`}
				hint="It may have been renamed, or its captured sessions may have aged out."
				backHref="/projects"
				backLabel="Back to projects"
			/>
		{:else if project && project.project_id == null}
			<p class="peg-no-vector" data-slot="project-embedding-unavailable">
				No session map available for this project yet.
			</p>
		{:else if graphLoading && !data}
			<div class="peg-skeleton" aria-hidden="true">
				<div class="peg-skel-row"></div>
				<div class="peg-skel-row peg-skel-row--short"></div>
			</div>
		{:else if graphError}
			<LoadErrorBanner
				title="Couldn't load the session map"
				detail={graphError}
				hint="The project embedding-graph endpoint may not be reachable yet."
				onRetry={retry}
				{backHref}
				backLabel="Back to project"
			/>
		{:else if data && !data.centroid.present}
			<p class="peg-no-vector" data-slot="project-embedding-empty">
				No embedded sessions for this project yet.
			</p>
		{:else if data}
			<EmbeddingGraph {store} mode="full" onOpenSession={goToSession} />
			{#if data.sibling_projects.length > 0}
				<div class="peg-siblings">
					<span class="peg-siblings__label">Similar projects</span>
					<ul class="peg-siblings__list">
						{#each data.sibling_projects as sibling (sibling.project_id)}
							<li>
								<button
									type="button"
									class="peg-siblings__link"
									onclick={() => goToProject(sibling.project_path)}
								>
									<span class="peg-siblings__path">{sibling.project_path}</span>
									<span class="peg-siblings__pct">{Math.round(sibling.similarity * 100)}%</span>
								</button>
							</li>
						{/each}
					</ul>
				</div>
			{/if}
		{/if}
	</div>
</section>

<style>
	.peg-page {
		display: flex;
		flex-direction: column;
		min-height: calc(100vh - 44px);
	}
	.peg-header {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 12px 24px;
		flex-shrink: 0;
	}
	.peg-back-backing {
		display: inline-flex;
		flex-shrink: 0;
		background: var(--surface-bg);
	}
	.peg-back {
		--edge: 6px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		color: var(--text-2);
		text-decoration: none;
		transition: color var(--dur-1, 120ms) ease;
	}
	.peg-back:hover {
		--aug-border-bg: var(--border-default);
		color: var(--text-strong);
	}
	.peg-back:focus-visible {
		outline: 2px solid var(--focus);
		outline-offset: 2px;
	}
	.peg-header__titles {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}
	.peg-title {
		margin: 0;
		font-family: var(--font-display);
		font-size: 1.05rem;
		font-weight: 700;
		letter-spacing: -0.01em;
		color: var(--text-1);
	}
	.peg-subtitle {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
		overflow-wrap: anywhere;
	}

	.peg-body {
		flex: 1 1 auto;
		min-height: 0;
		display: flex;
		flex-direction: column;
		padding: 0 24px 24px;
		container-type: inline-size;
		container-name: embedding-graph-full;
	}

	.peg-no-vector {
		margin: 24px;
		font-size: var(--text-sm, 13px);
		color: var(--text-2);
	}

	.peg-skeleton {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 24px;
	}
	.peg-skel-row {
		height: 14px;
		border-radius: 2px;
		background: var(--surface-2);
	}
	.peg-skel-row--short {
		width: 40%;
	}

	.peg-siblings {
		margin-top: 16px;
		padding: 0 24px 8px;
		border-top: 1px solid var(--border-subtle);
		flex-shrink: 0;
	}
	.peg-siblings__label {
		display: block;
		margin: 12px 0 8px;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-size: 10px;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.07em;
		color: var(--text-3);
	}
	.peg-siblings__list {
		display: flex;
		flex-direction: column;
		gap: 4px;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.peg-siblings__link {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		width: 100%;
		max-width: 480px;
		padding: 4px 0;
		background: transparent;
		border: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--status-info-fg);
		text-align: left;
		cursor: pointer;
	}
	.peg-siblings__link:hover {
		color: var(--text-strong);
		text-decoration: underline;
	}
	.peg-siblings__link:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
		border-radius: 2px;
	}
	.peg-siblings__path {
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.peg-siblings__pct {
		flex-shrink: 0;
		color: var(--text-3);
		font-variant-numeric: tabular-nums;
	}
</style>
