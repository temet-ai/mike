<!--
"What you keep saying": four groups of things the operator has told Claude over and over, each with
its weekly bars and every message in full, a link back to each session and a button that opens a
prefilled issue. Empty and failed loads are covered in repeatedAsks.test.ts.
-->
<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import RepeatedAsks from './RepeatedAsks.svelte';
	import { REPEATS, REPEATS_NOW } from '$lib/test-support/repeatedAsksFixtures';

	const { Story } = defineMeta({
		title: 'Improvement/RepeatedAsks',
		component: RepeatedAsks,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	args={{ repeats: REPEATS, error: null, onRetry: () => {}, now: REPEATS_NOW }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(canvas.getByText(/From 1745 of your messages/)).toBeVisible();
		const articles = canvasElement.querySelectorAll('article');
		await expect(articles).toHaveLength(REPEATS.groups.length);

		for (const group of REPEATS.groups) {
			for (const item of group.items) {
				await expect(canvasElement.textContent).toContain(item.text);
			}
		}

		const sessionLinks = canvas.getAllByRole('link', { name: /Open the session/ });
		const items = REPEATS.groups.flatMap((g) => g.items);
		await expect(sessionLinks).toHaveLength(items.length);
		sessionLinks.forEach((link, i) => {
			expect(link.getAttribute('href')).toBe(`/sessions/${items[i]!.session_id}`);
		});

		const issueLinks = canvas.getAllByRole('link', { name: /File as a mike issue/ });
		await expect(issueLinks).toHaveLength(REPEATS.groups.length);
		for (const link of issueLinks) {
			await expect(link).toHaveAttribute('target', '_blank');
			await expect(link.getAttribute('href')).toContain('?title=Operator%20keeps%20saying');
		}

		for (const article of articles) {
			await expect(within(article).getAllByTestId('ra-week-count')).toHaveLength(12);
		}

		await expect(canvas.getAllByText('Still happening')).toHaveLength(2);
		await expect(canvas.getAllByText('Quiet')).toHaveLength(2);
		await expect(canvas.getByText('Said 12 times')).toBeVisible();
		await expect(canvas.getAllByText('Weeks starting 25 Jul to 10 Oct')).toHaveLength(4);
	}}
/>
