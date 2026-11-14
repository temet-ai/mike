<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import SkillsPage from './+page.svelte';
	import type { SkillsPageData } from './+page';
	import { PROJECT_SKILLS, PROJECT_UUID, SKILL_PROJECTS } from '$lib/test-support/skillsFixtures';
	import { stubFetchJsonRouter, typed, type TypedBody } from '$lib/test-support/storyFetchStubs';

	const DATA: SkillsPageData = {
		projects: SKILL_PROJECTS.projects,
		projectsError: null,
		selected: PROJECT_UUID,
		skills: PROJECT_SKILLS,
		skillsError: null,
		order: 'default'
	};

	const SUGGESTED: SkillsPageData = {
		...DATA,
		order: 'suggested',
		skills: { ...PROJECT_SKILLS, suggested_accept: { '41': 0.71 } }
	};

	let policyBody: unknown = null;

	function router(method: string, url: string, body: unknown): TypedBody {
		if (method === 'POST' && url.endsWith('/api/skills/policy')) {
			policyBody = body;
			return typed<{ ok: boolean }>({ ok: true });
		}
		if (url.includes('/api/skills/projects')) return typed<typeof SKILL_PROJECTS>(SKILL_PROJECTS);
		return typed<typeof PROJECT_SKILLS>(PROJECT_SKILLS);
	}

	const { Story } = defineMeta({
		title: 'Routes/Skills',
		component: SkillsPage,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' },
		argTypes: { data: { control: false } }
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			policyBody = null;
			stubFetchJsonRouter(router);
			return {};
		}
	]}
	args={{ data: DATA }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByRole('heading', { level: 1, name: 'Skills' })).toBeInTheDocument();
		await expect(canvas.getByText(/nothing is written until you approve it/)).toBeInTheDocument();
		const tab = canvas.getByRole('tab', { name: /storefront/ });
		await expect(tab).toHaveAttribute('aria-selected', 'true');
		await expect(tab).toHaveTextContent('0 of 12 shipped, asks first, 1 waiting on you');
		await expect(canvas.getByRole('heading', { level: 2, name: 'storefront' })).toBeInTheDocument();
		await expect(canvas.getByRole('radio', { name: /Ask me first/ })).toBeChecked();
		await expect(canvas.getByRole('link', { name: 'checkout-flow' })).toBeInTheDocument();
		await expect(canvas.getByText('Did not pass review')).toBeInTheDocument();
		await userEvent.click(canvas.getByRole('radio', { name: /Ship when it passes review/ }));
		await waitFor(() => expect(policyBody).toEqual({ project: PROJECT_UUID, mode: 'auto' }));
	}}
/>

<Story
	name="Suggested order"
	loaders={[
		async () => {
			policyBody = null;
			stubFetchJsonRouter(router);
			return {};
		}
	]}
	args={{ data: SUGGESTED }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByLabelText('Order')).toHaveValue('suggested');
		const chances = [...canvasElement.querySelectorAll<HTMLElement>('[data-slot="accept-chance"]')];
		await expect(chances).toHaveLength(PROJECT_SKILLS.skills.length);
		const waiting = canvas.getByRole('link', { name: 'checkout-flow' }).closest('article');
		await expect(
			waiting
				?.querySelector('[data-slot="accept-chance"]')
				?.textContent?.replace(/\s+/g, ' ')
				.trim()
		).toBe('accept 71%');
		await expect(
			new Set(chances.map((c) => Math.round(c.getBoundingClientRect().width))).size
		).toBe(1);
		await expect(policyBody).toBeNull();
	}}
/>
