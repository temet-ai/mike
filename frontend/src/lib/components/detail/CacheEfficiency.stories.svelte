<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import CacheEfficiency from './CacheEfficiency.svelte';
	import type { TokenUsage } from '$lib/types';

	const HIGH_CACHE: TokenUsage = {
		input_tokens: 4_000,
		output_tokens: 3_847,
		cache_read_input_tokens: 38_200,
		cache_creation_input_tokens: 1_204
	};

	const LOW_CACHE: TokenUsage = {
		input_tokens: 500,
		output_tokens: 200,
		cache_read_input_tokens: 100,
		cache_creation_input_tokens: 50
	};

	const { Story } = defineMeta({
		title: 'Components/CacheEfficiency',
		component: CacheEfficiency,
		tags: ['autodocs']
	});
</script>

<Story name="HighCache" args={{ usage: HIGH_CACHE }}>
	{#snippet template(args)}
		<CacheEfficiency {...args} />
	{/snippet}
</Story>

<Story name="LowCache" args={{ usage: LOW_CACHE }}>
	{#snippet template(args)}
		<CacheEfficiency {...args} />
	{/snippet}
</Story>

<Story name="MissingUsage" args={{ usage: null }}>
	{#snippet template(args)}
		<CacheEfficiency {...args} />
	{/snippet}
</Story>
