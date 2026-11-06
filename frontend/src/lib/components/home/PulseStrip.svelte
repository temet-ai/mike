<script lang="ts">
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { formatUptime, formatCount } from '$lib/time';

	interface Props {
		uptimeSeconds: number | null;
		requestCount: number | null;
		sessionsStored: number;
		hasMoreSessions: boolean;
		activeStreams: number | null;
		activeSessionsCount: number;
		href?: '/sessions';
	}

	const {
		uptimeSeconds,
		requestCount,
		sessionsStored,
		hasMoreSessions,
		activeStreams,
		activeSessionsCount,
		href = '/sessions'
	}: Props = $props();

	let containerEl: HTMLDivElement | undefined = $state();
	let canvasEl: HTMLCanvasElement | undefined = $state();

	const TICK_MS = 33;
	const CELL_CSS_PX = 3;

	onMount(() => {
		if (!canvasEl || !containerEl) return;
		const canvas: HTMLCanvasElement = canvasEl;
		const container: HTMLDivElement = containerEl;
		const ctxOrNull = canvas.getContext('2d');
		if (!ctxOrNull) return;
		const ctx: CanvasRenderingContext2D = ctxOrNull;

		let dpr = window.devicePixelRatio || 1;
		let buffer: number[] = [];
		let burstTicksLeft = 0;
		let burstLevel = 0;
		let blipTicksLeft = 0;
		let blipLevel = 0;
		let rafId: number | null = null;
		let lastTs: number | null = null;
		let acc = 0;

		function resolveColor(varName: string, fallback: string): string {
			const v = getComputedStyle(container).getPropertyValue(varName).trim();
			return v.length > 0 ? v : fallback;
		}

		function withAlpha(color: string, alpha: number): string {
			if (!color.includes(')')) return color;
			return `${color.replace(/\)\s*$/, '')} / ${alpha})`;
		}

		function resize(): void {
			dpr = window.devicePixelRatio || 1;
			const rect = container.getBoundingClientRect();
			const cssW = Math.max(1, rect.width);
			const cssH = Math.max(1, rect.height);
			canvas.width = Math.round(cssW * dpr);
			canvas.height = Math.round(cssH * dpr);
			const cols = Math.max(1, Math.floor(cssW / CELL_CSS_PX));
			const next = new Array<number>(cols).fill(0);
			const overlap = Math.min(cols, buffer.length);
			for (let i = 0; i < overlap; i++) {
				next[cols - overlap + i] = buffer[buffer.length - overlap + i] ?? 0;
			}
			buffer = next;
			draw();
		}

		function step(): void {
			burstTicksLeft -= 1;
			if (burstTicksLeft <= 0) {
				if (Math.random() < 0.02) {
					burstTicksLeft = 40 + Math.floor(Math.random() * 90);
					burstLevel = 0.5 + Math.random() * 0.5;
				} else {
					burstTicksLeft = 0;
					burstLevel = 0;
				}
			} else {
				burstLevel = Math.min(1, Math.max(0, burstLevel + (Math.random() - 0.45) * 0.3));
			}

			blipTicksLeft -= 1;
			if (blipTicksLeft <= 0) {
				if (Math.random() < 0.035) {
					blipTicksLeft = 2 + Math.floor(Math.random() * 5);
					blipLevel = 0.35 + Math.random() * 0.3;
				} else {
					blipTicksLeft = 0;
					blipLevel = 0;
				}
			}

			let lvl = 0;
			if (burstTicksLeft > 0) lvl = Math.max(lvl, burstLevel);
			if (blipTicksLeft > 0) lvl = Math.max(lvl, blipLevel);
			const quantized = lvl > 0 ? Math.ceil(lvl * 5) / 5 : 0;

			buffer.push(quantized);
			buffer.shift();
		}

		function draw(): void {
			const w = canvas.width;
			const h = canvas.height;
			ctx.clearRect(0, 0, w, h);

			const baselineColor = resolveColor('--border-default', 'rgba(255,255,255,0.35)');
			const traceColor = resolveColor('--status-success-fg', 'rgba(120,220,160,1)');

			ctx.strokeStyle = withAlpha(baselineColor, 0.35);
			ctx.lineWidth = Math.max(1, dpr);
			ctx.beginPath();
			ctx.moveTo(0, h - 1);
			ctx.lineTo(w, h - 1);
			ctx.stroke();

			const cellW = CELL_CSS_PX * dpr;
			const cols = buffer.length;
			for (let i = 0; i < cols; i++) {
				const level = buffer[i] ?? 0;
				if (level <= 0) continue;
				const barH = Math.max(2 * dpr, level * h * 0.72);
				const recency = cols > 1 ? i / (cols - 1) : 1;
				const alpha = 0.25 + recency * 0.65;
				ctx.fillStyle = withAlpha(traceColor, alpha);
				ctx.fillRect(i * cellW, h - barH, Math.max(1, cellW * 0.7), barH);
			}

			ctx.fillStyle = traceColor;
			ctx.fillRect(Math.max(0, w - Math.max(2, dpr * 2)), 0, Math.max(2, dpr * 2), h);
		}

		function tick(ts: number): void {
			if (lastTs === null) lastTs = ts;
			acc += ts - lastTs;
			lastTs = ts;
			while (acc > TICK_MS) {
				step();
				acc -= TICK_MS;
			}
			draw();
			rafId = requestAnimationFrame(tick);
		}

		resize();
		rafId = requestAnimationFrame(tick);
		window.addEventListener('resize', resize);

		return () => {
			if (rafId !== null) cancelAnimationFrame(rafId);
			window.removeEventListener('resize', resize);
		};
	});
</script>

<a
	class="pulse"
	data-slot="pulse-strip"
	data-augmented-ui="tl-clip tr-clip br-clip bl-clip border"
	href={resolve(href)}
>
	<span class="pulse-item pulse-item--online">
		<span class="pulse-dot" aria-hidden="true"></span>
		ONLINE
	</span>
	<span class="pulse-item"
		>UP <b class="pulse-value">{uptimeSeconds !== null ? formatUptime(uptimeSeconds) : '—'}</b
		></span
	>
	<span class="pulse-item"
		>REQ <b class="pulse-value">{requestCount !== null ? formatCount(requestCount) : '—'}</b></span
	>
	<span class="pulse-item"
		>STORED
		<b class="pulse-value">{formatCount(sessionsStored)}{hasMoreSessions ? '+' : ''}</b></span
	>
	<span class="pulse-item"
		>STREAMS
		<b class="pulse-value" class:pulse-value--warn={(activeStreams ?? 0) > 0}
			>{activeStreams !== null ? formatCount(activeStreams) : '—'}</b
		></span
	>
	<div class="pulse-trace" bind:this={containerEl}>
		<canvas bind:this={canvasEl} aria-hidden="true"></canvas>
	</div>
	<span class="pulse-tail"
		>{formatCount(activeSessionsCount)} active session{activeSessionsCount === 1 ? '' : 's'} &rarr;</span
	>
</a>

<style>
	.pulse {
		--edge-lg: 18px;
		--edge-sm: 8px;
		--aug-tl: var(--edge-lg);
		--aug-tr: var(--edge-sm);
		--aug-br: var(--edge-lg);
		--aug-bl: var(--edge-sm);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			90deg,
			var(--status-success-fg),
			var(--border-color, var(--border-default)) 50%,
			var(--status-success-fg)
		);
		display: flex;
		align-items: center;
		gap: 18px;
		padding: calc(var(--edge-lg) + 4px) calc(var(--edge-lg) + 4px) calc(var(--edge-lg) + 4px)
			calc(var(--edge-sm) + 4px);
		background: var(--surface-card);
		text-decoration: none;
		font-size: var(--text-caption);
		letter-spacing: 0.03em;
		font-variant-numeric: tabular-nums;
		color: var(--text-2);
	}
	.pulse:hover,
	.pulse:focus-visible {
		--aug-border-bg: linear-gradient(
			90deg,
			var(--status-success-fg),
			var(--status-success-fg) 50%,
			var(--status-success-fg)
		);
	}
	.pulse:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.pulse-item {
		flex-shrink: 0;
		color: var(--text-faint);
		white-space: nowrap;
	}
	.pulse-item--online {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		color: var(--status-success-fg);
		font-weight: 600;
	}
	.pulse-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--status-success-fg);
	}
	.pulse-value {
		color: var(--text-2);
		font-weight: 500;
	}
	.pulse-value--warn {
		color: var(--status-warning-fg);
	}

	.pulse-trace {
		flex: 1 1 auto;
		min-width: 0;
		height: 22px;
		mask-image: linear-gradient(90deg, transparent, black 14%, black 86%, transparent);
		-webkit-mask-image: linear-gradient(90deg, transparent, black 14%, black 86%, transparent);
	}
	.pulse-trace canvas {
		display: block;
		width: 100%;
		height: 100%;
	}

	.pulse-tail {
		flex-shrink: 0;
		color: var(--status-success-fg);
		font-weight: 600;
	}
</style>
