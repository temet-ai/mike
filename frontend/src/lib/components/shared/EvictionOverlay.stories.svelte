<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import EvictionOverlay from './EvictionOverlay.svelte';

	const { Story } = defineMeta({
		title: 'Shared/EvictionOverlay',
		component: EvictionOverlay,
		tags: ['autodocs']
	});
</script>

<Story name="Empty">
	{#snippet template()}
		<div class="relative h-[320px] w-full bg-[var(--surface-bg)]">
			<p class="p-4 text-[var(--text-2)] text-sm">
				The eviction pill is fixed to the bottom-right of the viewport. With no cache pressure it
				reads "No evictions". The populated reason→count list appears under live proxy eviction
				pressure.
			</p>
			<EvictionOverlay />
		</div>
	{/snippet}
</Story>
