<script lang="ts">
	import type { HTMLAttributes } from 'svelte/elements';
	import { cn, type WithElementRef } from '$lib/utils';

	interface Props extends WithElementRef<HTMLAttributes<HTMLDivElement>> {
		label: string;
	}

	let { ref = $bindable(null), class: className, label, children, ...restProps }: Props = $props();
</script>

<div
	bind:this={ref}
	data-slot="message-view-subrow-detail"
	class={cn('subrow-detail', className)}
	role="region"
	aria-label={label}
	{...restProps}
>
	{@render children?.()}
</div>
