<script lang="ts">
	import { markerLabelFlips } from '$lib/session-flow/markers';
	import type { FlowMarker } from '$lib/session-flow/markers';

	let {
		markers,
		width,
		atMs,
		hovered = $bindable(null),
		onSeek
	}: {
		markers: readonly FlowMarker[];
		width: number;
		atMs: number;
		hovered?: string | null;
		onSeek: (atMs: number) => void;
	} = $props();

	const LABEL_GUTTER_PX = 88;
	const rowsInUse = $derived(markers.reduce((rows, marker) => Math.max(rows, marker.row + 1), 1));
	const ROW_PITCH_PX = 40;
	const NEAR_PLAYHEAD_MS = 500;

	function formatClockHMS(ms: number): string {
		const d = new Date(ms);
		const pad = (n: number) => String(n).padStart(2, '0');
		return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
	}
</script>

<div
	class="flow-markers"
	style="width: {width + LABEL_GUTTER_PX}px; height: {rowsInUse * ROW_PITCH_PX + 34}px;"
>
	{#each markers as marker (marker.id)}
		{@const clock = formatClockHMS(marker.atMs)}
		{@const active = Math.abs(marker.atMs - atMs) <= NEAR_PLAYHEAD_MS || hovered === marker.id}
		{@const flips = markerLabelFlips(marker.x, marker.label, width)}
		<button
			type="button"
			class="flow-markers__marker"
			class:flow-markers__marker--active={active}
			class:flow-markers__marker--flip={flips}
			style="left: {LABEL_GUTTER_PX + marker.x + (flips ? 1 : 0)}px; top: {marker.row *
				ROW_PITCH_PX}px;"
			aria-label={`Seek to ${marker.label} at ${clock}`}
			onclick={() => onSeek(marker.atMs)}
			onpointerenter={() => (hovered = marker.id)}
			onpointerleave={() => (hovered = null)}
			onfocus={() => (hovered = marker.id)}
			onblur={() => (hovered = null)}
		>
			<span class="flow-markers__stem" aria-hidden="true"></span>
			<span class="flow-markers__label">{marker.label}</span>
			<span class="flow-markers__clock">{clock}</span>
		</button>
	{/each}
</div>

<style>
	.flow-markers {
		position: relative;
	}
	.flow-markers__marker {
		position: absolute;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 2px;
		padding: 0;
		background: none;
		border: none;
		cursor: pointer;
		text-align: left;
	}
	.flow-markers__marker--flip {
		align-items: flex-end;
		text-align: right;
		transform: translateX(-100%);
	}
	.flow-markers__stem {
		display: block;
		width: 1px;
		height: 10px;
		background: var(--border-strong);
	}
	.flow-markers__label {
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--text-2);
		white-space: nowrap;
	}
	.flow-markers__clock {
		font-family: var(--font-mono);
		font-size: 10px;
		color: var(--text-3);
		white-space: nowrap;
	}
	.flow-markers__marker--active .flow-markers__stem {
		background: var(--signal-active);
	}
	.flow-markers__marker--active .flow-markers__label {
		color: var(--signal-active);
	}
	.flow-markers__marker:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
</style>
