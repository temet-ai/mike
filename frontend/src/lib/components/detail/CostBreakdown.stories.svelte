<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import CostBreakdown from './CostBreakdown.svelte';
	import type { TokenUsage } from '$lib/types';

	const FULL_USAGE: TokenUsage = {
		input_tokens: 4_000,
		output_tokens: 3_847,
		cache_read_input_tokens: 38_200,
		cache_creation_input_tokens: 1_204
	};

	const { Story } = defineMeta({
		title: 'Components/Detail/CostBreakdown',
		component: CostBreakdown,
		tags: ['autodocs']
	});
</script>

<Story name="FullUsage" args={{ usage: FULL_USAGE, model: 'claude-opus-4-6' }}>
	{#snippet template(args)}
		<CostBreakdown {...args} />
	{/snippet}
</Story>
