<script lang="ts">
	import { getContext } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { UnfoldHorizontal } from '@lucide/svelte';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import EmbeddingGraph from '$lib/components/graph/EmbeddingGraph.svelte';
	import { EmbeddingGraphStore } from '$lib/graph/embeddingGraphStore.svelte';
	import { ApiError } from '$lib/api';
	import { getProjectEmbeddingGraph, type ProjectEmbeddingGraph } from '$lib/api/embeddingGraph';
	import type { FleetProjectSummary } from '$lib/fleet/types';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';

	const COMPACT_VIEWPORT = { width: 420, height: 240 };

	interface Props {
		project: FleetProjectSummary;
		nested?: boolean;
	}

	let { project, nested = false }: Props = $props();
	const store = new EmbeddingGraphStore();

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);
	const sectionKey = $derived(`project:${project.project_path}:embedding-graph-open`);
	let sectionExpandedLocal = $state(false);
	const sectionExpanded = $derived(
		collapseStore ? collapseStore.isExpanded(sectionKey) : sectionExpandedLocal
	);

	let data = $state<ProjectEmbeddingGraph | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let loadedForPath = $state<string | null>(null);

	async function load(path: string, id: number): Promise<void> {
		loading = true;
		error = null;
		try {
			data = await getProjectEmbeddingGraph(id);
			store.load(data, COMPACT_VIEWPORT);
		} catch (err) {
			error = err instanceof ApiError ? err.message : String(err);
		} finally {
			loadedForPath = path;
			loading = false;
		}
	}

	$effect(() => {
		const path = project.project_path;
		const id = project.project_id;
		if (!sectionExpanded) return;
		if (id == null) return;
		if (loadedForPath === path || loading) return;
		void load(path, id);
	});

	function retry(): void {
		const id = project.project_id;
		if (id != null) void load(project.project_path, id);
	}

	const graphHref = $derived(
		resolve(
			`projects/${encodeURIComponent(project.project_path)}/graph` as `projects/${string}/graph`
		)
	);

	function goToSession(id: string): void {
		void goto(resolve(`sessions/${encodeURIComponent(id)}` as `sessions/${string}`));
	}

	function goToProject(path: string): void {
		void goto(resolve(`projects/${encodeURIComponent(path)}` as `projects/${string}`));
	}
</script>

<div data-slot="project-embedding-panel">
	<SectionCard
		title="Session map"
		collapsible
		bordered={!nested}
		{sectionKey}
		defaultCollapsed
		titleTooltip="This project's sessions laid out by embedding similarity around the project's centroid — the normalised mean of its session vectors."
	>
		{#snippet headerActions()}
			{#if data && data.centroid.present}
				<span class="pep-meta"
					>{data.nodes.length} of {data.centroid.session_count} sessions plotted</span
				>
			{/if}
			<a
				href={graphHref}
				class="pep-expand-link"
				aria-label="Open full session map for this project"
				title="Open full graph"
			>
				<UnfoldHorizontal size={13} aria-hidden="true" />
			</a>
		{/snippet}
		{#snippet children()}
			<div class="pep-body">
				{#if project.project_id == null}
					<p class="pep-empty" data-slot="project-embedding-unavailable">
						No session map available for this project yet.
					</p>
				{:else if loading && !data}
					<div class="pep-skeleton" aria-hidden="true">
						<div class="pep-skel-row"></div>
						<div class="pep-skel-row pep-skel-row--short"></div>
					</div>
				{:else if error}
					<LoadErrorBanner
						title="Couldn't load the session map"
						detail={error}
						hint="The project embedding-graph endpoint may not be reachable yet."
						onRetry={retry}
					/>
				{:else if data && !data.centroid.present}
					<p class="pep-empty" data-slot="project-embedding-empty">
						No embedded sessions for this project yet.
					</p>
				{:else if data}
					<EmbeddingGraph {store} mode="compact" onOpenSession={goToSession} />
					{#if data.sibling_projects.length > 0}
						<div class="pep-siblings">
							<span class="pep-siblings__label">Similar projects</span>
							<ul class="pep-siblings__list">
								{#each data.sibling_projects as sibling (sibling.project_id)}
									<li>
										<button
											type="button"
											class="pep-siblings__link"
											onclick={() => goToProject(sibling.project_path)}
										>
											{sibling.project_path}
											<span class="pep-siblings__pct">{Math.round(sibling.similarity * 100)}%</span>
										</button>
									</li>
								{/each}
							</ul>
						</div>
					{/if}
				{/if}
			</div>
		{/snippet}
	</SectionCard>
</div>

<style>
	.pep-meta {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption, 11px);
		color: var(--text-3);
		white-space: nowrap;
	}

	.pep-expand-link {
		display: inline-flex;
		align-self: center;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		flex-shrink: 0;
		border-radius: 2px;
		color: var(--text-3);
		transition:
			color var(--dur-1, 120ms) ease,
			background-color var(--dur-1, 120ms) ease;
	}
	.pep-expand-link:hover {
		background: var(--surface-2);
		color: var(--text-strong);
	}
	.pep-expand-link:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--ring);
	}

	.pep-empty {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
		font-style: italic;
	}

	.pep-skeleton {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.pep-skel-row {
		height: 14px;
		border-radius: var(--r-tol);
		background: var(--surface-2);
	}
	.pep-skel-row--short {
		width: 60%;
	}

	.pep-siblings {
		margin-top: var(--space-3, 12px);
		padding-top: var(--space-3, 12px);
		border-top: 1px solid var(--border-subtle);
	}
	.pep-siblings__label {
		display: block;
		margin-bottom: var(--space-2, 8px);
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-size: 10px;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.07em;
		color: var(--text-3);
	}
	.pep-siblings__list {
		display: flex;
		flex-direction: column;
		gap: 4px;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.pep-siblings__link {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		width: 100%;
		padding: 4px 0;
		background: transparent;
		border: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--status-info-fg);
		text-align: left;
		cursor: pointer;
	}
	.pep-siblings__link:hover {
		color: var(--text-strong);
		text-decoration: underline;
	}
	.pep-siblings__link:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
		border-radius: 2px;
	}
	.pep-siblings__pct {
		flex-shrink: 0;
		color: var(--text-3);
		font-variant-numeric: tabular-nums;
	}
</style>
