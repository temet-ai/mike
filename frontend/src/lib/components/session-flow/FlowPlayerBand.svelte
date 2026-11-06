<script module lang="ts">
	export const TIMELINE_LABEL_GUTTER_PX = 88;
	export const TIMELINE_RIGHT_MARGIN_PX = 8;
	export function timelineTrackWidth(bandWidthPx: number): number {
		return Math.max(1, bandWidthPx - TIMELINE_LABEL_GUTTER_PX - TIMELINE_RIGHT_MARGIN_PX);
	}
</script>

<script lang="ts">
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Slider } from '$lib/components/ui/slider';
	import FlowTransport from './FlowTransport.svelte';
	import FlowLanes from './FlowLanes.svelte';
	import FlowMarkers from './FlowMarkers.svelte';
	import { buildLaneModel } from '$lib/session-flow/laneModel';
	import { buildMarkers } from '$lib/session-flow/markers';
	import { buildTimeScale } from '$lib/session-flow/timeScale';
	import { buildTransportSteps } from '$lib/session-flow/transport';
	import { formatClockAbsolute, formatDuration } from '$lib/time';
	import type {
		FlowBounds,
		FlowDataset,
		FlowPlayback,
		FlowSeekHandler
	} from '$lib/session-flow/types';

	let {
		dataset,
		playback,
		bounds,
		zoomed = false,
		onZoomToBrush,
		onResetZoom,
		atMs,
		brush,
		liveFrontierMs,
		onSeek,
		onBrush,
		width
	}: {
		dataset: FlowDataset;
		playback: FlowPlayback;
		bounds: FlowBounds | null;
		zoomed?: boolean;
		onZoomToBrush?: () => void;
		onResetZoom?: () => void;
		atMs: number;
		brush: FlowBounds | null;
		liveFrontierMs: number | null;
		onSeek: FlowSeekHandler;
		onBrush: (range: FlowBounds | null) => void;
		width: number;
	} = $props();

	const trackWidth = $derived(timelineTrackWidth(width));
	const lanes = $derived(buildLaneModel(dataset, trackWidth));
	const markers = $derived(buildMarkers(dataset, trackWidth));
	const steps = $derived(buildTransportSteps(dataset));

	let hoveredMarkerId = $state<string | null>(null);

	function formatClockHMS(ms: number): string {
		const d = new Date(ms);
		const pad = (n: number) => String(n).padStart(2, '0');
		return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
	}

	const headerMeta = $derived.by(() => {
		if (!bounds) return 'No loaded activity';
		const start = formatClockAbsolute(new Date(bounds.startMs).toISOString());
		const end = formatClockAbsolute(new Date(bounds.endMs).toISOString());
		return `${start} → ${end} · ${formatDuration(bounds.endMs - bounds.startMs)} · ${markers.length} markers`;
	});

	const clampedAtMs = $derived(
		bounds ? Math.min(bounds.endMs, Math.max(bounds.startMs, atMs)) : atMs
	);

	const SLIDER_STEPS = 10_000;
	const scale = $derived(buildTimeScale(dataset));
	const sliderWarpStart = $derived(bounds ? scale.warp(bounds.startMs) : 0);
	const sliderWarpSpan = $derived(
		bounds ? Math.max(1, scale.warp(bounds.endMs) - sliderWarpStart) : 1
	);
	const sliderValue = $derived(
		bounds
			? Math.round(((scale.warp(clampedAtMs) - sliderWarpStart) / sliderWarpSpan) * SLIDER_STEPS)
			: 0
	);
	function sliderStepToMs(step: number): number {
		return bounds
			? Math.round(scale.unwarp(sliderWarpStart + (step / SLIDER_STEPS) * sliderWarpSpan))
			: step;
	}

	let sliderRootEl = $state<HTMLElement | null>(null);
	$effect(() => {
		const label = formatClockHMS(clampedAtMs);
		const thumb = sliderRootEl?.querySelector('[data-slot="slider-thumb"]');
		thumb?.setAttribute('aria-valuetext', label);
	});
</script>

<SectionCard title="Timeline" collapsible sectionKey="flow-timeline" bordered={false}>
	{#snippet headerActions()}
		<span class="flow-player-band__meta">{headerMeta}</span>
		{#if zoomed}
			<Button variant="outline" size="sm" onclick={() => onResetZoom?.()}>Full range</Button>
		{:else if brush}
			<Button variant="outline" size="sm" onclick={() => onZoomToBrush?.()}
				>Zoom to selection</Button
			>
		{/if}
		{#if brush}
			<Button variant="outline" size="sm" onclick={() => onBrush(null)}>Clear selection</Button>
		{/if}
	{/snippet}
	<FlowTransport {playback} {bounds} {steps} {scale} {onSeek} />
	{#if bounds}
		<div class="flow-player-band__time-labels">
			<span>{formatClockHMS(bounds.startMs)}</span>
			<span>{formatClockHMS(bounds.endMs)}</span>
		</div>
		<div class="flow-player-band__slider">
			<Slider
				bind:ref={sliderRootEl}
				value={sliderValue}
				min={0}
				max={SLIDER_STEPS}
				step={1}
				aria-label="Timeline seek"
				onValueCommit={(value) => onSeek(sliderStepToMs(value))}
			/>
		</div>
		<FlowLanes
			{lanes}
			{bounds}
			{scale}
			width={trackWidth}
			{atMs}
			{brush}
			{liveFrontierMs}
			{onSeek}
			{onBrush}
		/>
		<p class="flow-player-band__caption">
			RECORDED MOMENTS · SELECTING A MARKER MOVES THE WHOLE INSTRUMENT AND PAUSES
		</p>
		<FlowMarkers {markers} width={trackWidth} {atMs} bind:hovered={hoveredMarkerId} {onSeek} />
	{:else}
		<p class="flow-player-band__empty">No loaded activity yet.</p>
	{/if}
	<p class="flow-player-band__footer">
		MIKE MARKS ARE TIMESTAMPS · RECENT HIGHLIGHTS LAST 8 RECORDED SECONDS
	</p>
</SectionCard>

<style>
	.flow-player-band__meta {
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--text-2);
		overflow-wrap: anywhere;
	}
	.flow-player-band__time-labels {
		display: flex;
		justify-content: space-between;
		padding-left: 88px;
		padding-right: 8px;
		font-family: var(--font-mono);
		font-size: 10px;
		color: var(--text-3);
		margin-top: var(--space-3);
	}
	.flow-player-band__slider {
		padding-left: 88px;
		padding-right: 8px;
		margin-top: var(--space-1);
	}
	.flow-player-band__caption,
	.flow-player-band__footer {
		font-family: var(--font-mono);
		font-size: 10px;
		color: var(--text-3);
		margin-top: var(--space-2);
	}
	.flow-player-band__empty {
		color: var(--text-2);
		margin-top: var(--space-3);
	}
</style>
