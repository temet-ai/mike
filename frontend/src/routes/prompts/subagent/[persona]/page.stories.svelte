<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import Page from './+page.svelte';
	import type { CompositionRevision, PromptClassDetailDto } from '$lib/types';
	import { stubFetchJsonRouter, typed } from '$lib/test-support/storyFetchStubs';

	const DETAIL: PromptClassDetailDto = {
		lane: 'subagent',
		subagent_persona: null,
		observation_count: 3,
		first_seen: '2026-07-25T11:00:00Z',
		last_seen: '2026-07-26T09:00:00Z',
		model_breakdown: [],
		sample_blocks: [
			{ type: 'text', text: 'x-anthropic-billing-header: v1', cache_control: null },
			{ type: 'text', text: 'You are Claude Code, an AI coding assistant.', cache_control: null },
			{
				type: 'text',
				text: 'You are a helper spawned by the workflow orchestrator.',
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
		recent_observations: [],
		observations_total_count: 3,
		observations_offset: 0,
		observations_limit: 20
	};

	const { Story } = defineMeta({
		title: 'Routes/Prompts/SubagentPersona',
		component: Page,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			stubFetchJsonRouter((_method, url) => {
				if (url.includes('/api/prompts/classes/subagent/unresolved'))
					return typed<PromptClassDetailDto>(DETAIL);
				return typed<CompositionRevision[]>([]);
			});
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/prompts/subagent/unresolved'),
					params: { persona: 'unresolved' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(await canvas.findByText('Subagent (persona unresolved)')).toBeInTheDocument();
		await expect(await canvas.findByText('unregistered')).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="h-[900px] w-[900px] overflow-hidden bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>
