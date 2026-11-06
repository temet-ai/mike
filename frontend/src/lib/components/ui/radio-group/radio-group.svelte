<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLAttributes } from 'svelte/elements';
	import { cn } from '$lib/utils';
	import { setRadioGroupContext } from './context';

	type RadioGroupProps = Omit<HTMLAttributes<HTMLDivElement>, 'children'> & {
		ref?: HTMLDivElement | null;
		name: string;
		value?: string;
		disabled?: boolean;
		onValueChange?: (value: string) => void;
		children?: Snippet;
	};

	let {
		ref = $bindable(null),
		class: className,
		name,
		value = $bindable(''),
		disabled = false,
		onValueChange,
		children,
		...restProps
	}: RadioGroupProps = $props();

	setRadioGroupContext({
		get name() {
			return name;
		},
		get value() {
			return value;
		},
		get disabled() {
			return disabled;
		},
		select(next: string) {
			value = next;
			onValueChange?.(next);
		}
	});
</script>

<div
	bind:this={ref}
	role="radiogroup"
	data-slot="radio-group"
	class={cn('rg', className)}
	{...restProps}
>
	{@render children?.()}
</div>

<style>
	.rg {
		display: flex;
		flex-direction: column;
		min-block-size: 0;
		min-inline-size: 0;
	}
</style>
