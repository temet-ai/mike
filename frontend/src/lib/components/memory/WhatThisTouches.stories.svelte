<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect } from 'storybook/test';
	import WhatThisTouches from './WhatThisTouches.svelte';

	const { Story } = defineMeta({
		title: 'Memory/WhatThisTouches',
		component: WhatThisTouches,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText(/your files are not altered/i)).toBeInTheDocument();
		await expect(canvas.getByText('source="proxy-lessons"', { exact: false })).toBeInTheDocument();
		await expect(canvas.getByText(/daily/)).toBeInTheDocument();
		await expect(canvas.getByText(/long_term/)).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div style="max-width: 720px; padding: 12px;">
			<WhatThisTouches />
		</div>
	{/snippet}
</Story>
