<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import WrightNote from './WrightNote.svelte';

	const { Story } = defineMeta({
		title: 'Wright/WrightNote',
		component: WrightNote,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	play={async ({ canvas }) => {
		await expect(canvas.getByRole('status')).toHaveTextContent('Nothing done yet.');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface-card);">
			<WrightNote role="status">Nothing done yet.</WrightNote>
		</div>
	{/snippet}
</Story>
