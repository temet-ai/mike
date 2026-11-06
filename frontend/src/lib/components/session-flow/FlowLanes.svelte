<script lang="ts">
	import { flowKeyboardSeek, flowTimeToX, flowXToTime } from '$lib/session-flow/timeline';
	import type { FlowLane } from '$lib/session-flow/laneModel';
	import {
		formatGapDuration,
		LINEAR_TIME_SCALE,
		type FlowTimeScale
	} from '$lib/session-flow/timeScale';
	import type { FlowBounds } from '$lib/session-flow/types';

	let {
		lanes,
		bounds,
		scale = LINEAR_TIME_SCALE,
		width,
		atMs,
		brush,
		liveFrontierMs,
		onSeek,
		onBrush
	}: {
		lanes: readonly FlowLane[];
		bounds: FlowBounds | null;
		scale?: FlowTimeScale;
		width: number;
		atMs: number;
		brush: FlowBounds | null;
		liveFrontierMs: number | null;
		onSeek: (atMs: number) => void;
		onBrush: (range: FlowBounds | null) => void;
	} = $props();

	const LABEL_GUTTER_PX = 88;
	const GAP_BAND_MIN_PX = 6;
	const ROW_H = 20;
	const SVG_HEIGHT = ROW_H * 5;
	const DRAG_THRESHOLD_PX = 3;

	const totalWidth = $derived(width + LABEL_GUTTER_PX);

	let containerEl = $state<HTMLDivElement | null>(null);
	let dragStartTrackX = $state<number | null>(null);
	let dragCurrentTrackX = $state<number | null>(null);

	function toTrackX(clientX: number): number | null {
		if (!containerEl) return null;
		const rect = containerEl.getBoundingClientRect();
		if (rect.width <= 0) return null;
		const scale = totalWidth / rect.width;
		const localX = (clientX - rect.left) * scale - LABEL_GUTTER_PX;
		return Math.max(0, Math.min(width, localX));
	}

	function handlePointerDown(event: PointerEvent): void {
		if (!bounds) return;
		const trackX = toTrackX(event.clientX);
		if (trackX === null) return;
		dragStartTrackX = trackX;
		dragCurrentTrackX = trackX;
		containerEl?.setPointerCapture?.(event.pointerId);
	}

	function handlePointerMove(event: PointerEvent): void {
		if (dragStartTrackX === null) return;
		const trackX = toTrackX(event.clientX);
		if (trackX !== null) dragCurrentTrackX = trackX;
	}

	function handlePointerUp(): void {
		if (dragStartTrackX === null || dragCurrentTrackX === null || !bounds) {
			dragStartTrackX = null;
			dragCurrentTrackX = null;
			return;
		}
		const start = dragStartTrackX;
		const end = dragCurrentTrackX;
		dragStartTrackX = null;
		dragCurrentTrackX = null;
		if (Math.abs(end - start) < DRAG_THRESHOLD_PX) {
			onSeek(flowXToTime(start, bounds, width, scale));
			return;
		}
		onBrush({
			startMs: flowXToTime(Math.min(start, end), bounds, width, scale),
			endMs: flowXToTime(Math.max(start, end), bounds, width, scale)
		});
	}

	function handleKeydown(event: KeyboardEvent): void {
		if (!bounds) return;
		const next = flowKeyboardSeek(event.key, atMs, bounds, 1000);
		if (next !== null) {
			event.preventDefault();
			onSeek(next);
		}
	}

	function distinctActorCount(lane: FlowLane): number {
		return new Set(lane.intervals.map((interval) => interval.actorId)).size;
	}

	function laneLabel(lane: FlowLane): string {
		return lane.id === 'sidecars'
			? `${lane.label.toUpperCase()} (${distinctActorCount(lane)})`
			: lane.label.toUpperCase();
	}

	function intervalFill(interval: { error: boolean; late: boolean }): string {
		if (interval.error) return 'var(--signal-error)';
		if (interval.late) return 'var(--signal-warn)';
		return 'var(--signal-active)';
	}

	function formatClockHMS(ms: number): string {
		const d = new Date(ms);
		const pad = (n: number) => String(n).padStart(2, '0');
		return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
	}

	const cursorX = $derived(
		bounds ? LABEL_GUTTER_PX + flowTimeToX(atMs, bounds, width, scale) : null
	);
	const liveX = $derived(
		bounds && liveFrontierMs !== null
			? LABEL_GUTTER_PX + flowTimeToX(liveFrontierMs, bounds, width, scale)
			: null
	);
	const brushRect = $derived.by(() => {
		if (!bounds || !brush) return null;
		const x1 = LABEL_GUTTER_PX + flowTimeToX(brush.startMs, bounds, width, scale);
		const x2 = LABEL_GUTTER_PX + flowTimeToX(brush.endMs, bounds, width, scale);
		return { x: Math.min(x1, x2), w: Math.max(1, Math.abs(x2 - x1)) };
	});
	const gapBands = $derived.by(() => {
		if (!bounds) return [];
		return scale.gaps
			.filter((gap) => gap.endMs > bounds.startMs && gap.startMs < bounds.endMs)
			.map((gap) => {
				const x1 =
					LABEL_GUTTER_PX +
					flowTimeToX(Math.max(gap.startMs, bounds.startMs), bounds, width, scale);
				const x2 =
					LABEL_GUTTER_PX + flowTimeToX(Math.min(gap.endMs, bounds.endMs), bounds, width, scale);
				const w = Math.max(GAP_BAND_MIN_PX, x2 - x1);
				return {
					key: `${gap.startMs}-${gap.endMs}`,
					x: (x1 + x2) / 2 - w / 2,
					w,
					label: formatGapDuration(gap.endMs - gap.startMs)
				};
			});
	});
</script>

<div
	bind:this={containerEl}
	class="flow-lanes"
	style="width: {totalWidth}px"
	role="slider"
	aria-label="Recorded activity lanes"
	aria-orientation="horizontal"
	aria-valuemin={bounds?.startMs}
	aria-valuemax={bounds?.endMs}
	aria-valuenow={atMs}
	aria-valuetext={formatClockHMS(atMs)}
	tabindex={bounds ? 0 : -1}
	onpointerdown={handlePointerDown}
	onpointermove={handlePointerMove}
	onpointerup={handlePointerUp}
	onkeydown={handleKeydown}
>
	<svg viewBox="0 0 {totalWidth} {SVG_HEIGHT}" aria-hidden="true" focusable="false">
		{#each gapBands as band (band.key)}
			<rect
				x={band.x}
				y="0"
				width={band.w}
				height={SVG_HEIGHT}
				fill="var(--surface-2)"
				opacity="0.7"
			/>
			<line
				x1={band.x}
				x2={band.x}
				y1="0"
				y2={SVG_HEIGHT}
				stroke="var(--border-strong)"
				stroke-width="1"
				stroke-dasharray="1 3"
			/>
			<line
				x1={band.x + band.w}
				x2={band.x + band.w}
				y1="0"
				y2={SVG_HEIGHT}
				stroke="var(--border-strong)"
				stroke-width="1"
				stroke-dasharray="1 3"
			/>
			{#if band.w >= 34}
				<text
					x={band.x + band.w / 2}
					y={SVG_HEIGHT - 4}
					class="machine flow-lanes__gap-label"
					text-anchor="middle">{band.label}</text
				>
			{/if}
		{/each}
		{#if brushRect}
			<rect
				x={brushRect.x}
				y="0"
				width={brushRect.w}
				height={SVG_HEIGHT}
				fill="var(--text-1)"
				opacity="0.08"
			/>
			<line
				x1={brushRect.x}
				x2={brushRect.x}
				y1="0"
				y2={SVG_HEIGHT}
				stroke="var(--text-1)"
				stroke-width="1"
				stroke-dasharray="4 3"
			/>
			<line
				x1={brushRect.x + brushRect.w}
				x2={brushRect.x + brushRect.w}
				y1="0"
				y2={SVG_HEIGHT}
				stroke="var(--text-1)"
				stroke-width="1"
				stroke-dasharray="4 3"
			/>
		{/if}
		{#each lanes as lane, i (lane.id)}
			{@const rowY = i * ROW_H}
			<text x="0" y={rowY + ROW_H / 2 + 3} class="machine flow-lanes__label">{laneLabel(lane)}</text
			>
			<line
				x1={LABEL_GUTTER_PX}
				x2={LABEL_GUTTER_PX + width}
				y1={rowY + ROW_H / 2}
				y2={rowY + ROW_H / 2}
				stroke="var(--border-default)"
				stroke-width="1"
			/>
			{#each lane.intervals as interval (interval.id)}
				{@const x = LABEL_GUTTER_PX + interval.x}
				{@const endX = LABEL_GUTTER_PX + (interval.endX ?? width)}
				<rect
					{x}
					y={rowY + (ROW_H - 8) / 2}
					width={Math.max(1, endX - x)}
					height="8"
					fill={intervalFill(interval)}
				/>
			{/each}
			{#each lane.ticks as tick (tick.id)}
				<line
					x1={LABEL_GUTTER_PX + tick.x}
					x2={LABEL_GUTTER_PX + tick.x}
					y1={rowY + 2}
					y2={rowY + ROW_H - 2}
					stroke="var(--signal-complete)"
					stroke-width="1"
				/>
			{/each}
		{/each}
		{#if liveX !== null}
			<line
				x1={liveX}
				x2={liveX}
				y1="0"
				y2={SVG_HEIGHT}
				stroke="var(--signal-live)"
				stroke-width="1"
			/>
		{/if}
		{#if cursorX !== null}
			<line
				x1={cursorX}
				x2={cursorX}
				y1="0"
				y2={SVG_HEIGHT}
				stroke="var(--text-1)"
				stroke-width="1"
			/>
		{/if}
	</svg>
</div>

<style>
	.flow-lanes {
		display: block;
		height: 100px;
		touch-action: none;
	}
	.flow-lanes svg {
		display: block;
		width: 100%;
		height: 100%;
	}
	.flow-lanes__label {
		font-size: 9px;
		fill: var(--text-3);
	}
	.flow-lanes__gap-label {
		font-size: 8px;
		fill: var(--text-3);
	}
	.flow-lanes:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
</style>
