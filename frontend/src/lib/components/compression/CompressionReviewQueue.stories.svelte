<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within, waitFor } from 'storybook/test';
	import CompressionReviewQueue from './CompressionReviewQueue.svelte';
	import { compressionReviewFixture } from '$lib/test-support/compressionReviewFixtures';
	let record = compressionReviewFixture();
	const { Story } = defineMeta({
		title: 'Components/Compression/Queue',
		component: CompressionReviewQueue,
		beforeEach: () => {
			record = compressionReviewFixture();
			const previous = globalThis.fetch;
			globalThis.fetch = (async (_input, init) => {
				if (init?.method === 'POST') {
					const payload = JSON.parse(String(init.body));
					record.status = payload.status;
					record.history.push({ ...payload, action: 'review', at: '2026-10-04T12:01:00Z' });
					return new Response(JSON.stringify({ id: record.id, status: record.status }));
				}
				return new Response(JSON.stringify([record]));
			}) as typeof fetch;
			return () => {
				globalThis.fetch = previous;
			};
		}
	});
</script>

<Story
	name="ReviewAndFilter"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await waitFor(() => expect(canvas.getByText('1 flag')).toBeVisible());
		expect(canvas.getByRole('link', { name: 'Original capture' })).toHaveAttribute(
			'href',
			'/api/requests/473363/body/request'
		);
		await userEvent.selectOptions(canvas.getByLabelText('Review status'), 'investigating');
		await userEvent.type(canvas.getByLabelText('Review note'), 'Compare exact failure facts');
		await userEvent.click(canvas.getByRole('button', { name: 'Save review' }));
		await waitFor(() => expect(record.status).toBe('investigating'));
		await userEvent.selectOptions(canvas.getByLabelText('Filter status'), 'dismissed');
		await waitFor(() => expect(canvas.getByText(/No flags match/)).toBeVisible());
		await userEvent.selectOptions(canvas.getByLabelText('Filter status'), 'investigating');
		await waitFor(() => expect(canvas.getByText('1 flag')).toBeVisible());
	}}
/>
