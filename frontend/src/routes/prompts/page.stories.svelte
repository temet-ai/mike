<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import Page from './+page.svelte';
	import type {
		CompositionRevision,
		PromptAnomalyGroup,
		PromptClassDetailDto,
		PromptClassDto
	} from '$lib/types';
	import { stubFetchJsonRouter, typed, type TypedBody } from '$lib/test-support/storyFetchStubs';

	function blocks(billing: string, preamble: string, skeleton: string) {
		return [
			{ index: 0, type: 'text', text: billing, cache_control: null },
			{ index: 1, type: 'text', text: preamble, cache_control: null },
			{ index: 2, type: 'text', text: skeleton, cache_control: null }
		];
	}

	const RUST_DEV_REGISTERED = blocks(
		'x-anthropic-billing-header: v1',
		'You are Claude Code, an AI coding assistant.',
		'You are a Rust Development Expert. Original registered instructions.'
	);
	const RUST_DEV_OBSERVED = blocks(
		'x-anthropic-billing-header: v1',
		'You are Claude Code, an AI coding assistant.',
		'You are a Rust Development Expert. Instructions have since drifted from the registration.'
	);
	const VERIFICATION_BLOCKS = blocks(
		'x-anthropic-billing-header: v1',
		'You are Claude Code, an AI coding assistant.',
		'You are a Verification Specialist. Confirm claims against real command output.'
	);
	const MAIN_BLOCKS = blocks(
		'x-anthropic-billing-header: v1',
		'You are Claude Code, an AI coding assistant.',
		"You are Claude Code, operating in the user's terminal."
	);
	const FRONTEND_DEV_BLOCKS = blocks(
		'x-anthropic-billing-header: v1',
		'You are Claude Code, an AI coding assistant.',
		'You are a Frontend Development Expert. Build production-grade UIs with Svelte 5.'
	);

	const CLASSES: PromptClassDto[] = [
		{
			lane: 'main',
			subagent_persona: null,
			observation_count: 1416,
			first_seen: '2026-07-20T09:14:00Z',
			last_seen: '2026-07-27T16:48:00Z',
			block_count: 3,
			model_breakdown: [],
			registration: {
				registered: false,
				composition_id: null,
				label: null,
				mode: null,
				agent_definition_path: null
			}
		},
		{
			lane: 'subagent',
			subagent_persona: 'Frontend Development Expert',
			observation_count: 8,
			first_seen: '2026-07-23T12:00:00Z',
			last_seen: '2026-07-27T13:00:00Z',
			block_count: 3,
			model_breakdown: [],
			registration: {
				registered: false,
				composition_id: null,
				label: null,
				mode: null,
				agent_definition_path: null
			}
		},
		{
			lane: 'subagent',
			subagent_persona: 'Rust Development Expert',
			observation_count: 19,
			first_seen: '2026-07-20T09:14:00Z',
			last_seen: '2026-07-27T15:00:00Z',
			block_count: 3,
			model_breakdown: [],
			registration: {
				registered: true,
				composition_id: 'comp-drift-1',
				label: 'Rust Development Expert',
				mode: 'observe',
				agent_definition_path: '.claude/agents/rust-developer.md'
			}
		},
		{
			lane: 'subagent',
			subagent_persona: 'Verification Specialist',
			observation_count: 7,
			first_seen: '2026-07-15T10:00:00Z',
			last_seen: '2026-07-27T16:20:00Z',
			block_count: 3,
			model_breakdown: [],
			registration: {
				registered: true,
				composition_id: 'comp-match-1',
				label: 'Verification Specialist',
				mode: 'observe',
				agent_definition_path: '.claude/agents/verifier.md'
			}
		},
		{
			lane: 'subagent',
			subagent_persona: 'steer-author',
			observation_count: 0,
			first_seen: null,
			last_seen: null,
			block_count: 3,
			model_breakdown: [],
			registration: {
				registered: true,
				composition_id: 'comp-quiet-1',
				label: 'steer-author initial prompt (Phase A)',
				mode: 'observe',
				agent_definition_path: null
			}
		}
	];

	const ANOMALIES: PromptAnomalyGroup[] = [
		{
			classification: 'structural-mismatch',
			signature_hash: 'sig-non-3-block',
			prompt_sha256: null,
			block_text: null,
			unknown_blocks: { reason: 'non_3_block', len: 2 },
			count: 6,
			first_seen: '2026-07-27T13:00:00Z',
			last_seen: '2026-07-27T13:48:00Z',
			observations: [{ request_id: 4790, session_id: 'sess-a3f8e2c1' }],
			sub_pattern: {
				name: 'non-three-block-layout',
				description: 'The system field was a JSON array but did not contain exactly 3 blocks.',
				recommended_action: 'investigate-caller'
			},
			filter_modified: false,
			firing_rule_ids: [],
			persona: null
		}
	];

	function detailFor(
		lane: 'main' | 'subagent',
		persona: string | null
	): PromptClassDetailDto | null {
		const row = CLASSES.find((c) => c.lane === lane && (c.subagent_persona ?? null) === persona);
		if (!row) return null;
		const sampleBlocks =
			persona === 'Rust Development Expert'
				? RUST_DEV_OBSERVED
				: persona === 'Verification Specialist'
					? VERIFICATION_BLOCKS
					: persona === 'Frontend Development Expert'
						? FRONTEND_DEV_BLOCKS
						: persona === null && lane === 'main'
							? MAIN_BLOCKS
							: [];
		return {
			lane: row.lane,
			subagent_persona: row.subagent_persona,
			observation_count: row.observation_count,
			first_seen: row.first_seen,
			last_seen: row.last_seen,
			model_breakdown: row.model_breakdown,
			sample_blocks: sampleBlocks,
			registration: row.registration,
			recent_observations:
				row.observation_count > 0
					? [
							{
								request_id: '4821',
								session_id: 'sess-a3f8e2c1',
								created_at: row.last_seen ?? '2026-07-27T16:00:00Z',
								content_sha256: 'b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3',
								altered: false,
								model: null
							}
						]
					: [],
			observations_total_count: row.observation_count,
			observations_offset: 0,
			observations_limit: 20
		};
	}

	function revisionsFor(id: string): CompositionRevision[] {
		if (id === 'comp-drift-1') {
			return [
				{
					id: 'rev-drift-1',
					composition_id: 'comp-drift-1',
					version: 1,
					label: 'Rust Development Expert',
					blocks: RUST_DEV_REGISTERED,
					mode: 'observe',
					notes: null,
					created_at: '2026-06-01T10:00:00Z'
				}
			];
		}
		if (id === 'comp-match-1') {
			return [
				{
					id: 'rev-match-1',
					composition_id: 'comp-match-1',
					version: 1,
					label: 'Verification Specialist',
					blocks: VERIFICATION_BLOCKS,
					mode: 'observe',
					notes: null,
					created_at: '2026-06-01T10:00:00Z'
				}
			];
		}
		return [];
	}

	function routeFetch(url: string): TypedBody {
		const detailMatch = /\/api\/prompts\/classes\/(main|subagent\/([^/?]+))/.exec(url);
		if (detailMatch) {
			const persona = detailMatch[2] !== undefined ? decodeURIComponent(detailMatch[2]) : null;
			const lane: 'main' | 'subagent' = detailMatch[1]?.startsWith('main') ? 'main' : 'subagent';
			return typed<PromptClassDetailDto | null>(
				detailFor(lane, persona === 'unresolved' ? null : persona)
			);
		}
		if (url.includes('/api/prompts/classes')) return typed<PromptClassDto[]>(CLASSES);
		if (url.includes('/api/prompts/anomalies')) return typed<PromptAnomalyGroup[]>(ANOMALIES);
		const revisionsMatch = /\/api\/prompts\/compositions\/([^/]+)\/revisions/.exec(url);
		if (revisionsMatch)
			return typed<CompositionRevision[]>(revisionsFor(revisionsMatch[1] as string));
		return typed<CompositionRevision[]>([]);
	}

	const { Story } = defineMeta({
		title: 'Routes/Prompts/Index',
		component: Page,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			stubFetchJsonRouter((_method, url) => routeFetch(url));
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/prompts'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(await canvas.findByText('Registered')).toBeInTheDocument();
		await expect(await canvas.findByText('2', { exact: true })).toBeInTheDocument();

		await expect(await canvas.findByText('Rust Development Expert')).toBeInTheDocument();
		await expect(await canvas.findByText('Claude Code (main agent)')).toBeInTheDocument();
		await expect(
			await canvas.findByText(
				'The system field was a JSON array but did not contain exactly 3 blocks.'
			)
		).toBeInTheDocument();

		const drifted = await canvas.findByRole('button', { name: /Drifted · 1/ });
		await userEvent.click(drifted);
		await expect(await canvas.findByText('Rust Development Expert')).toBeInTheDocument();
		await expect(canvas.queryByText('Claude Code (main agent)')).not.toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="h-[900px] w-[1100px] overflow-hidden bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>
