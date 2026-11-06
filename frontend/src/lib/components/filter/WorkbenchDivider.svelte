<script lang="ts">
	import { keyboardResizeDelta } from './workbenchLayout';

	interface Props {
		orientation: 'vertical' | 'horizontal';
		label: string;
		/** The pane size this divider controls, in pixels — required by role="separator". */
		value: number;
		min: number;
		max: number;
		/** Positive deltaPx = pointer/key moved right (vertical) or down (horizontal). */
		onResize: (deltaPx: number) => void;
	}

	let { orientation, label, value, min, max, onResize }: Props = $props();

	let dragging = $state(false);
	let lastPos = 0;

	function posOf(e: PointerEvent): number {
		return orientation === 'vertical' ? e.clientX : e.clientY;
	}

	function onPointerDown(e: PointerEvent): void {
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
		dragging = true;
		lastPos = posOf(e);
	}

	function onPointerMove(e: PointerEvent): void {
		if (!dragging) return;
		const pos = posOf(e);
		const delta = pos - lastPos;
		lastPos = pos;
		if (delta !== 0) onResize(delta);
	}

	function endDrag(): void {
		dragging = false;
	}

	function onKeydown(e: KeyboardEvent): void {
		const delta = keyboardResizeDelta(e.key);
		if (delta === 0) return;
		e.preventDefault();
		onResize(delta);
	}
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -- a resizable splitter (role="separator") is a documented WAI-ARIA pattern that is keyboard-focusable and draggable, unlike a plain non-interactive separator -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -- pointer + keyboard handlers implement the splitter's own drag/arrow-key resize; there is no more specific interactive role for a resize handle -->
<div
	class="wb-divider"
	class:wb-divider--vertical={orientation === 'vertical'}
	class:wb-divider--horizontal={orientation === 'horizontal'}
	class:wb-divider--dragging={dragging}
	role="separator"
	aria-orientation={orientation}
	aria-label={label}
	aria-valuenow={value}
	aria-valuemin={min}
	aria-valuemax={max}
	tabindex="0"
	data-slot="workbench-divider"
	onpointerdown={onPointerDown}
	onpointermove={onPointerMove}
	onpointerup={endDrag}
	onpointercancel={endDrag}
	onkeydown={onKeydown}
>
	<div class="wb-divider-grip" aria-hidden="true"></div>
</div>

<style>
	.wb-divider {
		flex-shrink: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		background: transparent;
		touch-action: none;
	}
	.wb-divider--vertical {
		width: 7px;
		cursor: col-resize;
	}
	.wb-divider--horizontal {
		height: 7px;
		cursor: row-resize;
	}
	.wb-divider:hover .wb-divider-grip,
	.wb-divider--dragging .wb-divider-grip,
	.wb-divider:focus-visible .wb-divider-grip {
		background: var(--funk-cyan);
	}
	.wb-divider:focus-visible {
		outline: none;
	}
	.wb-divider-grip {
		background: var(--border-strong);
		transition: background 120ms ease;
	}
	.wb-divider--vertical .wb-divider-grip {
		width: 1px;
		height: 40px;
	}
	.wb-divider--horizontal .wb-divider-grip {
		width: 40px;
		height: 1px;
	}
</style>
