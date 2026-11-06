<script lang="ts">
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import { ArrowLeft } from '@lucide/svelte';
	import { onMount } from 'svelte';
	import { getFleetProject, FleetApiError } from '$lib/fleet/api';
	import { patchProjectName } from '$lib/api';
	import type { FleetProjectWithCounts } from '$lib/fleet/types';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import ProjectDetailPanel from '$lib/components/projects/ProjectDetailPanel.svelte';
	import ProjectIdentityHeading from '$lib/components/projects/ProjectIdentityHeading.svelte';
	import ProjectSessionsPanel from '$lib/components/projects/ProjectSessionsPanel.svelte';
	import TimeSpentPanel from '$lib/components/sessions/TimeSpentPanel.svelte';
	import ProjectEmbeddingPanel from '$lib/components/graph/ProjectEmbeddingPanel.svelte';
	import ProjectComms from '$lib/comms/ProjectComms.svelte';
	import ProjectMemoryPanel from '$lib/components/projects/ProjectMemoryPanel.svelte';
	import ProjectKnowledgePanel from '$lib/components/projects/ProjectKnowledgePanel.svelte';
	import ProjectLaunchesPanel from '$lib/components/projects/ProjectLaunchesPanel.svelte';

	const projectPath = $derived(page.params.projectPath ?? '');

	let projects = $state<FleetProjectWithCounts[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	async function load(): Promise<void> {
		loading = true;
		error = null;
		try {
			projects = [await getFleetProject(projectPath)];
		} catch (err) {
			projects = [];
			if (!(err instanceof FleetApiError && err.status === 404)) {
				error = err instanceof FleetApiError ? err.message : String(err);
			}
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		void load();
	});

	const project = $derived(projects.find((p) => p.project_path === projectPath) ?? null);
	const notFound = $derived(!loading && error === null && project === null);

	async function renameProject(path: string, customName: string | null): Promise<void> {
		const idx = projects.findIndex((p) => p.project_path === path);
		if (idx === -1) return;
		const previous = projects[idx]!.custom_name;
		projects = projects.map((p, i) => (i === idx ? { ...p, custom_name: customName } : p));
		try {
			const res = await patchProjectName(path, customName);
			projects = projects.map((p) =>
				p.project_path === path ? { ...p, custom_name: res.custom_name } : p
			);
		} catch (err) {
			projects = projects.map((p) =>
				p.project_path === path ? { ...p, custom_name: previous } : p
			);
			throw err;
		}
	}
</script>

<svelte:head>
	<title>{projectPath || 'Project'} — mike</title>
</svelte:head>

<section class="flex h-full flex-col overflow-hidden" data-slot="project-detail-page">
	<header class="flex items-center gap-3 border-b border-[var(--border-default)] px-6 py-4">
		<a
			class="pdp-back"
			href={resolve('/projects')}
			aria-label="Back to projects"
			data-slot="project-detail-back"
		>
			<ArrowLeft size={16} aria-hidden="true" />
		</a>
		{#if project !== null}
			<ProjectIdentityHeading {project} onRenameProject={renameProject} />
		{:else}
			<PageIntro title={projectPath || 'Project'} />
		{/if}
	</header>

	<div
		class="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-6 py-4"
		data-slot="project-detail-body"
	>
		{#if loading}
			<div class="flex flex-col gap-3" data-slot="project-detail-loading">
				<div class="h-8 w-1/3 animate-pulse rounded bg-[var(--surface-2)]"></div>
				<div class="h-24 animate-pulse rounded bg-[var(--surface-2)]"></div>
			</div>
		{:else if error !== null}
			<LoadErrorBanner
				title="Couldn't load this project"
				detail={error}
				hint="Check that the inspect API is reachable on :9989."
				onRetry={() => void load()}
				backHref="/projects"
				backLabel="Back to projects"
			/>
		{:else if notFound}
			<LoadErrorBanner
				title="Project not found"
				detail={`No captured traffic matches project path "${projectPath}".`}
				hint="It may have been renamed, or its captured sessions may have aged out."
				backHref="/projects"
				backLabel="Back to projects"
			/>
		{:else if project !== null}
			<div class="pdp-stack" data-slot="project-detail-stack">
				<div class="pdp-top-row" data-slot="project-detail-top-row">
					<ProjectDetailPanel {project} onRenameProject={renameProject} hideIdentity />
					<ProjectEmbeddingPanel {project} />
				</div>
				<div class="pdp-flush" data-slot="project-detail-flush">
					<ProjectKnowledgePanel {projectPath} />
					{#key projectPath}
						<ProjectComms {projectPath} />
					{/key}
					<TimeSpentPanel scope="project" {projectPath} />
					<ProjectSessionsPanel {project} />
					<ProjectLaunchesPanel {project} />
					<ProjectMemoryPanel {projectPath} />
				</div>
			</div>
		{/if}
	</div>
</section>

<style>
	.pdp-back {
		display: inline-flex;
		flex-shrink: 0;
		align-items: center;
		justify-content: center;
		width: 30px;
		height: 30px;
		border: 1px solid var(--border-default);
		border-radius: var(--r-tol, 2px);
		color: var(--text-2);
		transition:
			color var(--dur-1, 120ms) var(--ease-io, ease),
			border-color var(--dur-1, 120ms) var(--ease-io, ease);
	}
	.pdp-back:hover {
		color: var(--text-strong);
		border-color: var(--interactive-active-border);
	}
	.pdp-back:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.pdp-stack {
		display: flex;
		flex-direction: column;
		gap: var(--space-4, 16px);
	}

	.pdp-top-row {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-4, 16px);
		align-items: stretch;
	}
	.pdp-top-row > :global(*) {
		flex: 1 1 340px;
		min-width: 0;
	}

	.pdp-flush {
		display: flex;
		flex-direction: column;
		flex: 0 0 auto;
		background: var(--surface-card);
	}
	.pdp-flush > :global(* + *) {
		margin-top: -1px;
	}
	.pdp-flush > :global(:not(:first-child)) {
		--sec-tl: 0px;
	}
	.pdp-flush > :global(:not(:last-child)) {
		--sec-br: 0px;
	}
</style>
