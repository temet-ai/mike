<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import PromptClassDetail from './PromptClassDetail.svelte';
	import type {
		CompositionRevision,
		PromptClassDetailDto,
		PromptRegistrationDto
	} from '$lib/types';
	import { stubFetchJsonRouter, typed } from '$lib/test-support/storyFetchStubs';

	const UNREGISTERED: PromptRegistrationDto = {
		registered: false,
		composition_id: null,
		label: null,
		mode: null,
		agent_definition_path: null
	};

	function blocks(billing: string, preamble: string, skeleton: string) {
		return [
			{ type: 'text', text: billing, cache_control: null },
			{ type: 'text', text: preamble, cache_control: null },
			{ type: 'text', text: skeleton, cache_control: null }
		];
	}

	function stubDetail(dto: PromptClassDetailDto) {
		return async () => {
			stubFetchJsonRouter((_method, url) => {
				if (url.includes('/api/prompts/classes/')) return typed<PromptClassDetailDto>(dto);
				return typed<CompositionRevision[]>([]);
			});
			return {};
		};
	}

	const { Story } = defineMeta({
		title: 'Routes/Prompts/PromptClassDetail',
		component: PromptClassDetail,
		tags: ['autodocs']
	});
</script>

<Story
	name="MainWithTraffic"
	args={{ lane: 'main', persona: null }}
	loaders={[
		stubDetail({
			lane: 'main',
			subagent_persona: null,
			observation_count: 529,
			first_seen: '2026-07-01T09:00:00Z',
			last_seen: '2026-07-27T16:48:00Z',
			model_breakdown: [],
			sample_blocks: blocks(
				'x-anthropic-billing-header: v1',
				'You are Claude Code, an AI coding assistant.',
				"You are Claude Code, operating in the user's terminal."
			),
			registration: UNREGISTERED,
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
			observations_total_count: 1248,
			observations_offset: 0,
			observations_limit: 20
		})
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(await canvas.findByText('Claude Code (main agent)')).toBeInTheDocument();
		await expect(await canvas.findByText('unregistered')).toBeInTheDocument();
		await expect(
			await canvas.findByDisplayValue("operating in the user's terminal", { exact: false })
		).toBeInTheDocument();
		await expect(await canvas.findByText('request #5102')).toBeInTheDocument();
		await expect(await canvas.findByText(/529 in the last 7 days/)).toBeInTheDocument();
		await expect(await canvas.findByText(/1,248 captured all time/)).toBeInTheDocument();
	}}
>
	{#snippet template({ lane, persona })}
		<div class="h-[900px] w-[900px] overflow-hidden bg-[var(--surface-bg)]">
			<PromptClassDetail {lane} {persona} />
		</div>
	{/snippet}
</Story>

<Story
	name="SubagentWithTraffic"
	args={{ lane: 'subagent', persona: 'Postgres + Rust Database Specialist' }}
	loaders={[
		stubDetail({
			lane: 'subagent',
			subagent_persona: 'Postgres + Rust Database Specialist',
			observation_count: 12,
			first_seen: '2026-07-10T08:00:00Z',
			last_seen: '2026-07-27T14:00:00Z',
			model_breakdown: [],
			sample_blocks: blocks(
				'x-anthropic-billing-header: v1',
				'You are Claude Code, an AI coding assistant.',
				'You are a Postgres + Rust Database Specialist. Design schemas and sqlx queries.'
			),
			registration: UNREGISTERED,
			recent_observations: [
				{
					request_id: '8801',
					session_id: 'sess-pg-8801',
					created_at: '2026-07-27T14:00:00Z',
					content_sha256: 'b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3',
					altered: false,
					model: null
				}
			],
			observations_total_count: 12,
			observations_offset: 0,
			observations_limit: 20
		})
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(
			await canvas.findByText('Postgres + Rust Database Specialist')
		).toBeInTheDocument();
		const registerCta = await canvas.findByText('Register this');
		await expect(registerCta.closest('a')).toHaveAttribute(
			'href',
			'/prompts/new?persona=Postgres%20%2B%20Rust%20Database%20Specialist'
		);
	}}
>
	{#snippet template({ lane, persona })}
		<div class="h-[900px] w-[900px] overflow-hidden bg-[var(--surface-bg)]">
			<PromptClassDetail {lane} {persona} />
		</div>
	{/snippet}
</Story>

<Story
	name="PersonaUnresolved"
	args={{ lane: 'subagent', persona: null }}
	loaders={[
		stubDetail({
			lane: 'subagent',
			subagent_persona: null,
			observation_count: 3,
			first_seen: '2026-07-25T11:00:00Z',
			last_seen: '2026-07-26T09:00:00Z',
			model_breakdown: [],
			sample_blocks: blocks(
				'x-anthropic-billing-header: v1',
				'You are Claude Code, an AI coding assistant.',
				'You are a helper spawned by the workflow orchestrator.'
			),
			registration: UNREGISTERED,
			recent_observations: [],
			observations_total_count: 3,
			observations_offset: 0,
			observations_limit: 20
		})
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(await canvas.findByText('Subagent (persona unresolved)')).toBeInTheDocument();
		await expect(await canvas.findByText('unregistered')).toBeInTheDocument();
	}}
>
	{#snippet template({ lane, persona })}
		<div class="h-[900px] w-[900px] overflow-hidden bg-[var(--surface-bg)]">
			<PromptClassDetail {lane} {persona} />
		</div>
	{/snippet}
</Story>

<Story
	name="RegisteredNeverObserved"
	args={{ lane: 'subagent', persona: 'steer-author' }}
	loaders={[
		stubDetail({
			lane: 'subagent',
			subagent_persona: 'steer-author',
			observation_count: 0,
			first_seen: null,
			last_seen: null,
			model_breakdown: [],
			sample_blocks: [],
			registration: {
				registered: true,
				composition_id: 'comp-quiet-1',
				label: 'steer-author initial prompt (Phase A)',
				mode: 'observe',
				agent_definition_path: null
			},
			recent_observations: [],
			observations_total_count: 0,
			observations_offset: 0,
			observations_limit: 20
		})
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(await canvas.findByText('steer-author')).toBeInTheDocument();
		await expect(await canvas.findByText('registered')).toBeInTheDocument();
		await expect(await canvas.findByText('0 observations, ever')).toBeInTheDocument();
		await expect(await canvas.findByText('No observations captured yet.')).toBeInTheDocument();
	}}
>
	{#snippet template({ lane, persona })}
		<div class="h-[900px] w-[900px] overflow-hidden bg-[var(--surface-bg)]">
			<PromptClassDetail {lane} {persona} />
		</div>
	{/snippet}
</Story>
