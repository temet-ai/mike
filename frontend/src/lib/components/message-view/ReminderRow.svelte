<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLAttributes } from 'svelte/elements';
	import { cn, type WithElementRef } from '$lib/utils';

	interface Props extends WithElementRef<HTMLAttributes<HTMLDivElement>> {
		children: Snippet;
	}

	let { ref = $bindable(null), class: className, children, ...restProps }: Props = $props();
</script>

<div bind:this={ref} data-slot="message-view-row" class={cn('row', className)} {...restProps}>
	<span class="chev-spacer" aria-hidden="true"></span>
	{@render children()}
</div>

<style>
	:global(.row .chev-spacer) {
		flex: 0 0 12px;
		width: 12px;
		height: 12px;
	}
</style>
