<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import ThreadList from './ThreadList.svelte';
	import { THREAD_ROWS } from './threadFixtures';

	const { Story } = defineMeta({
		title: 'Threads/ThreadList',
		component: ThreadList,
		tags: ['autodocs']
	});
</script>

<Story
	name="Threads of a project"
	args={{
		threads: THREAD_ROWS,
		selected: 'search-latency',
		hrefFor: (slug: string) => `/threads?thread=${slug}`
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const nav = canvas.getByRole('navigation', { name: 'Threads' });
		await expect(within(nav).getAllByRole('link')).toHaveLength(3);
		await expect(canvas.getByRole('link', { name: /Search latency/ })).toHaveAttribute(
			'aria-current',
			'page'
		);
		await expect(canvas.getByText('Implementing')).toBeInTheDocument();
		await expect(
			canvas.getByText(/worklog.md: heading "Where we are" appears twice/)
		).toBeInTheDocument();
		await expect(
			canvas.getByText(
				'Cart totals move to the pricing service; the old totals module is read-only until the cutover.'
			)
		).toBeInTheDocument();
	}}
/>
