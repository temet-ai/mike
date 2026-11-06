<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import HomePageHarness from './HomePageHarness.svelte';

	const { Story } = defineMeta({
		title: 'Routes/Home',
		component: HomePageHarness,
		tags: ['autodocs'],
		parameters: {
			layout: 'fullscreen',
			docs: {
				description: {
					component:
						'Home (/) — a live Pulse strip, a compact band of live sessions, and three destination-card groups (Insight/Levers/System) teaching every capability.'
				}
			}
		}
	});
</script>

<Story
	name="Populated"
	tags={['data-driven', 'interactive']}
	args={{ scenario: 'populated' }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(canvas.getByRole('heading', { name: 'Home' })).toBeInTheDocument();
		await expect(canvas.getByText('ONLINE')).toBeInTheDocument();
		await expect(canvas.getByText('4d 02h')).toBeInTheDocument();

		await expect(canvas.getByRole('heading', { name: 'Sessions' })).toBeInTheDocument();
		await expect(canvas.getAllByText('Implement auth middleware').length).toBeGreaterThan(0);
		await expect(canvas.getAllByText('Refactor DB migrations').length).toBeGreaterThan(0);
		await expect(canvas.queryByText('Old finished session')).toBeNull();

		await expect(canvas.getByRole('heading', { name: 'Insight' })).toBeInTheDocument();
		await expect(canvas.getByRole('heading', { name: 'Levers' })).toBeInTheDocument();
		await expect(canvas.getByRole('heading', { name: 'System' })).toBeInTheDocument();
		const sessionsCard = canvas
			.getAllByText('Sessions')
			.map((el) => el.closest('[data-slot="destination-card"]'))
			.find((el): el is HTMLElement => el !== null);
		await expect(sessionsCard).toBeTruthy();
		await expect(canvas.getByText('Steering')).toBeInTheDocument();
		await expect(canvas.getByText('Settings')).toBeInTheDocument();

		const fleetCard = canvas
			.getByText('Fleet')
			.closest('[data-slot="destination-card"]') as HTMLElement;
		await expect(within(fleetCard).queryByText('How it works')).toBeNull();
		await expect(within(fleetCard).getByRole('link', { name: /open/i })).toHaveAttribute(
			'href',
			'/fleet'
		);

		const openLinks = canvas.getAllByRole('link', { name: /open/i });
		const hrefs = openLinks.map((a) => a.getAttribute('href')).sort();
		await expect(hrefs).toEqual(
			[
				'/agents',
				'/analysis',
				'/diagnostics',
				'/filter/workbench',
				'/fleet',
				'/help',
				'/memory',
				'/projects',
				'/prompts',
				'/sessions',
				'/settings',
				'/skills',
				'/steerers',
				'/steering',
				'/tools'
			].sort()
		);

		await expect(document.body.querySelector('[data-slot="tour-popover"]')).toBeNull();
		await userEvent.click(canvas.getByRole('button', { name: /take a tour/i }));
		const tour = (await waitFor(() => {
			const d = document.body.querySelector('[data-slot="tour-popover"]');
			if (!d) throw new Error('tour did not open');
			return d as HTMLElement;
		})) as HTMLElement;
		await expect(tour).toHaveAttribute('role', 'dialog');
		await expect(within(tour).getByText('The live pulse')).toBeInTheDocument();
		await expect(within(tour).getByText('Step 1 of 6')).toBeInTheDocument();
		await waitFor(() => {
			const ring = document.body.querySelector('[data-slot="tour-spotlight"]');
			const pulse = canvasElement.querySelector('[data-slot="pulse-strip"]');
			if (!ring || !pulse) throw new Error('spotlight or pulse strip missing');
			const r = ring.getBoundingClientRect();
			const p = pulse.getBoundingClientRect();
			if (r.left > p.left || r.top > p.top || r.right < p.right || r.bottom < p.bottom)
				throw new Error('spotlight does not cover the pulse strip');
		});
		await userEvent.keyboard('{ArrowRight}');
		await waitFor(() => within(tour).getByText('Start from a task'));
		await userEvent.keyboard('{Escape}');
		await waitFor(() => {
			if (document.body.querySelector('[data-slot="tour-popover"]'))
				throw new Error('tour still open after Escape');
		});
	}}
>
	{#snippet template(args)}
		<div class="h-[900px] w-[1100px] overflow-hidden bg-[var(--surface-bg)]">
			<HomePageHarness {...args} />
		</div>
	{/snippet}
</Story>
