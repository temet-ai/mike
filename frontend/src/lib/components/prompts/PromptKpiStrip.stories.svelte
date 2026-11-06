<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import PromptKpiStrip from './PromptKpiStrip.svelte';

	const { Story } = defineMeta({
		title: 'Routes/Prompts/PromptKpiStrip',
		tags: ['autodocs']
	});
</script>

<Story name="Catalogue">
	<PromptKpiStrip registered={2} observed={22} unregistered={20} drifted={1} windowLabel="7d" />
</Story>
