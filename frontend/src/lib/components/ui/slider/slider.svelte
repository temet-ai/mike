<script lang="ts" module>
	export interface SliderProps {
		ref?: HTMLElement | null;
		id?: string;
		'aria-label'?: string;
		class?: string;
		value?: number;
		min?: number;
		max?: number;
		step?: number;
		disabled?: boolean;
		onValueChange?: (value: number) => void;
		onValueCommit?: (value: number) => void;
	}
</script>

<script lang="ts">
	import { Slider as SliderPrimitive } from 'bits-ui';
	import { cn } from '$lib/utils';

	let {
		ref = $bindable(null),
		value = $bindable(0),
		class: className,
		min = 0,
		max = 100,
		step = 1,
		disabled = false,
		onValueChange,
		onValueCommit,
		id,
		'aria-label': ariaLabel
	}: SliderProps = $props();
</script>

<SliderPrimitive.Root
	bind:ref
	bind:value
	type="single"
	{min}
	{max}
	{step}
	{disabled}
	{onValueChange}
	{onValueCommit}
	{id}
	data-slot="slider"
	class={cn('sl-root', className)}
>
	<span class="sl-track" data-slot="slider-track">
		<SliderPrimitive.Range class="sl-range" data-slot="slider-range" />
	</span>
	<SliderPrimitive.Thumb
		index={0}
		class="sl-thumb"
		data-slot="slider-thumb"
		aria-label={ariaLabel}
	/>
</SliderPrimitive.Root>

<style>
	:global(.sl-root) {
		position: relative;
		display: flex;
		align-items: center;
		width: 100%;
		min-width: calc(var(--control-h-lg) * 2);
		height: var(--control-h);
		touch-action: none;
		user-select: none;
	}
	:global(.sl-track) {
		position: relative;
		flex: 1 1 auto;
		height: var(--space-1);
		border-radius: var(--r-tol);
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
		overflow: hidden;
	}
	:global(.sl-range) {
		position: absolute;
		top: 0;
		bottom: 0;
		background: var(--funk-cyan);
	}
	:global(.sl-thumb) {
		display: block;
		width: var(--space-3);
		height: var(--space-3);
		border-radius: var(--r-tol);
		background: var(--funk-cyan);
		border: 1px solid var(--funk-cyan-border);
		box-shadow: 0 0 0 2px var(--surface-bg);
		cursor: pointer;
		transition:
			background-color var(--dur-1) var(--ease-io),
			box-shadow var(--dur-1) var(--ease-io);
	}
	:global(.sl-thumb::before) {
		content: '';
		position: absolute;
		inset: calc((var(--space-3) - var(--control-h)) / 2);
	}
	:global(.sl-thumb:hover) {
		box-shadow: var(--glow-cyan);
	}
	:global(.sl-root[data-disabled]) {
		opacity: 0.5;
		pointer-events: none;
	}
	:global(.sl-thumb:focus-visible) {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
</style>
