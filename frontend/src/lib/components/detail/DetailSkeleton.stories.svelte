<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import DetailSkeleton from './DetailSkeleton.svelte';
	import 'augmented-ui/augmented-ui.min.css';

	const { Story } = defineMeta({
		title: 'Components/Detail/DetailSkeleton',
		component: DetailSkeleton,
		tags: ['autodocs']
	});
</script>

<Story name="Default">
	{#snippet template()}
		<div class="p-4" style="max-width: 480px;">
			<DetailSkeleton />
		</div>
	{/snippet}
</Story>
