<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import TestRunCard from './TestRunCard.svelte';
	import {
		BUNDLES,
		GATES,
		RUN_4,
		RUN_5,
		WORDING_BANNED,
		textOutsideCode
	} from '$lib/test-support/judgesFixtures';

	const BUNDLE_14 = BUNDLES.find((b) => b.id === 14) ?? null;

	const { Story } = defineMeta({
		title: 'Labelling/TestRunCard',
		component: TestRunCard,
		tags: ['autodocs'],
		args: {
			run: RUN_4,
			bundle: BUNDLE_14,
			liveBundleId: 14,
			gate: GATES[0] ?? null,
			counts: {
				reaction: { total: 40, reviewed: 0 },
				delivery: { total: 38, reviewed: 0 }
			},
			others: [RUN_4, RUN_5]
		}
	});
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const c = within(canvasElement);

		const runLink = c.getByRole('link', {
			name: /^Labelling accuracy, 28 Sep( 2026)? \d\d:\d\d · configuration 14 \(gemini-3\.8-flash\)$/
		});
		await expect(runLink).toHaveAttribute(
			'href',
			expect.stringMatching(/\/sessions\/labelling\/accuracy\/4$/)
		);

		await expect(c.getByRole('link', { name: 'Configuration 14' })).toBeInTheDocument();
		await expect(canvasElement.textContent).toContain('(in use)');
		await expect(canvasElement.textContent).toContain(
			'200 turns set aside for measuring accuracy, labelled up to 28 Sep 2026'
		);
		await expect(canvasElement.textContent).toContain('agreed on 160 of 200');
		await expect(canvasElement.textContent).toContain('80%');
		await expect(canvasElement.textContent).toContain('40 differ from the label · 0 reviewed');

		const review = c.getAllByRole('link', { name: 'Review labels' })[0] as HTMLAnchorElement;
		await expect(review.getAttribute('href')).toContain(
			'/sessions/labelling/accuracy/4/review?mode=reaction'
		);

		await userEvent.click(
			c.getByRole('button', { name: /Passed the accuracy bar set for being put in use/ })
		);
		await expect(canvasElement.textContent).toContain('86 times in 100 (needs 85)');
		await expect(
			c.getByRole('link', {
				name: /^Labelling accuracy, .* configuration 19 \(gemini-3\.8-flash\)$/
			})
		).toBeInTheDocument();
		await expect(canvasElement.textContent).not.toMatch(/\brun [0-9]/i);

		await expect(textOutsideCode(canvasElement)).not.toMatch(WORDING_BANNED);
	}}
/>
