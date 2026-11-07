<script module lang="ts">
	import type { SessionFlowAvailability } from '$lib/api/generated/SessionFlowAvailability';
	import type {
		FlowBounds,
		FlowDataset,
		FlowPlayback,
		FlowScene,
		FlowSeekHandler,
		FlowSelection
	} from '$lib/session-flow/types';
	import type { FlowLiveMark } from '$lib/session-flow/sessionFlowStore.svelte';
	export interface SessionFlowProps {
		title?: string | null;
		dataset: FlowDataset;
		scene: FlowScene;
		playback: FlowPlayback;
		selection: FlowSelection;
		availability: SessionFlowAvailability | null;
		loading?: boolean;
		error?: Error | null;
		stale?: boolean;
		staleReasons?: readonly string[];
		followId: string | null;
		collapsedIds: ReadonlySet<string>;
		brush: FlowBounds | null;
		overlay?: readonly FlowLiveMark[];
		onSelect: (selection: FlowSelection) => void;
		onSeek: FlowSeekHandler;
		onFollow: (id: string | null) => void;
		onCollapse: (id: string) => void;
		onBrush: (range: FlowBounds | null) => void;
		onOpenDrawer: (requestId: string, kind: 'request' | 'response') => void;
		onRetry?: () => void;
	}
</script>

<script lang="ts">
	import { SvelteURLSearchParams } from 'svelte/reactivity';
	import { page } from '$app/state';
	import { Button } from '$lib/components/ui/button';
	import FlowHeaderBand from './FlowHeaderBand.svelte';
	import FlowPlayerBand from './FlowPlayerBand.svelte';
	import SessionFlowBoard from './SessionFlowBoard.svelte';
	import FlowBoardViewport from './FlowBoardViewport.svelte';
	import FlowFilterBand from './FlowFilterBand.svelte';
	import FlowDetailBand from './FlowDetailBand.svelte';
	import { buildAgentIdentities } from '$lib/session-flow/grouping';
	import { defaultLegend } from '$lib/session-flow/legendEntries';

	let {
		title = null,
		dataset,
		scene,
		playback,
		selection,
		availability,
		loading = false,
		error = null,
		stale = false,
		staleReasons = [],
		followId,
		collapsedIds,
		brush,
		overlay = [],
		onSelect,
		onSeek,
		onFollow,
		onCollapse,
		onBrush,
		onOpenDrawer,
		onRetry
	}: SessionFlowProps = $props();

	const unavailable = $derived(
		availability !== null && (typeof availability === 'string' || !('eligible' in availability))
	);

	let zoom = $state<FlowBounds | null>(null);
	const coordinator = $derived(
		[...dataset.actors.values()].find((actor) => actor.kind === 'session')
	);
	const peer = $derived([...dataset.actors.values()].find((actor) => actor.kind === 'peer'));
	const agentIdentityCount = $derived(buildAgentIdentities(dataset).length);
	const timelineHref = $derived.by(() => {
		const params = new SvelteURLSearchParams(page.url.search);
		params.set('view', 'requests');
		const query = params.toString();
		return query ? `${page.url.pathname}?${query}` : page.url.pathname;
	});

	let groupWidth = $state(0);
	const memberInnerWidth = $derived(Math.max(0, groupWidth - 24));
	const boardCollapsed = $derived(collapsedIds.has('agents'));

	const BOARD_STAGE_W = 1372;
	let boardHeight = $state(0);

	let filterActive = $state<Set<string>>(defaultLegend());
</script>

<div class="edge mech-group flow-instrument" data-design="funk" bind:clientWidth={groupWidth}>
	{#if error}<div role="alert" class="flow-status">
			<p>{error.message}</p>
			{#if onRetry}<Button
					variant="outline"
					class="h-auto min-h-8 max-w-full whitespace-normal break-words"
					onclick={onRetry}>Retry loading flow</Button
				>{/if}
		</div>{/if}
	{#if unavailable}
		<p class="flow-status">
			Flow is unavailable for this session: {typeof availability === 'string'
				? 'no admitted fresh start'
				: 'unsupported capture version'}. Recorded requests remain available in Requests.
		</p>
	{:else if availability === null && !loading && !error}
		<p class="flow-status">Flow availability has not been loaded.</p>
	{:else}
		{#if dataset.activities.size === 0 && !loading}<p class="flow-status">
				No recorded activity is available.
			</p>{/if}

		<FlowHeaderBand
			sessionId={dataset.sessionId}
			peerId={peer?.id ?? null}
			startedAt={dataset.bounds ? new Date(dataset.bounds.startMs).toISOString() : null}
			agentCount={agentIdentityCount}
			title={title ?? coordinator?.label ?? null}
			{timelineHref}
			{loading}
			{stale}
			{staleReasons}
		/>

		<FlowPlayerBand
			{dataset}
			{playback}
			bounds={zoom ?? dataset.bounds}
			zoomed={zoom !== null}
			onZoomToBrush={() => (zoom = brush)}
			onResetZoom={() => (zoom = null)}
			atMs={scene.atMs}
			{brush}
			liveFrontierMs={dataset.observationFrontierMs}
			{onSeek}
			{onBrush}
			width={memberInnerWidth}
		/>

		<div class="flow-board-row">
			<div class="flow-board-card edge" data-design="funk" aria-label="Board">
				<FlowBoardViewport naturalWidth={BOARD_STAGE_W} naturalHeight={boardHeight}>
					<SessionFlowBoard
						{dataset}
						{scene}
						atMs={scene.atMs}
						{selection}
						{onSelect}
						legend={filterActive}
						collapsed={boardCollapsed}
						onToggleCollapsed={() => onCollapse('agents')}
						{overlay}
						stageW={BOARD_STAGE_W}
						bind:naturalHeight={boardHeight}
					/>
				</FlowBoardViewport>
			</div>

			<div class="flow-selected-pane">
				<FlowDetailBand {dataset} {selection} {title} {followId} {onSelect} {onFollow} {onOpenDrawer} />
			</div>
		</div>

		<FlowFilterBand bind:active={filterActive} />
	{/if}
</div>

<style>
	.flow-instrument {
		color: var(--text-1);
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.flow-status {
		padding: var(--space-3) var(--space-4);
		color: var(--text-2);
	}
	.flow-board-row {
		display: grid;
		grid-template-columns: 1fr;
		min-width: 0;
	}
	.flow-board-card {
		--edge: var(--edge-sm);
		min-width: 0;
		background: var(--surface-card);
	}
	.flow-selected-pane {
		display: flex;
		flex-direction: column;
		min-width: 0;
		border-top: var(--hairline) solid var(--border-subtle);
		background: var(--surface-card);
	}
	@media (min-width: 860px) {
		.flow-board-row {
			grid-template-columns: minmax(0, 1fr) minmax(320px, 400px);
			align-items: stretch;
		}
		.flow-selected-pane {
			border-top: 0;
			border-left: var(--hairline) solid var(--border-subtle);
		}
	}
</style>
