<script lang="ts">
	import type { HTMLButtonAttributes } from 'svelte/elements';
	import { cn, type WithElementRef } from '$lib/utils';

	type SwitchProps = WithElementRef<
		Omit<HTMLButtonAttributes, 'children' | 'role' | 'type' | 'aria-checked'>,
		HTMLButtonElement
	> & {
		checked?: boolean;
		onCheckedChange?: (checked: boolean) => void;
	};

	let {
		ref = $bindable(null),
		checked = $bindable(false),
		class: className,
		disabled,
		onCheckedChange,
		onclick,
		...restProps
	}: SwitchProps = $props();

	function toggle(event: MouseEvent & { currentTarget: EventTarget & HTMLButtonElement }): void {
		onclick?.(event);
		if (event.defaultPrevented || disabled) return;
		checked = !checked;
		onCheckedChange?.(checked);
	}
</script>

<button
	bind:this={ref}
	type="button"
	role="switch"
	aria-checked={checked}
	data-slot="switch"
	data-state={checked ? 'on' : 'off'}
	data-motion="select"
	class={cn('sw', className)}
	{disabled}
	onclick={toggle}
	{...restProps}
></button>

<style>
	.sw {
		position: relative;
		display: inline-block;
		flex: 0 0 auto;
		inline-size: 36px;
		block-size: 18px;
		padding: 0;
		border: var(--hairline) solid var(--border-strong);
		border-radius: 0;
		background: var(--surface-2);
		cursor: pointer;
	}

	.sw::after {
		content: '';
		position: absolute;
		inset-block-start: 3px;
		inset-inline-start: 3px;
		inline-size: 10px;
		block-size: 10px;
		background: var(--text-3);
	}

	.sw[data-state='on'] {
		border-color: var(--interactive-active-border);
		background: var(--interactive-active-bg);
	}

	.sw[data-state='on']::after {
		background: var(--interactive-active-fg);
		transform: translateX(18px);
	}

	.sw:hover:not(:disabled) {
		border-color: var(--interactive-active-border);
	}

	.sw:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.sw:disabled {
		cursor: not-allowed;
		opacity: 0.5;
	}
</style>
