<script lang="ts" module>
	export interface DataTableColumn {
		key: string;
		label: string;
		align?: 'start' | 'end';
		tooltip?: string;
	}
</script>

<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLAttributes } from 'svelte/elements';
	import { Info } from '@lucide/svelte';
	import { cn } from '$lib/utils';
	import * as TooltipUI from '$lib/components/ui/tooltip';

	type DataTableProps = Omit<HTMLAttributes<HTMLDivElement>, 'class' | 'children'> & {
		ref?: HTMLDivElement | null;
		class?: string;
		label: string;
		columns: DataTableColumn[];
		children?: Snippet;
	};

	let {
		ref = $bindable(null),
		class: className,
		label,
		columns,
		children,
		...restProps
	}: DataTableProps = $props();
</script>

<div bind:this={ref} data-slot="data-table" class={cn('dt-scroll', className)} {...restProps}>
	<table class="dt" aria-label={label}>
		<thead>
			<tr>
				{#each columns as column (column.key)}
					<th class="panel-subhd" scope="col" data-align={column.align ?? 'start'}>
						<span class="dt-th">
							<span>{column.label}</span>
							{#if column.tooltip}
								<TooltipUI.Root>
									<TooltipUI.Trigger class="dt-help" aria-label={column.tooltip}>
										<Info size={11} aria-hidden="true" />
									</TooltipUI.Trigger>
									<TooltipUI.Content side="top">{column.tooltip}</TooltipUI.Content>
								</TooltipUI.Root>
							{/if}
						</span>
					</th>
				{/each}
			</tr>
		</thead>
		{@render children?.()}
	</table>
</div>

<style>
	.dt-scroll {
		overflow-x: auto;
	}
	.dt {
		width: 100%;
		border-collapse: collapse;
	}
	.dt th {
		padding: var(--space-2) var(--space-3);
		text-align: left;
		vertical-align: bottom;
		border-bottom: var(--hairline) solid var(--border-subtle);
	}
	.dt th[data-align='end'] {
		text-align: right;
	}
	.dt-th {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
	}
	.dt-th :global(.dt-help) {
		display: inline-flex;
		padding: 0;
		border: 0;
		background: transparent;
		color: var(--text-3);
		cursor: inherit;
	}
	.dt-th :global(.dt-help:focus-visible) {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
</style>
