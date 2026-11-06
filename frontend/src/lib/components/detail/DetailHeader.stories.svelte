<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import DetailHeader from './DetailHeader.svelte';
	import { buildRequestDetail } from '$lib/components/__fixtures__/request';

	const now = Date.now();

	const { Story } = defineMeta({
		title: 'Components/Detail/DetailHeader',
		component: DetailHeader,
		tags: ['autodocs']
	});
</script>

<Story name="Typical">
	{#snippet template()}
		<DetailHeader
			detail={buildRequestDetail()}
			{now}
			sessionId="a1b2c3d4-e5f6-7890-abcd-ef1234567890"
		/>
	{/snippet}
</Story>
