<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import CarrierRef from './CarrierRef.svelte';
	import {
		fileCarrier,
		lessonCarrier
	} from '$lib/test-support/__fixtures__/knowledgeConsolidation';

	const { Story } = defineMeta({
		title: 'Components/Memory/CarrierRef',
		component: CarrierRef,
		tags: ['autodocs']
	});
</script>

<Story
	name="Carriers"
	play={async ({ canvas }) => {
		await expect(canvas.getByRole('link', { name: 'Lesson 2733' })).toHaveAttribute(
			'href',
			'/sessions/improvement/lessons/2733'
		);
		await expect(canvas.getByText(/claude-memory · memory\/capture-db.md/)).toBeInTheDocument();
		await expect(canvas.getByText(/machine-wide/)).toBeInTheDocument();
		await expect(canvas.getByText('lesson:old-row')).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="flex w-[420px] flex-col gap-2">
			<CarrierRef carrier={lessonCarrier(2733, '2026-11-18T09:00:00Z')} />
			<CarrierRef
				carrier={fileCarrier(
					'claude-memory',
					'memory/capture-db.md',
					'2026-11-19T10:00:00Z',
					'machine'
				)}
			/>
			<CarrierRef carrier={null} reference="lesson:old-row" />
		</div>
	{/snippet}
</Story>
