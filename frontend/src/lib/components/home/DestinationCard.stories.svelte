<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import DestinationCard from './DestinationCard.svelte';

	const { Story } = defineMeta({
		title: 'Home/DestinationCard',
		component: DestinationCard,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	args={{
		group: 'levers',
		tag: 'Steering',
		whatItIs:
			'Watches live sessions and injects a correction when a session claims "done" falsely or drifts off task.',
		why: [
			'A session said it finished but didn’t → catch it in the act',
			'A session drifted off task → nudge it back onto its actual ask'
		],
		stat: { value: '4', label: 'steerers enrolled' },
		openHref: '/steering',
		howItWorksHref: '/help/guides/steering-concepts'
	}}
	play={async ({ canvasElement, args }) => {
		const canvas = within(canvasElement);

		await expect(canvas.getByText(args.tag, { exact: false })).toBeInTheDocument();
		await expect(canvas.getByText(args.whatItIs)).toBeInTheDocument();
		await expect(canvas.getByText(args.why[0])).toBeInTheDocument();
		await expect(canvas.getByText(args.why[1])).toBeInTheDocument();
		await expect(canvas.getByText('4')).toBeInTheDocument();
		await expect(canvas.getByText('steerers enrolled', { exact: false })).toBeInTheDocument();

		const openLink = canvas.getByRole('link', { name: /open/i });
		await expect(openLink).toHaveAttribute('href', '/steering');
		const howLink = canvas.getByRole('link', { name: /how steering works/i });
		await expect(howLink).toHaveAttribute('href', '/help/guides/steering-concepts');
	}}
>
	{#snippet template(args)}
		<div class="w-[320px]">
			<DestinationCard {...args} />
		</div>
	{/snippet}
</Story>
