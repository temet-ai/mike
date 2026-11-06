<script lang="ts">
	import {
		CHAMFER,
		PAD,
		PITCH,
		TRACE_KINDS,
		VIA_R,
		type PadMark,
		type Route,
		type Trace,
		type TraceKind,
		type TraceLayer as PcbLayer
	} from '$lib/board/types';
	import { dashArray, pathLength, tracePath } from '$lib/board/trace';

	interface Props {
		traces: readonly Trace[];
		pads: readonly PadMark[];
		width: number;
		height: number;
		silk: readonly Route[];
		layer?: PcbLayer | 'both';
		onSelectLink?: (route: { routeKey: string; kind: TraceKind }) => void;
		selectedRouteKey?: string | null;
		selectedKind?: TraceKind | null;
	}

	let {
		traces,
		pads,
		width,
		height,
		silk,
		layer = 'both',
		onSelectLink,
		selectedRouteKey = null,
		selectedKind = null
	}: Props = $props();

	const showBottom = $derived(layer !== 'top');
	const showTop = $derived(layer !== 'bottom');

	interface PacketAnim {
		readonly path: string;
		readonly dashTo: number;
		readonly reverse: boolean;
		readonly chevronOffset: number;
	}

	interface LitTrace {
		readonly key: string;
		readonly routeKey: string;
		readonly kind: TraceKind;
		readonly path: string;
		readonly color: string;
		readonly width: number;
		readonly opacity: number;
		readonly dash: string;
		readonly packet?: PacketAnim;
		readonly selected: boolean;
	}

	function litOf(t: Trace, bottom: boolean): LitTrace {
		const spec = TRACE_KINDS[t.kind];
		const color = `var(${t.colorToken ?? spec.colorToken})`;
		const hops = t.hops ?? [];
		const path = tracePath(t.points, CHAMFER, hops);
		const strokeWidth = t.width ?? (bottom ? 1 : 1.5);
		const opacity = t.opacity ?? (bottom ? 0.6 : 0.55);
		let packet: PacketAnim | undefined;
		if (spec.packets && t.packets !== false) {
			const len = Math.max(40, pathLength(t.points, CHAMFER, hops.length));
			packet = { path, dashTo: -len, reverse: !!t.reverse, chevronOffset: t.reverse ? 40 : 60 };
		}
		const selected = selectedRouteKey === t.routeKey && selectedKind === t.kind;
		return {
			key: t.key,
			routeKey: t.routeKey,
			kind: t.kind,
			path,
			color,
			width: strokeWidth,
			opacity: selected ? 1 : opacity,
			dash: dashArray(t.kind),
			packet,
			selected
		};
	}

	function routeLabel(t: LitTrace): string {
		return `Select ${t.kind.replaceAll('_', ' ')} route: ${t.routeKey.replaceAll(':', ' → ')}`;
	}

	function activateLink(t: LitTrace): void {
		onSelectLink?.({ routeKey: t.routeKey, kind: t.kind });
	}

	function handleLinkKeydown(event: KeyboardEvent, t: LitTrace): void {
		if (event.key !== 'Enter' && event.key !== ' ') return;
		event.preventDefault();
		activateLink(t);
	}

	const bottomLit = $derived(
		traces.filter((t) => TRACE_KINDS[t.kind].layer === 'bottom').map((t) => litOf(t, true))
	);
	const topLit = $derived(
		traces.filter((t) => TRACE_KINDS[t.kind].layer === 'top').map((t) => litOf(t, false))
	);
	const bottomPads = $derived(pads.filter((p) => p.layer === 'bottom'));
	const topPads = $derived(pads.filter((p) => p.layer === 'top'));

	interface Rect {
		readonly x: number;
		readonly y: number;
		readonly w: number;
		readonly h: number;
	}

	function topPadRect(p: PadMark): Rect {
		const wide = PAD + PITCH * ((p.n ?? 1) - 1);
		if (p.vertical) {
			return { x: p.x - PAD / 2, y: p.y - wide / 2, w: PAD, h: wide };
		}
		return { x: p.x - wide / 2, y: p.y - PAD / 2, w: wide, h: PAD };
	}
</script>

{#snippet lit(t: LitTrace)}
	{#if onSelectLink}
		<path
			class="trace-hit"
			d={t.path}
			fill="none"
			stroke="transparent"
			stroke-width="10"
			stroke-linejoin="round"
			role="button"
			tabindex="0"
			aria-label={routeLabel(t)}
			aria-pressed={t.selected}
			onclick={() => activateLink(t)}
			onkeydown={(event) => handleLinkKeydown(event, t)}
		/>
	{/if}
	<path
		d={t.path}
		fill="none"
		stroke={t.color}
		stroke-width={t.width}
		opacity={t.opacity}
		stroke-linejoin="round"
		stroke-dasharray={t.dash || undefined}
		style={t.selected
			? `filter: drop-shadow(0 0 2px var(--cyan-border)) drop-shadow(0 0 6px var(--cyan-dim));`
			: undefined}
		aria-hidden="true"
		pointer-events="none"
	/>
	{#if t.packet}
		{@const p = t.packet}
		<path
			class="trace-packet"
			data-motion="wait-dash"
			data-direction={p.reverse ? 'reverse' : undefined}
			d={p.path}
			fill="none"
			stroke={t.color}
			stroke-width="2"
			stroke-linecap="round"
			style="stroke-dasharray: 12 {-p.dashTo - 12}; --dash-to: {p.dashTo}px;"
			aria-hidden="true"
			pointer-events="none"
		/>
		<polygon
			class="trace-chevron"
			points="-4,-3 4,0 -4,3"
			fill={t.color}
			style="offset-path: path('{p.path}'); offset-distance: {p.chevronOffset}%; offset-rotate: auto;"
			aria-hidden="true"
			pointer-events="none"
		/>
	{/if}
{/snippet}

<svg class="trace-layer" {width} {height} viewBox="0 0 {width} {height}">
	{#if showBottom}
		<g class="trace-layer__silk" aria-hidden="true">
			{#each silk as route (route.key)}
				<path
					d={tracePath(route.points, CHAMFER)}
					fill="none"
					stroke="var(--border-strong)"
					stroke-width="1"
					opacity="0.6"
					pointer-events="none"
				/>
			{/each}
		</g>
		<g class="trace-layer__lit trace-layer__lit--bottom">
			{#each bottomLit as t (t.key)}
				{@render lit(t)}
			{/each}
		</g>
	{/if}
	{#if showTop}
		<g class="trace-layer__lit trace-layer__lit--top">
			{#each topLit as t (t.key)}
				{@render lit(t)}
			{/each}
		</g>
	{/if}
	<g class="trace-layer__pads" aria-hidden="true">
		{#if showBottom}
			{#each bottomPads as p, i (i)}
				<circle
					cx={p.x}
					cy={p.y}
					r={VIA_R}
					fill="var(--surface-1)"
					stroke="var({p.colorToken})"
					stroke-width="1"
					pointer-events="none"
				/>
				<circle cx={p.x} cy={p.y} r="1" fill="var({p.colorToken})" pointer-events="none" />
			{/each}
		{/if}
		{#if showTop}
			{#each topPads as p, i (i)}
				{@const r = topPadRect(p)}
				<rect
					x={r.x}
					y={r.y}
					width={r.w}
					height={r.h}
					fill="var(--surface-1)"
					stroke="var({p.colorToken})"
					stroke-width="1"
					pointer-events="none"
				/>
			{/each}
		{/if}
	</g>
</svg>

<style>
	.trace-layer {
		position: absolute;
		inset: 0;
		pointer-events: none;
	}

	.trace-hit {
		pointer-events: stroke;
		cursor: pointer;
	}
	.trace-hit:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.trace-chevron {
		visibility: hidden;
	}
</style>
