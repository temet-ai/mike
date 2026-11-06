<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import SkillDraftPage from './+page.svelte';
	import type { SkillDraftPageData } from './+page';
	import {
		FOLDER_SHIPPED,
		FOLDER_WAITING,
		PROJECT_SKILLS,
		SKILL_DRAFT_RESPONSE
	} from '$lib/test-support/skillsFixtures';
	import { stubFetchJsonRouter, typed, type TypedBody } from '$lib/test-support/storyFetchStubs';

	// The reviewer advises that invoice-rounding, a folder mike keeps, already covers these lessons.
	const ADVICE = {
		decision: 'wrong_place',
		reason: 'The retry steps repeat what invoice-rounding already tells Claude to do.',
		overlap: 'invoice-rounding',
		belongs_in: 'existing_skill',
		original_description: null,
		original_skill_md: null
	} as const;

	const DETAIL: typeof SKILL_DRAFT_RESPONSE = {
		...SKILL_DRAFT_RESPONSE,
		draft: {
			...SKILL_DRAFT_RESPONSE.draft,
			eval: { ...SKILL_DRAFT_RESPONSE.draft.eval, review: ADVICE }
		},
		folder: { ...FOLDER_WAITING, reviewer: ADVICE }
	};

	const DATA: SkillDraftPageData = {
		id: 41,
		detail: DETAIL,
		skills: { ...PROJECT_SKILLS, skills: [...PROJECT_SKILLS.skills, FOLDER_SHIPPED] },
		error: null
	};

	const sent: { method: string; url: string; body: unknown }[] = [];

	function router(method: string, url: string, body: unknown): TypedBody {
		if (method !== 'GET') {
			sent.push({ method, url, body });
			return typed<{ ok: boolean }>({ ok: true });
		}
		return typed<typeof SKILL_DRAFT_RESPONSE>(SKILL_DRAFT_RESPONSE);
	}

	const { Story } = defineMeta({
		title: 'Routes/SkillDraft',
		component: SkillDraftPage,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' },
		argTypes: { data: { control: false } }
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			sent.length = 0;
			stubFetchJsonRouter(router);
			return {};
		}
	]}
	args={{ data: DATA }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(
			canvas.getByRole('heading', { level: 1, name: 'checkout-flow' })
		).toBeInTheDocument();
		await expect(canvas.getByRole('link', { name: /Skills/ })).toHaveAttribute(
			'href',
			'/skills?project=6f1c2b8e-0d1a-4c55-9a0e-3b7f2e1d4c90'
		);
		await expect(canvas.getAllByText('Waiting on you').length).toBeGreaterThan(0);
		await expect(canvas.getByText(/only after you approve it/)).toBeInTheDocument();
		await expect(canvas.getByText(/a rounding change once double-charged retries\.\s+Read/)).toBeInTheDocument();
		await expect(canvas.getByRole('link', { name: 'Lesson 812' })).toBeInTheDocument();
		await expect(
			canvas.getByText(
				'The reviewer advised against a new skill, because the skill invoice-rounding already covers these lessons.'
			)
		).toBeInTheDocument();
		await expect(canvas.getByRole('link', { name: /Open invoice-rounding/ })).toHaveAttribute(
			'href',
			'/skills/17'
		);

		await userEvent.click(canvas.getByRole('button', { name: 'Approve' }));
		await waitFor(() =>
			expect(sent).toContainEqual({
				method: 'POST',
				url: '/api/skills/drafts/41/review',
				body: { decision: 'approve' }
			})
		);

		await userEvent.click(canvas.getByRole('button', { name: 'Edit SKILL.md' }));
		const field = canvas.getByRole('textbox', { name: /SKILL.md/ });
		await userEvent.type(field, 'Also run the tax test.');
		await userEvent.click(canvas.getByRole('button', { name: 'Save SKILL.md' }));
		await waitFor(() =>
			expect(sent.some((s) => s.method === 'PATCH' && s.url === '/api/skills/drafts/41/content')).toBe(
				true
			)
		);
	}}
/>
