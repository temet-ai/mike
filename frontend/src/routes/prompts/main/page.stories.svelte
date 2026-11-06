<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import Page from './+page.svelte';
	import type { CompositionRevision, PromptClassDetailDto } from '$lib/types';
	import { stubFetchJsonRouter, typed } from '$lib/test-support/storyFetchStubs';

	const DETAIL: PromptClassDetailDto = {
		lane: 'main',
		subagent_persona: null,
		observation_count: 1416,
		first_seen: '2026-07-20T09:14:00Z',
		last_seen: '2026-07-27T16:48:00Z',
		model_breakdown: [],
		sample_blocks: [
			{ type: 'text', text: 'x-anthropic-billing-header: v1', cache_control: null },
			{ type: 'text', text: 'You are Claude Code, an AI coding assistant.', cache_control: null },
			{
				type: 'text',
				text: "You are Claude Code, operating in the user's terminal.",
				cache_control: null
			}
		],
		registration: {
			registered: false,
			composition_id: null,
			label: null,
			mode: null,
			agent_definition_path: null
		},
		recent_observations: [
			{
				request_id: '5102',
				session_id: 'sess-b7e1f9c2',
				created_at: '2026-07-27T16:48:00Z',
				content_sha256: 'a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2',
				altered: false,
				model: null
			}
		],
		observations_total_count: 1416,
		observations_offset: 0,
		observations_limit: 20
	};

	const { Story } = defineMeta({
		title: 'Routes/Prompts/Main',
		component: Page,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			stubFetchJsonRouter((_method, url) => {
				if (url.includes('/api/prompts/classes/main')) return typed<PromptClassDetailDto>(DETAIL);
				return typed<CompositionRevision[]>([]);
			});
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/prompts/main'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(await canvas.findByText('Claude Code (main agent)')).toBeInTheDocument();
		await expect(await canvas.findByText('unregistered')).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="h-[900px] w-[900px] overflow-hidden bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>
