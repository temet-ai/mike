<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLTdAttributes } from 'svelte/elements';
	import { cn } from '$lib/utils';

	type DataTableCellProps = Omit<HTMLTdAttributes, 'class' | 'children' | 'align'> & {
		ref?: HTMLTableCellElement | null;
		class?: string;
		align?: 'start' | 'end';
		mono?: boolean;
		tone?: 'quiet';
		wrap?: boolean;
		colspan?: number;
		children?: Snippet;
	};

	let {
		ref = $bindable(null),
		class: className,
		align = 'start',
		mono = false,
		tone,
		wrap = true,
		colspan,
		children,
		...restProps
	}: DataTableCellProps = $props();
</script>

<td
	bind:this={ref}
	data-slot="data-table-cell"
	data-align={align}
	data-mono={mono ? '' : undefined}
	data-tone={tone}
	data-wrap={wrap ? '' : undefined}
	{colspan}
	class={cn('dt-cell', className)}
	{...restProps}
>
	{@render children?.()}
</td>

<style>
	.dt-cell {
		padding: var(--space-2) var(--space-3);
		text-align: left;
		vertical-align: top;
		font-size: var(--text-sm);
		color: var(--text-1);
		border-bottom: var(--hairline) solid var(--border-subtle);
	}
	.dt-cell[data-align='end'] {
		text-align: right;
	}
	.dt-cell[data-mono] {
		font-family: var(--font-mono, monospace);
		font-variant-numeric: tabular-nums;
	}
	.dt-cell[data-tone='quiet'] {
		color: var(--text-3);
	}
	.dt-cell[data-wrap] {
		overflow-wrap: anywhere;
	}
</style>
