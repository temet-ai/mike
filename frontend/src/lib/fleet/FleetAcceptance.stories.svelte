<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import FleetAcceptance from './FleetAcceptance.svelte';
	import { acceptanceStatusFixture } from './__fixtures__/acceptance';

	const { Story } = defineMeta({
		title: 'Fleet/Acceptance',
		component: FleetAcceptance
	});
</script>

<script lang="ts">
	let status = $state(acceptanceStatusFixture());

	function refresh(): void {
		const next = acceptanceStatusFixture();
		next.accepted = true;
		next.now = '2026-10-04T14:45:00Z';
		const task = next.tasks[0];
		const criterion = next.criteria[0];
		if (task) task.passed_criteria = ['C1', 'C2'];
		if (criterion) criterion.passed = 1;
		const journey = next.obligations?.[0];
		if (journey) {
			journey.outcome = 'accepted';
			journey.body = 'The new build is running and the journey was exercised independently.';
			journey.data = {
				expected:
					'Selecting the command search result opens the requested setting and keeps it visible.',
				observation:
					'Opened command search, selected the result, and confirmed the requested setting was visible.',
				evidence: ['Browser observation: setting-journey-verified']
			};
		}
		status = next;
	}
</script>

<Story
	name="Readiness and acceptance"
	play={async ({ canvas, canvasElement }) => {
		const acceptanceState = canvasElement.querySelector(
			'[data-slot="fleet-acceptance-state"]'
		) as HTMLElement;
		await expect(acceptanceState).toHaveTextContent('Outstanding');
		await expect(canvas.getByText('Ready for dependents', { exact: true })).toBeInTheDocument();
		await expect(canvas.getByText('1 / 2 criteria accepted')).toBeInTheDocument();
		await expect(canvas.getByText('Work state: done')).toBeInTheDocument();
		const journey = canvasElement.querySelector(
			'[data-obligation-id="setting-journey"]'
		) as HTMLElement;
		await expect(within(journey).getByText('Expected outcome')).toBeInTheDocument();
		await expect(within(journey).getByText('Observed outcome')).toBeInTheDocument();
		await expect(journey.querySelector('[data-slot="obligation-observed"]')).toHaveTextContent(
			'has not exercised the change'
		);
		await expect(
			journey.querySelector('[data-slot="obligation-record-details"]')
		).not.toHaveAttribute('open');
		await userEvent.click(within(journey).getByText('Record details'));
		await expect(journey.querySelector('[data-slot="obligation-record"]')).toBeVisible();
		await expect(journey.querySelector('[data-slot="obligation-record"]')).toHaveTextContent(
			'Serve the worker build and assign a fresh journey acceptance session.'
		);
		await expect(canvasElement.querySelectorAll('[data-slot="fleet-obligation"]')).toHaveLength(3);
		await userEvent.click(canvas.getByRole('button', { name: 'Refresh fleet acceptance' }));
		await expect(acceptanceState).toHaveTextContent('Accepted');
		await expect(canvas.getByText('2 / 2 criteria accepted')).toBeInTheDocument();
		await expect(journey.querySelector('[data-slot="obligation-observed"]')).toHaveTextContent(
			'confirmed the requested setting was visible'
		);
		await expect(canvasElement.querySelectorAll('[data-slot="fleet-obligation"]')).toHaveLength(3);
		await expect(canvas.getByText('Advisory · search-wording')).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="max-w-lg p-4">
			<FleetAcceptance {status} loading={false} error={null} onRefresh={refresh} />
		</div>
	{/snippet}
</Story>
