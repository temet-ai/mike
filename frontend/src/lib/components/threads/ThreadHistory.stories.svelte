<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import ThreadHistory from './ThreadHistory.svelte';
	import { THREAD_ENTRIES } from './threadFixtures';

	const { Story } = defineMeta({
		title: 'Threads/ThreadHistory',
		component: ThreadHistory,
		tags: ['autodocs']
	});
</script>

<Story
	name="Collapsed history, opened and read"
	args={{ entries: THREAD_ENTRIES, total: THREAD_ENTRIES.length }}
	play={async ({ canvasElement }) => {
		try {
			localStorage.removeItem('mike.threads.history-open');
		} catch {
			// Storage is a convenience only.
		}
		const canvas = within(canvasElement);
		const heading = canvas.getByRole('button', { name: /History/ });
		await expect(heading).toHaveAttribute('aria-expanded', 'false');
		await expect(canvas.queryByText(/Keep the old totals module/)).toBeNull();
		await userEvent.click(heading);
		await expect(canvas.getAllByRole('listitem')).toHaveLength(3);
		const decision = canvas.getByRole('button', { name: /Keep the old totals module/ });
		await userEvent.click(decision);
		await expect(
			canvas.getByText(/Both modules writing totals produced drift/)
		).toBeInTheDocument();
		await expect(canvas.getByRole('link', { name: /5b0c8e12/ })).toBeInTheDocument();
		await expect(canvas.getByText('No summary')).toBeInTheDocument();
	}}
/>
