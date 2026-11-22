<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import { actionFixture, remedies } from '../__fixtures__/wright';
	import WrightFixRow from './WrightFixRow.svelte';

	const { Story } = defineMeta({
		title: 'Wright/WrightFixRow',
		component: WrightFixRow,
		tags: ['autodocs']
	});

	const [paused] = remedies() as [ReturnType<typeof remedies>[number]];
	const active = { ...paused, paused_at: null, pause_reason: null };
	const ACTIONS = [actionFixture({ id: 1 }), actionFixture({ id: 2 })];
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText('Better')).toBeVisible();
		await expect(canvas.getByText(/2 cleared the problem, 0 did not/)).toBeVisible();
		await expect(canvas.getByText('Paused')).toBeVisible();
		await expect(canvas.getByText('Paused at your request until the fleet closes.')).toBeVisible();
	}}
>
	{#snippet template()}
		<ul
			class="edge"
			style="list-style: none; margin: 0; padding: 0; max-inline-size: 44rem; --edge-surface: var(--surface-card);"
		>
			<WrightFixRow remedy={active} actions={ACTIONS} />
			<WrightFixRow remedy={paused} actions={[]} />
		</ul>
	{/snippet}
</Story>
