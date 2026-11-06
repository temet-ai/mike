<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import PulseStrip from './PulseStrip.svelte';

	const { Story } = defineMeta({
		title: 'Home/PulseStrip',
		component: PulseStrip,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	args={{
		uptimeSeconds: 4 * 86_400 + 2 * 3_600,
		requestCount: 128_430,
		sessionsStored: 812,
		hasMoreSessions: true,
		activeStreams: 2,
		activeSessionsCount: 3
	}}
	play={async ({ canvasElement, args }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText('ONLINE')).toBeInTheDocument();
		await expect(canvas.getByText('4d 02h')).toBeInTheDocument();
		await expect(canvas.getByText('128,430')).toBeInTheDocument();
		await expect(canvas.getByText('812+')).toBeInTheDocument();
		await expect(canvas.getByText('3 active sessions', { exact: false })).toBeInTheDocument();
		const link = canvasElement.querySelector('a[data-slot="pulse-strip"]');
		await expect(link).toHaveAttribute('href', '/sessions');
	}}
>
	{#snippet template(args)}
		<div class="w-[820px]">
			<PulseStrip {...args} />
		</div>
	{/snippet}
</Story>
