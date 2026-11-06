<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import LabelDefinitions from './LabelDefinitions.svelte';
	import { VOCABULARY } from '$lib/test-support/evalReviewFixtures';

	const { Story } = defineMeta({
		title: 'Eval/LabelDefinitions',
		component: LabelDefinitions,
		tags: ['autodocs'],
		args: { vocabulary: VOCABULARY }
	});
</script>

<Story
	name="Closed, then opened"
	play={async ({ canvasElement }) => {
		const c = within(canvasElement);
		const toggle = c.getByRole('button', {
			name: 'What each label means, and the rules the AI labellers followed'
		});
		await expect(toggle).toHaveAttribute('aria-expanded', 'false');
		await expect(c.queryByRole('region', { name: 'How you reacted' })).toBeNull();
		await userEvent.click(toggle);
		await expect(toggle).toHaveAttribute('aria-expanded', 'true');
		const reaction = c.getByRole('region', { name: 'How you reacted' });
		await expect(within(reaction).getByText('Corrected')).toBeInTheDocument();
		await expect(reaction.textContent).toContain(
			'A reply that accepts part of the turn and corrects another part is Corrected.'
		);
		await expect(c.getByRole('region', { name: 'What was delivered' }).textContent).toContain(
			'Partial:'
		);
	}}
/>

<Story name="Definitions could not be loaded" args={{ vocabulary: [], open: true }} />
