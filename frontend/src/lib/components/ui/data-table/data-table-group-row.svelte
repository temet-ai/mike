<script lang="ts">
	import type { HTMLThAttributes } from 'svelte/elements';
	import { cn } from '$lib/utils';

	type DataTableGroupRowProps = Omit<HTMLThAttributes, 'class' | 'children' | 'colspan'> & {
		ref?: HTMLTableCellElement | null;
		class?: string;
		colspan: number;
		label: string;
	};

	let {
		ref = $bindable(null),
		class: className,
		colspan,
		label,
		...restProps
	}: DataTableGroupRowProps = $props();
</script>

<tr data-slot="data-table-group-row">
	<th
		bind:this={ref}
		scope="rowgroup"
		{colspan}
		class={cn('panel-subhd dt-group', className)}
		{...restProps}>{label}</th
	>
</tr>

<style>
	.dt-group {
		padding: var(--space-3) var(--space-3) var(--space-2);
		text-align: left;
		border-bottom: var(--hairline) solid var(--border-subtle);
	}
</style>
