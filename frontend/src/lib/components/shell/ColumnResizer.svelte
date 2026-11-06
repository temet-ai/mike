<script lang="ts">
	interface Props {
		width: number;
		min: number;
		max: number;
		ariaLabel: string;
		onResize: (next: number) => void;
	}

	const { width, min, max, ariaLabel, onResize }: Props = $props();

	let dragging = $state(false);
	let startX = 0;
	let startWidth = 0;

	function onPointerDown(event: PointerEvent): void {
		if (event.button !== 0) return;
		const target = event.currentTarget as HTMLElement;
		dragging = true;
		startX = event.clientX;
		startWidth = width;
		target.setPointerCapture(event.pointerId);
		event.preventDefault();
	}

	function onPointerMove(event: PointerEvent): void {
		if (!dragging) return;
		const delta = event.clientX - startX;
		onResize(startWidth + delta);
	}

	function onPointerUp(event: PointerEvent): void {
		if (!dragging) return;
		dragging = false;
		const target = event.currentTarget as HTMLElement;
		try {
			target.releasePointerCapture(event.pointerId);
		} catch {}
	}

	function onKeyDown(event: KeyboardEvent): void {
		if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
		event.preventDefault();
		const step = event.shiftKey ? 32 : 8;
		const direction = event.key === 'ArrowRight' ? 1 : -1;
		onResize(width + direction * step);
	}
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
	class="col-resizer"
	class:dragging
	role="separator"
	aria-orientation="vertical"
	aria-label={ariaLabel}
	aria-valuenow={width}
	aria-valuemin={min}
	aria-valuemax={max}
	tabindex="0"
	onpointerdown={onPointerDown}
	onpointermove={onPointerMove}
	onpointerup={onPointerUp}
	onpointercancel={onPointerUp}
	onkeydown={onKeyDown}
></div>

<style>
	.col-resizer {
		position: absolute;
		top: 0;
		right: -3px;
		bottom: 0;
		width: 6px;
		z-index: 5;
		cursor: col-resize;
		background: transparent;
		transition: background 0.14s;
		touch-action: none;
	}
	.col-resizer:hover,
	.col-resizer:focus-visible,
	.col-resizer.dragging {
		background: linear-gradient(
			to right,
			transparent 0%,
			oklch(from var(--interactive-active-fg) l c h / 0.45) 50%,
			transparent 100%
		);
		outline: none;
	}
	.col-resizer:focus-visible {
		box-shadow: inset 0 0 0 1px var(--interactive-focus-ring);
	}
</style>
