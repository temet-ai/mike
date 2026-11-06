<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import OverviewBand from './OverviewBand.svelte';

	const { Story } = defineMeta({
		title: 'Shared/OverviewBand',
		component: OverviewBand,
		tags: ['autodocs'],
		argTypes: { children: { control: false } }
	});
</script>

<Story
	name="Default"
	args={{
		label: 'Loaded session overview',
		facts: [
			{ label: 'Sessions loaded', value: 1284 },
			{ label: 'Project groups', value: 4 }
		]
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const list = canvas.getByLabelText('Loaded session overview');
		await expect(within(list).getByText('Sessions loaded')).toBeVisible();
		await expect(within(list).getByText((1284).toLocaleString())).toBeVisible();
		await expect(within(list).getByText('4')).toBeVisible();
		await expect(canvas.getByText(/Expand a project/)).toBeVisible();
	}}
>
	{#snippet template(args)}
		<div class="bg-[var(--surface-bg)]">
			<OverviewBand label={args.label} facts={args.facts}>
				Search a title, session ID, or anything said in a conversation. Expand a project to explore
				its history.
			</OverviewBand>
		</div>
	{/snippet}
</Story>
