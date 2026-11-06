<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import SetupRow from './SetupRow.svelte';

	const { Story } = defineMeta({
		title: 'Routes/Settings/setup/SetupRow',
		component: SetupRow,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	args={{
		label: 'Route',
		tone: 'warn',
		status: 'Not chosen',
		detail:
			'How Claude Code reaches mike. The sidecar route also captures Remote Control traffic. The plain route needs no certificate.',
		commands: ['mike claude-code route sidecar', 'mike claude-code route plain']
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText('Route')).toBeVisible();
		await expect(canvas.getByText('Not chosen')).toBeVisible();
		await expect(canvas.getByText('mike claude-code route sidecar')).toBeVisible();
		await expect(canvas.getByText('mike claude-code route plain')).toBeVisible();
	}}
>
	{#snippet template(args)}
		<div style="width: 40rem; background: var(--surface-card);">
			<SetupRow {...args} />
		</div>
	{/snippet}
</Story>
