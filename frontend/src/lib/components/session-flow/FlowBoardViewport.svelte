<script lang="ts">
	import type { Snippet } from 'svelte';
	import { Minus, Plus } from '@lucide/svelte';

	let {
		children,
		naturalWidth = 0,
		naturalHeight = 0
	}: {
		children: Snippet;
		naturalWidth?: number;
		naturalHeight?: number;
	} = $props();

	const MIN_ZOOM = 0.4;
	const MAX_ZOOM = 2;
	const ZOOM_STEP = 0.15;
	const MAX_FIT_HEIGHT_VH = 78;
	const MAX_FIT_HEIGHT_PX = 760;

	function clamp(value: number, min: number, max: number): number {
		return Math.min(max, Math.max(min, value));
	}
	function round2(value: number): number {
		return Math.round(value * 100) / 100;
	}

	let frameEl = $state<HTMLDivElement | undefined>(undefined);
	let measureEl = $state<HTMLDivElement | undefined>(undefined);
	let frameW = $state(0);
	let measuredW = $state(0);
	let measuredH = $state(0);
	const naturalW = $derived(naturalWidth > 0 ? naturalWidth : measuredW);
	const naturalH = $derived(naturalHeight > 0 ? naturalHeight : measuredH);
	let viewportH = $state(typeof window !== 'undefined' ? window.innerHeight : 800);

	$effect(() => {
		if (typeof window === 'undefined') return;
		const onResize = (): void => {
			viewportH = window.innerHeight;
		};
		window.addEventListener('resize', onResize);
		return () => window.removeEventListener('resize', onResize);
	});

	$effect(() => {
		const el = measureEl;
		if (!el || typeof ResizeObserver === 'undefined') return;
		const observer = new ResizeObserver((entries) => {
			const rect = entries[0]?.contentRect;
			if (!rect) return;
			measuredW = rect.width;
			measuredH = rect.height;
		});
		observer.observe(el);
		measuredW = el.offsetWidth;
		measuredH = el.offsetHeight;
		return () => observer.disconnect();
	});

	$effect(() => {
		const el = frameEl;
		if (!el || typeof ResizeObserver === 'undefined') return;
		const observer = new ResizeObserver((entries) => {
			const rect = entries[0]?.contentRect;
			frameW = rect ? rect.width : el.clientWidth;
		});
		observer.observe(el);
		frameW = el.clientWidth;
		return () => observer.disconnect();
	});

	const maxHeightPx = $derived(Math.min((viewportH * MAX_FIT_HEIGHT_VH) / 100, MAX_FIT_HEIGHT_PX));

	const fitScale = $derived.by((): number => {
		if (naturalW <= 0 || frameW <= 0 || naturalH <= 0) return 1;
		const widthScale = frameW / naturalW;
		const heightScale = maxHeightPx > 0 ? maxHeightPx / naturalH : widthScale;
		return Math.min(widthScale, heightScale, 1);
	});

	let userZoom = $state<number | null>(null);
	let panX = $state(0);
	let panY = $state(0);

	const isFit = $derived(userZoom === null);
	const zoom = $derived(userZoom ?? fitScale);
	const zoomLabel = $derived(isFit ? 'Fit' : `${Math.round(zoom * 100)}%`);
	const fitOffsetX = $derived(isFit ? Math.max(0, (frameW - naturalW * zoom) / 2) : 0);
	const frameHeightPx = $derived(Math.min(Math.round(naturalH * zoom), Math.round(maxHeightPx)));

	function clampPan(nextX: number, nextY: number, atZoom: number): { x: number; y: number } {
		const contentW = naturalW * atZoom;
		const contentH = Math.min(naturalH * atZoom, maxHeightPx);
		const minX = Math.min(0, frameW - contentW);
		const minY = Math.min(0, frameEl!.clientHeight - contentH);
		return { x: clamp(nextX, minX, 0), y: clamp(nextY, minY, 0) };
	}

	function setZoom(next: number): void {
		userZoom = round2(clamp(next, MIN_ZOOM, MAX_ZOOM));
		if (frameEl) {
			const clamped = clampPan(panX, panY, userZoom);
			panX = clamped.x;
			panY = clamped.y;
		}
	}

	function zoomIn(): void {
		setZoom(zoom + ZOOM_STEP);
	}
	function zoomOut(): void {
		setZoom(zoom - ZOOM_STEP);
	}
	function zoomFit(): void {
		userZoom = null;
		panX = 0;
		panY = 0;
	}

	function handleWheel(event: WheelEvent): void {
		if (event.ctrlKey || event.metaKey) {
			event.preventDefault();
			if (!frameEl) return;
			const rect = frameEl.getBoundingClientRect();
			const cx = event.clientX - rect.left;
			const cy = event.clientY - rect.top;
			const prevZoom = zoom;
			const nextZoom = clamp(prevZoom * (1 - event.deltaY * 0.0015), MIN_ZOOM, MAX_ZOOM);
			const contentX = (cx - panX) / prevZoom;
			const contentY = (cy - panY) / prevZoom;
			const clamped = clampPan(cx - contentX * nextZoom, cy - contentY * nextZoom, nextZoom);
			userZoom = round2(nextZoom);
			panX = clamped.x;
			panY = clamped.y;
			return;
		}
		if (event.shiftKey) return;
		if (Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
		event.preventDefault();
		window.scrollBy({ top: event.deltaY, left: 0, behavior: 'instant' });
	}

	const DRAG_THRESHOLD_PX = 4;
	let dragState: {
		pointerId: number;
		startX: number;
		startY: number;
		panX0: number;
		panY0: number;
	} | null = null;
	let dragging = $state(false);

	function handlePointerDown(event: PointerEvent): void {
		if (event.button !== 0) return;
		dragState = {
			pointerId: event.pointerId,
			startX: event.clientX,
			startY: event.clientY,
			panX0: panX,
			panY0: panY
		};
	}
	function handlePointerMove(event: PointerEvent): void {
		if (!dragState || dragState.pointerId !== event.pointerId) return;
		const dx = event.clientX - dragState.startX;
		const dy = event.clientY - dragState.startY;
		if (!dragging && Math.hypot(dx, dy) < DRAG_THRESHOLD_PX) return;
		if (!dragging) {
			dragging = true;
			frameEl?.setPointerCapture(event.pointerId);
		}
		const clamped = clampPan(dragState.panX0 + dx, dragState.panY0 + dy, zoom);
		panX = clamped.x;
		panY = clamped.y;
	}
	function endDrag(event: PointerEvent): void {
		if (dragState?.pointerId === event.pointerId) {
			if (dragging) frameEl?.releasePointerCapture(event.pointerId);
			dragState = null;
			dragging = false;
		}
	}
</script>

<div class="fbv" data-design="funk">
	<!-- svelte-ignore a11y_no_static_element_interactions -- pan/zoom viewport, not a semantic control; every reachable action (node select, zoom buttons) has its own focusable/interactive element -->
	<div
		class="fbv-frame"
		bind:this={frameEl}
		style="height: {frameHeightPx}px;"
		data-dragging={dragging || undefined}
		onwheel={handleWheel}
		onpointerdown={handlePointerDown}
		onpointermove={handlePointerMove}
		onpointerup={endDrag}
		onpointercancel={endDrag}
	>
		<div class="fbv-dots" aria-hidden="true"></div>
		<div
			class="fbv-inner"
			style="transform: translate({panX +
				fitOffsetX}px, {panY}px) scale({zoom}); transform-origin: 0 0;"
		>
			<div class="fbv-measure" bind:this={measureEl}>
				{@render children()}
			</div>
		</div>
	</div>

	<div
		class="fbv-zoom"
		role="group"
		aria-label="Board zoom — Ctrl + scroll also zooms, drag to pan"
		title="Ctrl + scroll to zoom · drag to pan"
	>
		<button type="button" class="fbv-zoom__btn" aria-label="Zoom out" onclick={zoomOut}>
			<Minus size={12} aria-hidden="true" />
		</button>
		<button
			type="button"
			class="fbv-zoom__btn fbv-zoom__fit"
			aria-label="Fit board to view"
			aria-pressed={isFit}
			data-active={isFit || undefined}
			onclick={zoomFit}
		>
			{zoomLabel}
		</button>
		<button type="button" class="fbv-zoom__btn" aria-label="Zoom in" onclick={zoomIn}>
			<Plus size={12} aria-hidden="true" />
		</button>
	</div>
</div>

<style>
	.fbv {
		position: relative;
	}
	.fbv-frame {
		position: relative;
		overflow: hidden;
		background: var(--surface-card);
		touch-action: none;
	}
	.fbv-frame[data-dragging] {
		cursor: grabbing;
	}
	.fbv-dots {
		position: absolute;
		inset: 0;
		pointer-events: none;
		background-image: radial-gradient(var(--border-default) 0.6px, transparent 0.7px);
		background-size: 24px 24px;
	}
	.fbv-inner {
		position: absolute;
		left: 0;
		top: 0;
		will-change: transform;
	}
	.fbv-measure {
		display: block;
		inline-size: max-content;
	}
	.fbv-zoom {
		position: absolute;
		right: 10px;
		bottom: 10px;
		display: flex;
		align-items: center;
		gap: 2px;
		padding: 2px;
		background: color-mix(in oklab, var(--surface-2) 92%, transparent);
		box-shadow: inset 0 0 0 1px var(--border-default);
	}
	.fbv-zoom__btn {
		--edge: 4px;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		height: 24px;
		min-width: 24px;
		padding: 0 8px;
		border: 0;
		background: var(--surface-2);
		color: var(--text-2);
		font-family: var(--font-mono);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		font-variant-numeric: tabular-nums;
		cursor: pointer;
	}
	.fbv-zoom__btn:hover {
		background: var(--interactive-hover-surface);
		color: var(--text-strong);
	}
	.fbv-zoom__btn:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.fbv-zoom__fit {
		min-width: 40px;
	}
	.fbv-zoom__fit[data-active] {
		background: var(--data-teal-bg);
		color: var(--data-teal-fg);
		box-shadow: inset 0 0 0 1px var(--data-teal-border);
	}
</style>
