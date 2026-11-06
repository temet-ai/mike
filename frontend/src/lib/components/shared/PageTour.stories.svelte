<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, userEvent, expect, waitFor } from 'storybook/test';
	import PageTour from './PageTour.svelte';

	const { Story } = defineMeta({
		title: 'Shared/PageTour',
		component: PageTour,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' }
	});

	function popover(): HTMLElement {
		const d = document.body.querySelector<HTMLElement>('[data-slot="tour-popover"]');
		if (!d) throw new Error('tour popover is not open');
		return d;
	}

	function spotlightCovers(targetSelector: string): void {
		const ring = document.body.querySelector<HTMLElement>('[data-slot="tour-spotlight"]');
		const target = document.querySelector<HTMLElement>(targetSelector);
		if (!ring || !target) throw new Error('spotlight or target missing');
		const r = ring.getBoundingClientRect();
		const t = target.getBoundingClientRect();
		const inside = r.left <= t.left && r.top <= t.top && r.right >= t.right && r.bottom >= t.bottom;
		if (!inside) throw new Error(`spotlight does not cover ${targetSelector}`);
	}
</script>

<script lang="ts">
	import type { TourStep } from './PageTour.svelte';

	const steps: TourStep[] = [
		{
			target: '[data-demo="search"]',
			title: 'Search everything',
			body: 'Type a prompt, a model or a session id to find it.'
		},
		{
			target: '[data-demo="missing"]',
			title: 'Skipped step',
			body: 'This target does not exist, so the tour drops the step.'
		},
		{
			target: '[data-demo="filters"]',
			title: 'Narrow the list',
			body: 'Filters keep only the rows you care about.',
			placement: 'right'
		},
		{
			title: 'That is it',
			body: 'A step without a target is centred with no spotlight.'
		}
	];

	let open = $state(false);
</script>

{#snippet demoPage()}
	<div style="padding: 2rem; display: grid; gap: 2rem; grid-template-columns: 200px 1fr;">
		<button type="button" data-demo="trigger" onclick={() => (open = true)}>Take a tour</button>
		<input data-demo="search" placeholder="Search" style="grid-column: 2;" />
		<div data-demo="filters" style="height: 240px; border: 1px dashed var(--border-strong);">
			Filters
		</div>
		<div style="height: 240px; border: 1px dashed var(--border-strong);">Results</div>
	</div>
	<PageTour bind:open {steps} label="Demo tour" />
{/snippet}

<Story
	name="Spotlight walkthrough"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const trigger = canvas.getByRole('button', { name: /take a tour/i });

		await expect(document.body.querySelector('[data-slot="tour-popover"]')).toBeNull();
		await userEvent.click(trigger);

		const first = await waitFor(popover);
		await expect(first).toHaveAttribute('role', 'dialog');
		await expect(within(first).getByText('Search everything')).toBeTruthy();
		await expect(within(first).getByText('Step 1 of 3')).toBeTruthy();
		await expect(within(first).getByRole('button', { name: /^back$/i })).toBeDisabled();
		await waitFor(() => spotlightCovers('[data-demo="search"]'));
		await waitFor(() => {
			if (document.activeElement !== popover()) throw new Error('popover did not take focus');
		});

		await userEvent.keyboard('{ArrowRight}');
		await waitFor(() => within(popover()).getByText('Narrow the list'));
		await expect(within(popover()).getByText('Step 2 of 3')).toBeTruthy();
		await expect(popover()).toHaveAttribute('data-side', 'right');
		await waitFor(() => spotlightCovers('[data-demo="filters"]'));

		await userEvent.click(within(popover()).getByRole('button', { name: /^next$/i }));
		await waitFor(() => within(popover()).getByText('That is it'));
		await expect(document.body.querySelector('[data-slot="tour-spotlight"]')).toBeNull();
		await expect(within(popover()).queryByRole('button', { name: /skip tour/i })).toBeNull();

		await userEvent.click(within(popover()).getByRole('button', { name: /^done$/i }));
		await waitFor(() => {
			if (document.body.querySelector('[data-slot="tour-popover"]'))
				throw new Error('tour still open after Done');
		});
		await waitFor(() => {
			if (document.activeElement !== trigger) throw new Error('focus did not return to trigger');
		});
	}}
>
	{#snippet template()}
		{@render demoPage()}
	{/snippet}
</Story>
