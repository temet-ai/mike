<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import Separator from './separator.svelte';

	const { Story } = defineMeta({
		title: 'UI/Separator',
		component: Separator,
		tags: ['autodocs']
	});
</script>

<Story name="Horizontal">
	{#snippet template()}
		<div class="flex w-64 flex-col gap-2">
			<span class="text-sm text-[var(--text-1)]">Above</span>
			<Separator orientation="horizontal" />
			<span class="text-sm text-[var(--text-1)]">Below</span>
		</div>
	{/snippet}
</Story>

<Story name="Vertical">
	{#snippet template()}
		<div class="flex h-8 items-center gap-2">
			<span class="text-sm text-[var(--text-1)]">Left</span>
			<Separator orientation="vertical" />
			<span class="text-sm text-[var(--text-1)]">Right</span>
		</div>
	{/snippet}
</Story>
