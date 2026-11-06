<script lang="ts">
	import { Tabs as TabsPrimitive } from 'bits-ui';
	import { cn } from '$lib/utils';

	let { ref = $bindable(null), class: className, ...restProps }: TabsPrimitive.ListProps = $props();
</script>

<TabsPrimitive.List
	bind:ref
	data-slot="tabs-list"
	class={cn(
		'group/tabs-list inline-flex h-9 max-w-full w-fit items-center justify-start gap-4 overflow-x-auto overflow-y-hidden overscroll-x-contain border-b border-border text-sm text-muted-foreground',
		'group-data-[orientation=vertical]/tabs:h-fit group-data-[orientation=vertical]/tabs:flex-col',
		className
	)}
	{...restProps}
/>
