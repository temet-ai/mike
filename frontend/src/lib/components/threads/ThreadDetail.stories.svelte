<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import ThreadDetail from './ThreadDetail.svelte';
	import { THREAD_DETAIL } from './threadFixtures';

	const { Story } = defineMeta({
		title: 'Threads/ThreadDetail',
		component: ThreadDetail,
		tags: ['autodocs']
	});
</script>

<Story
	name="Thread with state, criteria, dependencies and sessions"
	args={{ detail: THREAD_DETAIL, hrefFor: (slug: string) => `/threads?thread=${slug}` }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(
			canvas.getByRole('heading', { level: 2, name: 'Checkout rewrite' })
		).toBeInTheDocument();
		await expect(canvas.getByText('feat/pricing-service-totals')).toBeInTheDocument();
		const state = within(canvas.getByRole('region', { name: 'State' }));
		await expect(state.getByText(/Three of five coupon types are ported/)).toBeInTheDocument();
		await expect(state.getByText('checkout/totals.ts')).toBeInTheDocument();
		const criteria = within(canvas.getByRole('region', { name: 'Acceptance criteria' }));
		await expect(criteria.getAllByRole('listitem')).toHaveLength(2);
		await expect(canvas.getByRole('link', { name: 'pricing-service' })).toHaveAttribute(
			'href',
			'/threads?thread=pricing-service'
		);
		await expect(canvas.getByRole('link', { name: 'search-latency' })).toBeInTheDocument();
		const sessions = within(canvas.getByRole('region', { name: 'Sessions' }));
		await expect(sessions.getByText('Ported the first three coupon types.')).toBeInTheDocument();
		await expect(sessions.getAllByRole('link')).toHaveLength(2);
		await expect(canvas.getByRole('button', { name: /History/ })).toBeInTheDocument();
	}}
/>
