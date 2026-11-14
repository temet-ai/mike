<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLAttributes } from 'svelte/elements';
	import { cn } from '$lib/utils';

	type DataTableRowProps = Omit<HTMLAttributes<HTMLTableRowElement>, 'class' | 'children'> & {
		ref?: HTMLTableRowElement | null;
		class?: string;
		children?: Snippet;
	};

	let {
		ref = $bindable(null),
		class: className,
		children,
		...restProps
	}: DataTableRowProps = $props();
</script>

<tr
	bind:this={ref}
	data-slot="data-table-row"
	data-motion="hover"
	class={cn('dt-row', className)}
	{...restProps}
>
	{@render children?.()}
</tr>

<style>
	.dt-row:hover {
		background: var(--surface-2);
	}
</style>
