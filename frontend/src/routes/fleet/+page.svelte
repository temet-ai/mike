<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { FleetApiError, getFleetGraph, getFleetProjects } from '$lib/fleet/api';
	import { computeComponents, type ComponentInfo } from '$lib/fleet/canvasLayout';
	import {
		deriveActiveFleet,
		fromGraphResponse,
		matchesQuery,
		partitionByConnectivity
	} from '$lib/fleet/graphNormalize';
	import type { CanvasEdge, CanvasNode } from '$lib/fleet/graphNormalize';
	import type { FleetProjectSummary } from '$lib/fleet/types';
	import { parseIso } from '$lib/time';
	import { MAX_EVENTS, proxyEvents } from '$lib/ws.svelte';
	import FleetBoard from '$lib/fleet/FleetBoard.svelte';
	import FleetCanvas from '$lib/fleet/FleetCanvas.svelte';
	import FleetInspectorPanel from '$lib/fleet/FleetInspectorPanel.svelte';
	import FleetSourceBar from '$lib/fleet/FleetSourceBar.svelte';
	import type { ClusterChip, FleetView } from '$lib/fleet/FleetSourceBar.svelte';
	import ProjectNotebook from '$lib/fleet/ProjectNotebook.svelte';
	import EmptyStateOnboarding from '$lib/components/shared/EmptyStateOnboarding.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';

	import { chooseFleetProject, LatestFleetRequest } from '$lib/fleet/projectSelection';
	const DEFAULT_WINDOW_HOURS = 24;
	const NOW_TICK_MS = 30_000;
	const REFRESH_POLL_MS = 20_000;
	const REFETCH_THROTTLE_MS = 500;

	let now = $state(Date.now());

	let selectedProject = $state('');
	let windowHours = $state(DEFAULT_WINDOW_HOURS);
	let selectedNodeId = $state<string | null>(null);
	let hydrated = $state(false);
	let lastProjectParam = '';
	let view = $state<FleetView>('relationships');
	let manualClusterHubId = $state<string | null>(null);

	let searchQuery = $state('');
	let centerRequest = $state(0);

	let projects = $state<FleetProjectSummary[]>([]);
	let projectsLoading = $state(true);
	let projectsError = $state<string | null>(null);
	const projectRequests = new LatestFleetRequest();
	const graphRequests = new LatestFleetRequest();

	let graphNodes = $state<CanvasNode[]>([]);
	let graphEdges = $state<CanvasEdge[]>([]);
	let graphLoading = $state(true);
	let graphError = $state<FleetApiError | null>(null);

	const nodesById = $derived(new Map(graphNodes.map((n) => [n.id, n])));
	const partitioned = $derived(partitionByConnectivity(graphNodes, graphEdges));
	const activeFleet = $derived(deriveActiveFleet(graphNodes, graphEdges, now));
	const relationshipsAvailable = $derived(partitioned.connected.length >= 2);
	const effectiveView = $derived<FleetView>(
		view === 'relationships' ? (relationshipsAvailable ? 'relationships' : 'board') : view
	);

	const components = $derived(computeComponents(partitioned.connected, graphEdges));
	const componentByHub = $derived(new Map(components.map((c) => [c.hubId, c])));
	const defaultClusterHubId = $derived.by((): string | null => {
		if (components.length === 0) return null;
		if (selectedNodeId !== null) {
			const containing = components.find((c) => c.ids.includes(selectedNodeId as string));
			if (containing) return containing.hubId;
		}
		const ranked = components.map((c: ComponentInfo) => {
			const errorCount = c.ids.filter((id) => nodesById.get(id)?.state === 'error').length;
			const latest = c.ids.reduce((max, id) => {
				const iso = nodesById.get(id)?.lastActivityAt;
				const ms = iso ? (parseIso(iso)?.getTime() ?? 0) : 0;
				return Math.max(max, ms);
			}, 0);
			return { c, errorCount, latest };
		});
		ranked.sort(
			(a, b) =>
				b.errorCount - a.errorCount || b.c.ids.length - a.c.ids.length || b.latest - a.latest
		);
		return ranked[0]?.c.hubId ?? null;
	});
	const selectedClusterHubId = $derived(
		manualClusterHubId !== null && componentByHub.has(manualClusterHubId)
			? manualClusterHubId
			: defaultClusterHubId
	);
	const selectedComponent = $derived(
		selectedClusterHubId !== null ? (componentByHub.get(selectedClusterHubId) ?? null) : null
	);
	const clusterChips = $derived<ClusterChip[]>(
		components.map((c) => {
			const hub = nodesById.get(c.hubId) ?? null;
			return {
				hubId: c.hubId,
				label: hub?.label ?? c.hubId,
				role: hub?.role ?? null,
				agentType: hub?.agentType ?? null,
				count: c.ids.length,
				errorCount: c.ids.filter((id) => nodesById.get(id)?.state === 'error').length
			};
		})
	);
	const scopedNodeIds = $derived(new Set(selectedComponent?.ids ?? []));
	const scopedNodes = $derived(partitioned.connected.filter((n) => scopedNodeIds.has(n.id)));
	const scopedEdges = $derived(
		graphEdges.filter((e) => scopedNodeIds.has(e.from) && scopedNodeIds.has(e.to))
	);

	const visibleForSearch = $derived(
		effectiveView === 'relationships' ? scopedNodes : activeFleet.active
	);
	const selectedNode = $derived(
		selectedNodeId !== null ? (nodesById.get(selectedNodeId) ?? null) : null
	);

	const trimmedSearchQuery = $derived(searchQuery.trim());
	const searchMatches = $derived(
		trimmedSearchQuery.length > 0
			? visibleForSearch.filter((n) => matchesQuery(n, trimmedSearchQuery))
			: []
	);

	async function refreshProjects(): Promise<void> {
		const current = projectRequests.begin();
		projectsLoading = true;
		projectsError = null;
		try {
			const loaded = await getFleetProjects();
			if (!current()) return;
			projects = loaded;
			selectedProject = chooseFleetProject(selectedProject, loaded);
		} catch (error) {
			if (current()) projectsError = error instanceof Error ? error.message : String(error);
		} finally {
			if (current()) projectsLoading = false;
		}
	}

	async function loadGraph(
		project: string,
		hours: number,
		opts: { showLoading: boolean } = { showLoading: false }
	): Promise<void> {
		if (!project) return;
		const current = graphRequests.begin();
		const canApply = () => current() && project === selectedProject && hours === windowHours;
		if (opts.showLoading) {
			graphLoading = true;
			graphError = null;
		}
		try {
			const res = await getFleetGraph(project, hours);
			if (!canApply()) return;
			const g = fromGraphResponse(res.nodes, res.edges);
			graphNodes = g.nodes;
			graphEdges = g.edges;
			graphError = null;
		} catch (err) {
			if (!canApply()) return;
			if (opts.showLoading) {
				graphNodes = [];
				graphEdges = [];
			}
			graphError =
				err instanceof FleetApiError ? err : new FleetApiError(String(err), 0, 'unknown');
		} finally {
			if (canApply()) graphLoading = false;
		}
	}

	function selectNode(id: string | null): void {
		selectedNodeId = id;
	}

	function changeProject(next: string): void {
		selectedProject = next;
		selectedNodeId = null;
		manualClusterHubId = null;
	}

	function changeView(next: FleetView): void {
		view = next;
	}

	function changeCluster(hubId: string): void {
		manualClusterHubId = hubId;
	}

	$effect(() => {
		const raw = page.url.searchParams.get('view');
		const next: FleetView = raw === 'board' || raw === 'notebook' ? raw : 'relationships';
		untrack(() => {
			if (view !== next) view = next;
		});
	});

	function submitSearch(): void {
		const first = searchMatches[0];
		if (!first) return;
		selectedNodeId = first.id;
		if (effectiveView === 'relationships') centerRequest += 1;
	}

	onMount(() => {
		const sp = page.url.searchParams;
		const proj = sp.get('project');
		lastProjectParam = proj ?? '';
		if (proj) selectedProject = proj;
		const win = sp.get('window');
		const winNum = win ? Number(win) : NaN;
		if (Number.isFinite(winNum) && winNum > 0) windowHours = winNum;
		const node = sp.get('node');
		if (node) selectedNodeId = node;
		hydrated = true;

		void refreshProjects();

		const nowTimer = setInterval(() => (now = Date.now()), NOW_TICK_MS);
		const pollTimer = setInterval(() => {
			if (!selectedProject) void refreshProjects();
			void loadGraph(selectedProject, windowHours, { showLoading: false });
		}, REFRESH_POLL_MS);
		return () => {
			projectRequests.invalidate();
			graphRequests.invalidate();
			if (refetchTimer !== null) clearTimeout(refetchTimer);
			clearInterval(nowTimer);
			clearInterval(pollTimer);
		};
	});

	$effect(() => {
		const project = selectedProject;
		const hours = windowHours;
		if (!hydrated) return;
		graphRequests.invalidate();
		graphNodes = [];
		graphEdges = [];
		graphError = null;
		graphLoading = Boolean(project);
		if (project) void loadGraph(project, hours, { showLoading: true });
	});

	$effect(() => {
		const requested = page.url.searchParams.get('project') ?? '';
		if (!hydrated) return;
		untrack(() => {
			if (requested === lastProjectParam) return;
			lastProjectParam = requested;
			changeProject(chooseFleetProject(requested, projects));
		});
	});

	let processedEvents = $state(0);
	let refetchTimer: ReturnType<typeof setTimeout> | null = null;
	function scheduleRefetch(): void {
		if (refetchTimer !== null) clearTimeout(refetchTimer);
		refetchTimer = setTimeout(() => {
			refetchTimer = null;
			if (!selectedProject) void refreshProjects();
			void loadGraph(selectedProject, windowHours, { showLoading: false });
		}, REFETCH_THROTTLE_MS);
	}
	$effect(() => {
		const total = proxyEvents.totalEvents;
		if (total === processedEvents) return;
		const start = total < processedEvents ? 0 : Math.max(processedEvents, total - MAX_EVENTS);
		for (let i = start; i < total; i++) {
			const ev = proxyEvents.at(i);
			if (!ev) continue;
			if (ev.type === 'Lagged') {
				scheduleRefetch();
				continue;
			}
			if (ev.type === 'RowChanged' && ev.table === 'sessions') scheduleRefetch();
		}
		processedEvents = total;
	});

	$effect(() => {
		const proj = selectedProject;
		const win = windowHours;
		const node = selectedNodeId;
		const v = view;
		untrack(() => {
			if (!hydrated) return;
			syncUrl(proj, win, node, v);
		});
	});

	function syncUrl(proj: string, win: number, node: string | null, v: FleetView): void {
		const url = new URL(page.url.href);
		if (proj) url.searchParams.set('project', proj);
		else url.searchParams.delete('project');
		url.searchParams.set('window', String(win));
		if (node) url.searchParams.set('node', node);
		else url.searchParams.delete('node');
		if (v === 'board' || v === 'notebook') url.searchParams.set('view', v);
		else url.searchParams.delete('view');
		const next = resolve(`/fleet?${url.searchParams.toString()}`);
		const current = `${page.url.pathname}${page.url.search}`;
		if (next === current) return;
		lastProjectParam = proj;
		void goto(next, { replace: true, reset: false });
	}
</script>

<svelte:head>
	<title>Fleet — mike</title>
</svelte:head>

<section class="fleet-page flex flex-col overflow-hidden" data-slot="fleet-page">
	<FleetSourceBar
		{projects}
		{selectedProject}
		onProjectChange={changeProject}
		{windowHours}
		onWindowHoursChange={(h) => (windowHours = h)}
		{searchQuery}
		onSearchQueryChange={(q) => (searchQuery = q)}
		onSearchSubmit={submitSearch}
		searchMatchCount={searchMatches.length}
		view={effectiveView}
		onViewChange={changeView}
		{relationshipsAvailable}
		connectedCount={partitioned.connected.length}
		clusters={clusterChips}
		{selectedClusterHubId}
		onClusterChange={changeCluster}
	/>

	{#if selectedProject && projectsError}
		<LoadErrorBanner
			title="Couldn't refresh the project selector"
			detail={projectsError}
			onRetry={() => void refreshProjects()}
		/>
	{/if}

	{#if graphError && graphNodes.length > 0}
		<LoadErrorBanner
			title="Couldn't refresh fleet sessions"
			detail={graphError.message}
			onRetry={() => void loadGraph(selectedProject, windowHours)}
		/>
	{/if}

	<div class="fleet-stage flex min-h-0 flex-1" data-slot="fleet-stage">
		{#if !selectedProject && projectsLoading}
			<EmptyStateOnboarding
				headline="Loading projects"
				body="Reading projects from this installation."
				command={null}
				confirmation=""
			/>
		{:else if !selectedProject && projectsError}
			<LoadErrorBanner
				title="Couldn't load projects"
				detail={projectsError}
				onRetry={() => void refreshProjects()}
			/>
		{:else if !selectedProject}
			<EmptyStateOnboarding
				headline="No projects captured yet"
				body="Run Claude Code in your project and send a prompt. Its captured sessions will appear here."
			/>
		{:else if effectiveView === 'notebook'}
			<ProjectNotebook projectPath={selectedProject} />
		{:else if graphLoading}
			<div
				class="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-6"
				data-slot="fleet-graph-loading"
				aria-busy="true"
			>
				<div class="fleet-loading-row" data-augmented-ui="tl-clip br-clip"></div>
				<div class="fleet-loading-row" data-augmented-ui="tl-clip br-clip"></div>
				<div class="fleet-loading-row" data-augmented-ui="tl-clip br-clip"></div>
			</div>
		{:else if graphError !== null && graphNodes.length === 0}
			<div
				class="flex min-h-0 flex-1 items-center overflow-y-auto p-6"
				data-slot="fleet-graph-error"
			>
				<LoadErrorBanner
					title="Couldn't load the fleet session graph"
					detail={graphError.message}
					onRetry={() => void loadGraph(selectedProject, windowHours, { showLoading: true })}
				/>
			</div>
		{:else if graphNodes.length === 0}
			<div class="flex min-h-0 flex-1 items-center overflow-y-auto" data-slot="fleet-graph-empty">
				<EmptyStateOnboarding
					headline="No sessions in this project's fleet"
					body={`No roster activity, agent dispatches, or mailbox messages for ${selectedProject} in the last ${windowHours}h.`}
					command={null}
					confirmation=""
				/>
			</div>
		{:else if effectiveView === 'board'}
			{#if activeFleet.active.length === 0}
				<div class="flex min-h-0 flex-1 items-center overflow-y-auto" data-slot="fleet-board-empty">
					<EmptyStateOnboarding
						headline="No active fleet activity in this window"
						body={`${activeFleet.historyCount} session${activeFleet.historyCount === 1 ? '' : 's'} in this window has no declared role, isn't mailbox-connected, and hasn't been active recently — not fleet, just history.`}
						command={null}
						confirmation=""
					>
						{#snippet actions()}
							<span class="fleet-empty-action-btn-backing">
								<a
									class="fleet-empty-action-btn"
									href={resolve('/sessions')}
									data-augmented-ui="tr-clip bl-clip border"
								>
									See all sessions
								</a>
							</span>
						{/snippet}
					</EmptyStateOnboarding>
				</div>
			{:else}
				<FleetBoard
					nodes={activeFleet.active}
					historyCount={activeFleet.historyCount}
					selectedId={selectedNodeId}
					{searchQuery}
					{now}
					onSelect={selectNode}
					edges={graphEdges}
				/>
				<FleetInspectorPanel
					node={selectedNode}
					edges={graphEdges}
					{nodesById}
					{now}
					onSelect={selectNode}
				/>
			{/if}
		{:else}
			<FleetCanvas
				nodes={scopedNodes}
				edges={scopedEdges}
				selectedId={selectedNodeId}
				{searchQuery}
				{centerRequest}
				{now}
				scopeKey={selectedClusterHubId ?? ''}
				onSelect={selectNode}
			/>
			<FleetInspectorPanel
				node={selectedNode}
				edges={graphEdges}
				{nodesById}
				{now}
				onSelect={selectNode}
			/>
		{/if}
	</div>
</section>

<style>
	.fleet-page {
		height: calc(100vh - 44px);
		container-type: inline-size;
		container-name: fleet-workspace;
	}

	@container fleet-workspace (max-width: 640px) {
		.fleet-stage {
			flex-direction: column;
		}
	}

	.fleet-loading-row {
		--edge: 4px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		height: 48px;
		background: linear-gradient(
			90deg,
			var(--surface-2) 0%,
			var(--surface-shine) 50%,
			var(--surface-2) 100%
		);
		background-size: 200% 100%;
	}
	.fleet-loading-row {
		animation: fleet-loading-shimmer 1.6s ease-in-out infinite;
	}
	@keyframes fleet-loading-shimmer {
		0% {
			background-position: 200% 0;
		}
		100% {
			background-position: -200% 0;
		}
	}

	.fleet-empty-action-btn-backing {
		display: inline-flex;
		background: var(--surface-bg);
	}
	.fleet-empty-action-btn {
		--edge: 6px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: calc(var(--edge) + 4px) 12px;
		background: var(--surface-card);
		color: var(--text-2);
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 11px;
		text-decoration: none;
		transition: color 0.14s;
	}
	.fleet-empty-action-btn:hover {
		color: var(--text-strong);
		--aug-border-bg: var(--interactive-active-border);
	}
	.fleet-empty-action-btn:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--ring);
	}
</style>
