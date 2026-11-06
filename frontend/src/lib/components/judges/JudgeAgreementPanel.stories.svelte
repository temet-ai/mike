<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import JudgeAgreementPanel from './JudgeAgreementPanel.svelte';
	import { RUN_4, WORDING_BANNED, textOutsideCode } from '$lib/test-support/judgesFixtures';

	const { Story } = defineMeta({
		title: 'Labelling/JudgeAgreementPanel',
		component: JudgeAgreementPanel,
		tags: ['autodocs'],
		args: {
			role: 'turn_judge',
			run: RUN_4,
			counts: {
				reaction: { total: 40, reviewed: 0 },
				delivery: { total: 38, reviewed: 0 }
			}
		}
	});
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const c = within(canvasElement);

		await expect(
			c.getByRole('link', {
				name: /^Labelling accuracy, .* configuration 14 \(gemini-3\.8-flash\)$/
			})
		).toBeInTheDocument();
		await expect(c.getByRole('heading', { name: 'Latest labelling accuracy' })).toBeInTheDocument();
		await expect(canvasElement.textContent).toContain('160 of 200');
		await expect(canvasElement.textContent).toContain('162 of 200');
		await expect(c.getByTestId('differ').textContent?.replace(/\s+/g, ' ')).toContain(
			"The judge's answer and the label differ on 40 turns for how you reacted and 38 turns for what was delivered. You have reviewed 0 of 78."
		);

		const review = c.getByRole('link', { name: 'Review labels' }) as HTMLAnchorElement;
		await expect(review.getAttribute('href')).toBe('/sessions/labelling/accuracy/4/review');

		await expect(textOutsideCode(canvasElement)).not.toMatch(WORDING_BANNED);
	}}
/>
