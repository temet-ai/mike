<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import ProposalsPage from './+page.svelte';
	import type { ProposalsPageData } from './+page';
	import {
		PROJECT_PROPOSALS,
		PROPOSAL_PROJECT_UUID,
		PROPOSAL_PROJECTS
	} from '$lib/test-support/proposalsFixtures';
	import { stubFetchJsonRouter, typed, type TypedBody } from '$lib/test-support/storyFetchStubs';

	const DATA: ProposalsPageData = {
		projects: PROPOSAL_PROJECTS.projects,
		projectsError: null,
		selected: PROPOSAL_PROJECT_UUID,
		proposals: PROJECT_PROPOSALS,
		proposalsError: null
	};

	let decision: { url: string; body: unknown } | null = null;

	function router(method: string, url: string, body: unknown): TypedBody {
		if (method === 'POST' && url.includes('/decision')) {
			decision = { url, body };
			return typed<{ ok: boolean }>({ ok: true });
		}
		if (url.includes('/api/proposals/projects')) {
			return typed<typeof PROPOSAL_PROJECTS>(PROPOSAL_PROJECTS);
		}
		return typed<typeof PROJECT_PROPOSALS>(PROJECT_PROPOSALS);
	}

	const { Story } = defineMeta({
		title: 'Routes/Proposals',
		component: ProposalsPage,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' },
		argTypes: { data: { control: false } }
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			decision = null;
			stubFetchJsonRouter(router);
			return {};
		}
	]}
	args={{ data: DATA }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByRole('heading', { level: 1, name: 'Proposals' })).toBeInTheDocument();
		const tab = canvas.getByRole('tab', { name: /storefront/ });
		await expect(tab).toHaveTextContent('3 proposals, 2 waiting on you');
		await expect(canvas.getByRole('heading', { level: 2, name: 'storefront' })).toBeInTheDocument();
		await expect(canvas.getByText('.claude/rules/dependency-pins.md')).toBeInTheDocument();
		const first = within(canvas.getByRole('group', { name: 'Your decision on lesson 1303' }));
		await userEvent.click(first.getByRole('button', { name: 'Decline' }));
		await waitFor(() =>
			expect(decision).toEqual({
				url: '/api/proposals/1303/decision',
				body: { decision: 'decline' }
			})
		);
	}}
/>
