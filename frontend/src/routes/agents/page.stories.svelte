<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import Page from './+page.svelte';
	import type { AgentListItem } from '$lib/api/agents';
	import {
		stubFetchJson,
		stubFetchError,
		stubFetchPending
	} from '$lib/test-support/storyFetchStubs';

	function buildAgent(overrides: {
		id: number;
		name: string;
		source?: AgentListItem['source'];
		dispatch_count?: number;
		instance_count?: number;
		prompt_version_count?: number;
		composition_count?: number;
		last_dispatched_at?: string | null;
		definition_path?: string | null;
		description?: string | null;
		aliases?: AgentListItem['aliases'];
		created_at?: string;
		updated_at?: string;
	}): AgentListItem {
		return {
			id: overrides.id,
			name: overrides.name,
			source: overrides.source ?? 'observed',
			dispatch_count: overrides.dispatch_count ?? 1,
			instance_count: overrides.instance_count ?? 1,
			prompt_version_count: overrides.prompt_version_count ?? 1,
			composition_count: overrides.composition_count ?? 0,
			last_dispatched_at: overrides.last_dispatched_at ?? '2026-05-04T08:30:00Z',
			definition_path: overrides.definition_path ?? null,
			description: overrides.description ?? null,
			aliases: overrides.aliases ?? [],
			created_at: overrides.created_at ?? '2026-05-01T10:00:00Z',
			updated_at: overrides.updated_at ?? '2026-05-04T08:30:00Z'
		} satisfies AgentListItem;
	}

	const AGENTS_LIST: AgentListItem[] = [
		buildAgent({
			id: 1,
			name: 'frontend-developer',
			source: 'file_backed',
			dispatch_count: 42,
			instance_count: 38,
			prompt_version_count: 5,
			composition_count: 3,
			definition_path: '.claude/agents/frontend-developer.md',
			description:
				'Handles Svelte 5 runes, Tailwind v4, bits-ui, and shadcn-svelte component authoring. Runs vitest and storybook checks before reporting done.',
			aliases: [{ alias: 'frontend', kind: 'name_variant' }],
			last_dispatched_at: '2026-05-04T12:00:00Z',
			created_at: '2026-04-10T09:00:00Z',
			updated_at: '2026-05-03T14:20:00Z'
		}),
		buildAgent({
			id: 2,
			name: 'rust-developer',
			source: 'file_backed',
			dispatch_count: 27,
			instance_count: 25,
			prompt_version_count: 3,
			definition_path: '.claude/agents/rust-developer.md',
			description: 'Axum handlers, sqlx queries, and Rust async patterns.',
			last_dispatched_at: '2026-05-01T11:00:00Z',
			created_at: '2026-04-10T09:05:00Z',
			updated_at: '2026-05-01T11:00:00Z'
		}),
		buildAgent({
			id: 3,
			name: 'ui-designer',
			source: 'file_backed',
			dispatch_count: 6,
			instance_count: 6,
			prompt_version_count: 2,
			definition_path: '.claude/agents/ui-designer.md',
			description: 'Style guide authorship and Storybook coverage for the inspection UI.',
			last_dispatched_at: '2026-04-28T16:45:00Z',
			created_at: '2026-04-15T10:30:00Z',
			updated_at: '2026-04-28T16:45:00Z'
		}),
		buildAgent({
			id: 4,
			name: 'coordinator',
			source: 'dynamic',
			dispatch_count: 1,
			instance_count: 1,
			prompt_version_count: 1,
			description: 'One-shot coordinator agent generated dynamically for the route-blitz team.',
			last_dispatched_at: '2026-05-04T07:00:00Z',
			created_at: '2026-05-04T07:00:00Z',
			updated_at: '2026-05-04T07:00:00Z'
		}),
		buildAgent({
			id: 5,
			name: 'research',
			source: 'observed',
			dispatch_count: 2,
			instance_count: 2,
			prompt_version_count: 2,
			description: null,
			aliases: [{ alias: 'researcher', kind: 'persona' }],
			last_dispatched_at: '2026-05-02T09:55:00Z',
			created_at: '2026-04-22T13:10:00Z',
			updated_at: '2026-05-02T09:55:00Z'
		}),
		buildAgent({
			id: 6,
			name: 'claude',
			source: 'builtin',
			dispatch_count: 118,
			instance_count: 0,
			prompt_version_count: 8,
			description: 'Catch-all default agent — no specialist match.',
			last_dispatched_at: null,
			created_at: '2026-01-01T00:00:00Z',
			updated_at: '2026-01-01T00:00:00Z'
		})
	];

	const { Story } = defineMeta({
		title: 'Routes/Agents/Index',
		component: Page,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			stubFetchJson<AgentListItem[]>(AGENTS_LIST);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/agents'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[700px] w-[900px] overflow-hidden bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="Empty"
	loaders={[
		async () => {
			stubFetchJson<AgentListItem[]>([]);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/agents'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[700px] w-[900px] overflow-hidden bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="Loading"
	loaders={[
		async () => {
			stubFetchPending();
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/agents'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[700px] w-[900px] overflow-hidden bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="Error"
	loaders={[
		async () => {
			stubFetchError('GET /api/agents failed: 503 Service Unavailable');
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/agents'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[700px] w-[900px] overflow-hidden bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="BareAgent"
	loaders={[
		async () => {
			stubFetchJson<AgentListItem[]>([
				buildAgent({
					id: 7,
					name: 'unregistered-helper',
					source: 'observed',
					dispatch_count: 1,
					instance_count: 1,
					prompt_version_count: 1,
					composition_count: 0,
					definition_path: null,
					description: null,
					aliases: [],
					last_dispatched_at: '2026-05-17T11:45:00Z',
					created_at: '2026-05-17T07:30:00Z',
					updated_at: '2026-05-17T11:45:00Z'
				})
			]);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/agents'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[700px] w-[900px] overflow-hidden bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>
