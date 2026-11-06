<script lang="ts">
	import { Tabs as TabsPrimitive } from 'bits-ui';
	import { cn } from '$lib/utils';

	let {
		ref = $bindable(null),
		class: className,
		...restProps
	}: TabsPrimitive.TriggerProps = $props();
</script>

<TabsPrimitive.Trigger
	bind:ref
	data-slot="tabs-trigger"
	data-motion="select"
	data-augmented-ui="tl-clip border"
	class={cn(
		"relative inline-flex h-9 shrink-0 items-center justify-center gap-2 whitespace-nowrap px-2 text-sm font-medium text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[var(--ring)] touch-manipulation disabled:pointer-events-none disabled:opacity-50 [&_svg:not([class*='size-'])]:size-4 [&_svg]:pointer-events-none [&_svg]:shrink-0",
		'data-[state=active]:text-[var(--interactive-active-fg)] data-[state=active]:bg-[var(--interactive-active-bg)]',
		className
	)}
	{...restProps}
/>

<style>
	:global([data-slot='tabs-trigger'][data-augmented-ui]) {
		--aug-tl: 5px;
		--aug-border-all: 1px;
		--aug-border-bg: transparent;
	}
	:global([data-slot='tabs-trigger'][data-augmented-ui][data-state='active']) {
		--aug-border-bg: var(--interactive-active-border);
	}
	:global([data-slot='tabs-trigger'][data-augmented-ui]:focus-visible) {
		--aug-border-bg: var(--interactive-focus-ring);
	}
</style>
