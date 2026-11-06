<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import Page from './+page.svelte';
	import type {
		AgentDetail,
		AgentSource,
		AgentAlias,
		AgentDispatchRow,
		AgentPromptVersion,
		AgentInstance
	} from '$lib/api/agents';
	import type { PromptComposition } from '$lib/types';
	import {
		stubFetchJson,
		stubFetch404,
		stubFetchError,
		stubFetchPending
	} from '$lib/test-support/storyFetchStubs';

	const STORY_AGENT_ID = 42;
	const STORY_AGENT_ID_PARAM = String(STORY_AGENT_ID);

	function buildComposition(overrides: {
		id: string;
		label: string;
		prompt_sha256: string;
		mode?: PromptComposition['mode'];
		applied_count_24h?: number;
		created_at?: string;
		updated_at?: string;
	}): PromptComposition {
		return {
			id: overrides.id,
			prompt_sha256: overrides.prompt_sha256,
			label: overrides.label,
			version: 1,
			blocks: [],
			mode: overrides.mode ?? 'observe',
			notes: null,
			agent_name: 'frontend-developer',
			agent_definition_path: '.claude/agents/frontend-developer.md',
			created_at: overrides.created_at ?? '2026-05-01T10:00:00Z',
			updated_at: overrides.updated_at ?? '2026-05-04T08:30:00Z',
			applied_count_24h: overrides.applied_count_24h ?? 0
		} satisfies PromptComposition;
	}

	function buildDispatch(overrides: {
		tool_use_id: number;
		dispatched_at?: string;
		prompt?: string;
		session_uuid?: string;
		request_id?: number;
	}): AgentDispatchRow {
		return {
			tool_use_id: overrides.tool_use_id,
			dispatched_at: overrides.dispatched_at ?? '2026-05-04T07:00:00Z',
			prompt:
				overrides.prompt ??
				'You are the frontend-developer agent. Implement the requested Svelte 5 component using runes, Tailwind v4 tokens, and the project style guide. Run scripts/sb-test before reporting done.',
			session_uuid: overrides.session_uuid ?? '3f9b6e2a-1111-4c3d-9a10-abcdefabcdef01',
			request_id: overrides.request_id ?? 100
		} satisfies AgentDispatchRow;
	}

	function buildPromptVersion(overrides: {
		content_sha256: string;
		first_seen_at?: string;
		last_seen_at?: string;
		request_count?: number;
	}): AgentPromptVersion {
		return {
			content_sha256: overrides.content_sha256,
			first_seen_at: overrides.first_seen_at ?? '2026-04-10T09:00:00Z',
			last_seen_at: overrides.last_seen_at ?? '2026-05-04T08:30:00Z',
			request_count: overrides.request_count ?? 1
		} satisfies AgentPromptVersion;
	}

	function buildInstance(overrides: {
		id: number;
		session_uuid?: string;
		subagent_persona?: string | null;
		started_at?: string | null;
		first_request_id?: number | null;
	}): AgentInstance {
		return {
			id: overrides.id,
			session_uuid: overrides.session_uuid ?? '3f9b6e2a-1111-4c3d-9a10-abcdefabcdef01',
			subagent_persona: overrides.subagent_persona ?? 'frontend-developer',
			started_at: overrides.started_at ?? '2026-05-04T07:00:00Z',
			first_request_id: overrides.first_request_id ?? 100
		} satisfies AgentInstance;
	}

	function buildAgentDetail(overrides: {
		name: string;
		source?: AgentSource;
		definition_path?: string | null;
		description?: string | null;
		aliases?: AgentAlias[];
		compositions?: PromptComposition[];
		dispatches?: { total_count: number; rows: AgentDispatchRow[] };
		prompt_versions?: AgentPromptVersion[];
		instances?: AgentInstance[];
	}): AgentDetail {
		const dispatches = overrides.dispatches ?? { total_count: 0, rows: [] };
		const promptVersions = overrides.prompt_versions ?? [];
		const instances = overrides.instances ?? [];
		const compositions = overrides.compositions ?? [];
		return {
			id: STORY_AGENT_ID,
			name: overrides.name,
			source: overrides.source ?? 'file_backed',
			definition_path: overrides.definition_path ?? null,
			description: overrides.description ?? null,
			aliases: overrides.aliases ?? [],
			composition_count: compositions.length,
			dispatch_count: dispatches.total_count,
			instance_count: instances.length,
			prompt_version_count: promptVersions.length,
			last_dispatched_at: dispatches.rows[0]?.dispatched_at ?? null,
			dispatches,
			prompt_versions: promptVersions,
			instances,
			compositions,
			created_at: '2026-04-10T09:00:00Z',
			updated_at: '2026-05-04T08:30:00Z'
		} satisfies AgentDetail;
	}

	const COMPOSITIONS: PromptComposition[] = [
		buildComposition({
			id: 'comp-1',
			label: 'Frontend developer — route-blitz sprint',
			prompt_sha256: 'a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2',
			mode: 'replace',
			applied_count_24h: 14,
			created_at: '2026-05-04T07:00:00Z',
			updated_at: '2026-05-04T12:00:00Z'
		}),
		buildComposition({
			id: 'comp-2',
			label: 'Frontend developer — storybook blitz',
			prompt_sha256: 'b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3',
			mode: 'replace',
			applied_count_24h: 3,
			created_at: '2026-04-28T09:00:00Z',
			updated_at: '2026-04-30T14:00:00Z'
		}),
		buildComposition({
			id: 'comp-3',
			label: 'Frontend developer — initial definition',
			prompt_sha256: 'c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4',
			mode: 'replace',
			applied_count_24h: 0,
			created_at: '2026-04-10T09:00:00Z',
			updated_at: '2026-04-10T09:00:00Z'
		})
	];

	const LONG_BRIEF_COMPOSITIONS: PromptComposition[] = [
		buildComposition({
			id: 'comp-long-1',
			label:
				'Frontend developer — comprehensive multi-pass refactor of the Svelte 5 runes migration including all component tests, storybook stories, and accessibility audit for the complete inspection UI surface',
			prompt_sha256: 'd4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5',
			mode: 'replace',
			applied_count_24h: 2,
			created_at: '2026-05-01T08:00:00Z',
			updated_at: '2026-05-04T10:00:00Z'
		}),
		...COMPOSITIONS
	];

	const DISPATCHES = {
		total_count: 2,
		rows: [
			buildDispatch({
				tool_use_id: 1002,
				dispatched_at: '2026-05-04T12:00:00Z',
				request_id: 205
			}),
			buildDispatch({
				tool_use_id: 1001,
				dispatched_at: '2026-05-03T09:15:00Z',
				request_id: 188
			})
		]
	};

	const PROMPT_VERSIONS: AgentPromptVersion[] = [
		buildPromptVersion({
			content_sha256: 'a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2',
			first_seen_at: '2026-05-01T08:00:00Z',
			last_seen_at: '2026-05-04T12:00:00Z',
			request_count: 14
		}),
		buildPromptVersion({
			content_sha256: 'c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4',
			first_seen_at: '2026-04-10T09:00:00Z',
			last_seen_at: '2026-04-30T14:00:00Z',
			request_count: 6
		})
	];

	const INSTANCES: AgentInstance[] = [
		buildInstance({ id: 1, started_at: '2026-05-04T07:00:00Z', first_request_id: 205 }),
		buildInstance({
			id: 2,
			session_uuid: '4a0c7f3b-2222-4d4e-8b21-bcdefabcdef012',
			started_at: '2026-05-03T09:15:00Z',
			first_request_id: 188
		})
	];

	const { Story } = defineMeta({
		title: 'Routes/Agents/Detail',
		component: Page,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			stubFetchJson<AgentDetail>(
				buildAgentDetail({
					name: 'frontend-developer',
					source: 'file_backed',
					definition_path: '.claude/agents/frontend-developer.md',
					description:
						'Handles Svelte 5 runes, Tailwind v4, bits-ui, and shadcn-svelte component authoring. Runs vitest and storybook checks before reporting done.',
					aliases: [{ alias: 'frontend', kind: 'name_variant' }],
					dispatches: DISPATCHES,
					prompt_versions: PROMPT_VERSIONS,
					instances: INSTANCES,
					compositions: COMPOSITIONS
				})
			);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/agents/${STORY_AGENT_ID_PARAM}`),
					params: { agentId: STORY_AGENT_ID_PARAM }
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
					url: new URL(`http://localhost/agents/${STORY_AGENT_ID_PARAM}`),
					params: { agentId: STORY_AGENT_ID_PARAM }
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
	name="NotFound"
	loaders={[
		async () => {
			stubFetch404();
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/agents/${STORY_AGENT_ID_PARAM}`),
					params: { agentId: STORY_AGENT_ID_PARAM }
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
	name="WithLongBrief"
	loaders={[
		async () => {
			stubFetchJson<AgentDetail>(
				buildAgentDetail({
					name: 'frontend-developer',
					source: 'file_backed',
					definition_path: '.claude/agents/frontend-developer.md',
					description:
						'Handles Svelte 5 runes, Tailwind v4, bits-ui, and shadcn-svelte component authoring. Runs vitest and storybook checks before reporting done.',
					dispatches: DISPATCHES,
					prompt_versions: PROMPT_VERSIONS,
					instances: INSTANCES,
					compositions: LONG_BRIEF_COMPOSITIONS
				})
			);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/agents/${STORY_AGENT_ID_PARAM}`),
					params: { agentId: STORY_AGENT_ID_PARAM }
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
	name="LoadError"
	loaders={[
		async () => {
			stubFetchError('agent store unavailable');
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/agents/${STORY_AGENT_ID_PARAM}`),
					params: { agentId: STORY_AGENT_ID_PARAM }
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
	name="NoCompositions"
	loaders={[
		async () => {
			stubFetchJson<AgentDetail>(
				buildAgentDetail({
					name: 'newly-registered-agent',
					source: 'file_backed',
					definition_path: '.claude/agents/newly-registered-agent.md',
					description:
						'Registered from disk and dispatched once, but has no prompt compositions linked yet.',
					dispatches: DISPATCHES,
					prompt_versions: PROMPT_VERSIONS,
					instances: INSTANCES,
					compositions: []
				})
			);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/agents/${STORY_AGENT_ID_PARAM}`),
					params: { agentId: STORY_AGENT_ID_PARAM }
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
	name="EditingForm"
	loaders={[
		async () => {
			stubFetchJson<AgentDetail>(
				buildAgentDetail({
					name: 'frontend-developer',
					source: 'file_backed',
					definition_path: '.claude/agents/frontend-developer.md',
					description: 'Svelte 5 / Tailwind v4 / bits-ui authoring.',
					dispatches: DISPATCHES,
					prompt_versions: PROMPT_VERSIONS,
					instances: INSTANCES,
					compositions: COMPOSITIONS
				})
			);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/agents/${STORY_AGENT_ID_PARAM}`),
					params: { agentId: STORY_AGENT_ID_PARAM }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const editBtn = await waitFor(() => {
			const el = canvasElement.querySelector('[data-slot="agent-detail-edit-btn"]');
			if (!el) throw new Error('edit button not yet rendered');
			return el as HTMLButtonElement;
		});
		await userEvent.click(editBtn);
		const form = await waitFor(
			() => {
				const el = canvasElement.querySelector('[data-slot="agent-detail-edit-form"]');
				if (!el) throw new Error('edit form not yet rendered');
				return el as HTMLElement;
			},
			{ timeout: 3000 }
		);
		await expect(form).toBeInTheDocument();
		void canvas;
	}}
>
	{#snippet template()}
		<div class="h-[700px] w-[900px] overflow-hidden bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="SaveErrorEmptyName"
	loaders={[
		async () => {
			stubFetchJson<AgentDetail>(
				buildAgentDetail({
					name: 'frontend-developer',
					source: 'file_backed',
					definition_path: '.claude/agents/frontend-developer.md',
					description: 'Svelte 5 / Tailwind v4 / bits-ui authoring.',
					dispatches: DISPATCHES,
					prompt_versions: PROMPT_VERSIONS,
					instances: INSTANCES,
					compositions: COMPOSITIONS
				})
			);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/agents/${STORY_AGENT_ID_PARAM}`),
					params: { agentId: STORY_AGENT_ID_PARAM }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const editBtn = await waitFor(() => {
			const el = canvasElement.querySelector('[data-slot="agent-detail-edit-btn"]');
			if (!el) throw new Error('edit button not yet rendered');
			return el as HTMLButtonElement;
		});
		await userEvent.click(editBtn);
		const form = await waitFor(() => {
			const el = canvasElement.querySelector('[data-slot="agent-detail-edit-form"]');
			if (!el) throw new Error('edit form not yet rendered');
			return el as HTMLFormElement;
		});
		const nameInput = form.querySelector('input') as HTMLInputElement;
		await userEvent.click(nameInput);
		await userEvent.keyboard('{Control>}a{/Control}');
		await userEvent.keyboard('{Delete}');
		const saveBtn = form.querySelector('button[type="submit"]') as HTMLButtonElement;
		await userEvent.click(saveBtn);
		const err = await waitFor(
			() => {
				const hit = within(form).queryByText(/Name must not be empty/i);
				if (!hit) throw new Error('empty-name error not yet rendered');
				return hit as HTMLElement;
			},
			{ timeout: 3000 }
		);
		await expect(err).toBeInTheDocument();
		void canvas;
	}}
>
	{#snippet template()}
		<div class="h-[700px] w-[900px] overflow-hidden bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="SaveErrorInteraction"
	loaders={[
		async () => {
			stubFetchJson<AgentDetail>(
				buildAgentDetail({
					name: 'frontend-developer',
					source: 'file_backed',
					definition_path: '.claude/agents/frontend-developer.md',
					description: 'Svelte 5 / Tailwind v4 / bits-ui authoring.',
					dispatches: DISPATCHES,
					prompt_versions: PROMPT_VERSIONS,
					instances: INSTANCES,
					compositions: COMPOSITIONS
				})
			);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/agents/${STORY_AGENT_ID_PARAM}`),
					params: { agentId: STORY_AGENT_ID_PARAM }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const editBtn = await waitFor(() => {
			const el = canvasElement.querySelector('[data-slot="agent-detail-edit-btn"]');
			if (!el) throw new Error('edit button not yet rendered');
			return el as HTMLButtonElement;
		});
		await userEvent.click(editBtn);
		const form = await waitFor(() => {
			const el = canvasElement.querySelector('[data-slot="agent-detail-edit-form"]');
			if (!el) throw new Error('edit form not yet rendered');
			return el as HTMLFormElement;
		});
		const nameInput = form.querySelector('input') as HTMLInputElement;
		await userEvent.click(nameInput);
		await userEvent.type(nameInput, '-edited');
		globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (init?.method === 'PATCH' && url.includes('/api/agents/')) {
				return new Response(JSON.stringify({ error: 'agent store rejected the patch' }), {
					status: 503,
					statusText: 'Service Unavailable',
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404 });
		};
		const saveBtn = form.querySelector('button[type="submit"]') as HTMLButtonElement;
		await userEvent.click(saveBtn);
		const err = await waitFor(
			() => {
				const ps = Array.from(form.querySelectorAll('p'));
				const hit = ps.find(
					(p) =>
						p.className.includes('--status-danger-fg') && (p.textContent ?? '').trim().length > 0
				);
				if (!hit) throw new Error('save-error not yet rendered');
				return hit as HTMLElement;
			},
			{ timeout: 3000 }
		);
		await expect(err).toBeInTheDocument();
		void canvas;
	}}
>
	{#snippet template()}
		<div class="h-[700px] w-[900px] overflow-hidden bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="ConfirmDelete"
	loaders={[
		async () => {
			stubFetchJson<AgentDetail>(
				buildAgentDetail({
					name: 'frontend-developer',
					source: 'file_backed',
					definition_path: '.claude/agents/frontend-developer.md',
					description: 'Svelte 5 / Tailwind v4 / bits-ui authoring.',
					dispatches: DISPATCHES,
					prompt_versions: PROMPT_VERSIONS,
					instances: INSTANCES,
					compositions: COMPOSITIONS
				})
			);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/agents/${STORY_AGENT_ID_PARAM}`),
					params: { agentId: STORY_AGENT_ID_PARAM }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const deleteBtn = await waitFor(() => {
			const el = canvasElement.querySelector('[data-slot="agent-detail-delete-btn"]');
			if (!el) throw new Error('delete button not yet rendered');
			return el as HTMLButtonElement;
		});
		await userEvent.click(deleteBtn);
		const confirm = await waitFor(
			() => {
				const el = canvasElement.querySelector('[data-slot="agent-detail-delete-confirm"]');
				if (!el) throw new Error('delete-confirm panel not yet rendered');
				return el as HTMLElement;
			},
			{ timeout: 3000 }
		);
		await expect(confirm).toBeInTheDocument();
		void canvas;
	}}
>
	{#snippet template()}
		<div class="h-[700px] w-[900px] overflow-hidden bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="DeleteErrorInteraction"
	loaders={[
		async () => {
			stubFetchJson<AgentDetail>(
				buildAgentDetail({
					name: 'frontend-developer',
					source: 'file_backed',
					definition_path: '.claude/agents/frontend-developer.md',
					description: 'Svelte 5 / Tailwind v4 / bits-ui authoring.',
					dispatches: DISPATCHES,
					prompt_versions: PROMPT_VERSIONS,
					instances: INSTANCES,
					compositions: COMPOSITIONS
				})
			);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/agents/${STORY_AGENT_ID_PARAM}`),
					params: { agentId: STORY_AGENT_ID_PARAM }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const deleteBtn = await waitFor(() => {
			const el = canvasElement.querySelector('[data-slot="agent-detail-delete-btn"]');
			if (!el) throw new Error('delete button not yet rendered');
			return el as HTMLButtonElement;
		});
		await userEvent.click(deleteBtn);
		const confirm = await waitFor(() => {
			const el = canvasElement.querySelector('[data-slot="agent-detail-delete-confirm"]');
			if (!el) throw new Error('delete-confirm panel not yet rendered');
			return el as HTMLElement;
		});
		globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (init?.method === 'DELETE' && url.includes('/api/agents/')) {
				return new Response(JSON.stringify({ error: 'agent store rejected the delete' }), {
					status: 503,
					statusText: 'Service Unavailable',
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404 });
		};
		const confirmBtn = Array.from(confirm.querySelectorAll('button')).find((b) =>
			/delete/i.test(b.textContent ?? '')
		) as HTMLButtonElement;
		await userEvent.click(confirmBtn);
		const err = await waitFor(
			() => {
				const hit = Array.from(confirm.querySelectorAll('p')).find(
					(p) =>
						p.className.includes('--status-danger-fg') && (p.textContent ?? '').trim().length > 0
				);
				if (!hit) throw new Error('delete-error not yet rendered');
				return hit as HTMLElement;
			},
			{ timeout: 3000 }
		);
		await expect(err).toBeInTheDocument();
		void canvas;
	}}
>
	{#snippet template()}
		<div class="h-[700px] w-[900px] overflow-hidden bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>
