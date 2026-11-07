<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import SkillEvidence from './SkillEvidence.svelte';
	import { SKILL_DRAFT } from '$lib/test-support/skillsFixtures';
	import type { SkillDraft } from '$lib/api/skills';

	const PATCHED: SkillDraft = {
		...SKILL_DRAFT,
		eval: {
			...SKILL_DRAFT.eval,
			trigger: { score: 0.7, correct: 14, total: 20, results: [] },
			calls: 5,
			review: {
				decision: 'patch',
				reason: 'The draft repeated the retry rule twice; the rewrite keeps one step.',
				overlap: null,
				belongs_in: null,
				original_description: 'Checkout changes.',
				original_skill_md:
					'---\nname: checkout-flow\n---\n\nRun the retry test. Always run the retry test.\n'
			},
			recheck: {
				score: 0.9,
				correct: 18,
				total: 20,
				results: [
					{
						query: 'Change the order confirmation email subject.',
						should_trigger: true,
						triggered: true,
						correct: true,
						loaded: ['checkout-flow']
					}
				]
			}
		}
	};

	const { Story } = defineMeta({
		title: 'Skills/SkillEvidence',
		component: SkillEvidence,
		tags: ['autodocs']
	});
</script>

<Story
	name="Approved and rewritten drafts"
	play={async ({ canvasElement }) => {
		const approved = within(within(canvasElement).getByTestId('approved'));
		await expect(approved.getByText('Passed review.')).toBeInTheDocument();
		await expect(approved.getByText('19 of 20 (95%), needs 80%')).toBeInTheDocument();
		await expect(approved.getByText('Approved')).toBeInTheDocument();
		await expect(
			approved.getByText(
				/The reviewer approved this draft as written\. The steps match the lessons/
			)
		).toBeInTheDocument();
		await expect(
			approved.getByRole('region', { name: 'Would Claude load it?' })
		).toBeInTheDocument();
		await expect(approved.getByText('Update the README badges.')).toBeInTheDocument();
		await expect(approved.getByText('yes, wrong')).toBeInTheDocument();

		const patched = within(within(canvasElement).getByTestId('patched'));
		await expect(patched.getByText('Approved after a rewrite')).toBeInTheDocument();
		await expect(patched.getByText('18 of 20 (90%)')).toBeInTheDocument();
		await expect(
			patched.getByRole('region', { name: "The text before the reviewer's rewrite" })
		).toBeInTheDocument();
		await expect(patched.getByText(/Always run the retry test/)).toBeInTheDocument();
		await expect(
			patched.getByRole('region', { name: 'Would Claude load the rewritten skill?' })
		).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="flex w-[52rem] flex-col gap-6 bg-[var(--surface-bg)] p-4">
			<div data-testid="approved"><SkillEvidence draft={SKILL_DRAFT} /></div>
			<div data-testid="patched"><SkillEvidence draft={PATCHED} /></div>
		</div>
	{/snippet}
</Story>
