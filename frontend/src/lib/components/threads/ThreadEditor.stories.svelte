<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import ThreadEditor from './ThreadEditor.svelte';
	import { THREAD_DETAIL } from './threadFixtures';

	const onPatch = fn();
	const onCheckpoint = fn();
	const onEntry = fn();

	const { Story } = defineMeta({
		title: 'Threads/ThreadEditor',
		component: ThreadEditor,
		tags: ['autodocs']
	});
</script>

<Story
	name="Change details, checkpoint and add an entry"
	args={{
		detail: THREAD_DETAIL,
		error: 'Acceptance criteria are required before a thread moves to review',
		onPatch,
		onCheckpoint,
		onEntry,
		onClose: fn()
	}}
	play={async ({ canvasElement }) => {
		onPatch.mockClear();
		onCheckpoint.mockClear();
		onEntry.mockClear();
		const canvas = within(canvasElement);
		await expect(
			canvas.getByText('Acceptance criteria are required before a thread moves to review')
		).toBeInTheDocument();

		const title = canvas.getByLabelText('Title');
		await userEvent.clear(title);
		await userEvent.type(title, 'Checkout rewrite, phase two');
		await userEvent.click(canvas.getByRole('button', { name: 'Save details' }));
		await expect(onPatch).toHaveBeenCalledWith({ title: 'Checkout rewrite, phase two' });

		await userEvent.click(canvas.getByRole('tab', { name: 'Checkpoint' }));
		const summary = within(canvas.getByRole('tabpanel')).getByLabelText(/Summary/);
		await userEvent.clear(summary);
		await userEvent.type(summary, 'Bundle coupons ported');
		await userEvent.click(canvas.getByRole('button', { name: 'Write checkpoint' }));
		await expect(onCheckpoint).toHaveBeenCalledWith(
			expect.objectContaining({
				slug: 'checkout-rewrite',
				tldr: 'Bundle coupons ported',
				files: ['services/pricing/discounts.ts', 'checkout/totals.ts']
			})
		);

		await userEvent.click(canvas.getByRole('tab', { name: 'Add entry' }));
		await expect(canvas.getByRole('button', { name: 'Add entry' })).toBeDisabled();
		await userEvent.type(
			within(canvas.getByRole('tabpanel')).getByLabelText('Body'),
			'Staging flag switched on for 5%.'
		);
		await userEvent.click(canvas.getByRole('button', { name: 'Add entry' }));
		await expect(onEntry).toHaveBeenCalledWith({
			kind: 'note',
			body: 'Staging flag switched on for 5%.',
			summary: undefined
		});
	}}
/>
