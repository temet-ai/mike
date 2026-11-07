<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import SkillFolderRow from './SkillFolderRow.svelte';
	import {
		FOLDER_COVERED,
		FOLDER_FAILED,
		FOLDER_GENERATED,
		FOLDER_RETIRED,
		FOLDER_SHIPPED,
		FOLDER_WAITING
	} from '$lib/test-support/skillsFixtures';

	const ROWS = [
		FOLDER_WAITING,
		FOLDER_SHIPPED,
		FOLDER_RETIRED,
		FOLDER_FAILED,
		FOLDER_COVERED,
		FOLDER_GENERATED
	];

	const { Story } = defineMeta({
		title: 'Skills/SkillFolderRow',
		component: SkillFolderRow,
		tags: ['autodocs']
	});
</script>

<Story
	name="A project's skills"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByRole('link', { name: 'checkout-flow' })).toHaveAttribute(
			'href',
			'/skills/41'
		);
		await expect(canvas.getByText('Waiting on you')).toBeInTheDocument();
		await expect(
			canvas.getByText('/Users/sam/code/billing-service/.claude/skills/invoice-rounding/SKILL.md')
		).toBeInTheDocument();
		await expect(canvas.getByText('9 loads, 1 of 8 judged turns corrected')).toBeInTheDocument();
		await expect(
			canvas.getByText(/The reviewer approved this draft as written\. The steps match/)
		).toBeInTheDocument();
		await expect(canvas.getByText('Retired, did not help')).toBeInTheDocument();
		await expect(
			canvas.getByText(
				/mike removed this skill on 13 Oct because it was not helping: the turns that loaded it were corrected more often \(42%\) than the project's turns over the same time \(18%\)/
			)
		).toBeInTheDocument();
		await expect(canvas.getByText(/removed 13 Oct, it was not helping/)).toBeInTheDocument();
		await expect(canvas.getAllByText('Belongs somewhere else')).toHaveLength(2);
		await expect(
			canvas.getByText(
				'The reviewer advised against a new skill, because the skill invoice-rounding already covers these lessons.'
			)
		).toBeInTheDocument();
		await expect(
			canvas.getByText('mike does not yet move these lessons into invoice-rounding by itself.')
		).toBeInTheDocument();
		await expect(canvas.getAllByText("The reviewer's advice")).toHaveLength(2);
		await expect(
			canvas.getAllByRole('link', { name: /Decide on the skill page/ })[1]
		).toHaveAttribute('href', '/skills/44');
		await expect(canvas.getAllByText('holds this draft').length).toBeGreaterThan(0);
		await expect(canvas.getByText('holds an earlier revision')).toBeInTheDocument();
		await expect(canvas.getByText("holds mike's generated list of lessons")).toBeInTheDocument();
		await expect(canvas.getByText('held this draft')).toBeInTheDocument();
		await expect(
			canvas.getByText(/belongs elsewhere \(rule: \.claude\/rules\/changelog\.md\)/)
		).toBeInTheDocument();
		await expect(canvas.getByRole('heading', { name: 'deploy-and-restart' })).toBeInTheDocument();
		await expect(canvas.queryByRole('link', { name: 'deploy-and-restart' })).toBeNull();
	}}
>
	{#snippet template()}
		<div class="flex w-[52rem] flex-col gap-3 bg-[var(--surface-bg)] p-4">
			{#each ROWS as skill (skill.folder)}
				<SkillFolderRow {skill} />
			{/each}
		</div>
	{/snippet}
</Story>
