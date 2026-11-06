<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import Page from './+page.svelte';
	import type { VolatileLinePattern } from '$lib/types';
	import {
		stubFetchJson,
		stubFetchError,
		stubFetchPending
	} from '$lib/test-support/storyFetchStubs';

	function buildPattern(
		overrides: Partial<VolatileLinePattern> & { id: string }
	): VolatileLinePattern {
		return {
			id: overrides.id,
			name: overrides.name ?? 'cwd-path',
			pattern: overrides.pattern ?? '^Working directory: /Users/[^/]+/projects/.*$',
			applies_to_block_index: overrides.applies_to_block_index ?? 2,
			enabled: overrides.enabled ?? true,
			notes: overrides.notes ?? null,
			created_at: overrides.created_at ?? '2026-05-10T09:00:00Z',
			updated_at: overrides.updated_at ?? '2026-05-15T14:30:00Z'
		};
	}

	const PATTERNS: VolatileLinePattern[] = [
		buildPattern({
			id: 'vlp-1',
			name: 'cwd-path',
			pattern: '^Working directory: /Users/[^/]+/projects/.*$',
			applies_to_block_index: 2,
			enabled: true,
			notes:
				'Strips the per-session working-directory line so the prompt class hash is stable across machines.'
		}),
		buildPattern({
			id: 'vlp-2',
			name: 'today-date',
			pattern: "Today's date is \\d{4}-\\d{2}-\\d{2}\\.",
			applies_to_block_index: 0,
			enabled: true,
			notes: 'Volatile date line injected into the billing header.'
		}),
		buildPattern({
			id: 'vlp-3',
			name: 'git-status-block',
			pattern: '^gitStatus: This is the git status[\\s\\S]*?(?=\\n\\n)',
			applies_to_block_index: 2,
			enabled: false,
			notes: null
		})
	];

	const { Story } = defineMeta({
		title: 'Routes/Prompts/VolatileLines',
		component: Page,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			stubFetchJson<VolatileLinePattern[]>(PATTERNS);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/prompts/volatile-lines'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[1000px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="Empty"
	loaders={[
		async () => {
			stubFetchJson<VolatileLinePattern[]>([]);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/prompts/volatile-lines'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[1000px] overflow-y-auto bg-[var(--surface-bg)]">
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
					url: new URL('http://localhost/prompts/volatile-lines'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[1000px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="ListError"
	loaders={[
		async () => {
			stubFetchError('volatile-line store unavailable');
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/prompts/volatile-lines'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[1000px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="FormErrorInteraction"
	loaders={[
		async () => {
			stubFetchJson<VolatileLinePattern[]>(PATTERNS);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/prompts/volatile-lines'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const addForm = await waitFor(() => {
			const el = canvasElement.querySelector('[data-slot="volatile-lines-add"] form');
			if (!el) throw new Error('add form not yet rendered');
			return el as HTMLFormElement;
		});
		const nameInput = addForm.querySelector('input[type="text"]') as HTMLInputElement;
		const patternInput = addForm.querySelector('textarea') as HTMLTextAreaElement;
		await userEvent.click(nameInput);
		await userEvent.type(nameInput, 'play-fn-pattern');
		await userEvent.click(patternInput);
		await userEvent.type(patternInput, '^play: .+$');
		globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (init?.method === 'POST' && url.includes('/volatile-lines')) {
				return new Response(JSON.stringify({ error: 'volatile-line store rejected the create' }), {
					status: 503,
					statusText: 'Service Unavailable',
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404 });
		};
		const submit = canvas.getByRole('button', { name: /add pattern/i });
		await userEvent.click(submit);
		const err = await waitFor(
			() => {
				const el = canvasElement.querySelector('[data-slot="volatile-lines-add-error"]');
				if (!el) throw new Error('add-error alert not yet rendered');
				return el as HTMLElement;
			},
			{ timeout: 3000 }
		);
		await expect(err).toBeInTheDocument();
		void canvas;
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[1000px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="IsEditingForm"
	loaders={[
		async () => {
			stubFetchJson<VolatileLinePattern[]>(PATTERNS);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/prompts/volatile-lines'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const editBtn = await waitFor(() => {
			const el = canvasElement.querySelector('button[aria-label="edit cwd-path"]');
			if (!el) throw new Error('edit button not yet rendered');
			return el as HTMLButtonElement;
		});
		await userEvent.click(editBtn);
		const editForm = await waitFor(
			() => {
				const el = canvasElement.querySelector('[data-slot="volatile-lines-edit-form"]');
				if (!el) throw new Error('edit form not yet rendered');
				return el as HTMLElement;
			},
			{ timeout: 3000 }
		);
		await expect(editForm).toBeInTheDocument();
		void canvas;
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[1000px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="EditErrorInteraction"
	loaders={[
		async () => {
			stubFetchJson<VolatileLinePattern[]>(PATTERNS);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/prompts/volatile-lines'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const editBtn = await waitFor(() => {
			const el = canvasElement.querySelector('button[aria-label="edit cwd-path"]');
			if (!el) throw new Error('edit button not yet rendered');
			return el as HTMLButtonElement;
		});
		await userEvent.click(editBtn);
		const editForm = await waitFor(() => {
			const el = canvasElement.querySelector('[data-slot="volatile-lines-edit-form"]');
			if (!el) throw new Error('edit form not yet rendered');
			return el as HTMLFormElement;
		});
		globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (init?.method === 'PATCH' && url.includes('/volatile-lines/')) {
				return new Response(JSON.stringify({ error: 'patch rejected: store unavailable' }), {
					status: 503,
					statusText: 'Service Unavailable',
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404 });
		};
		const saveBtn = editForm.querySelector('button[type="submit"]') as HTMLButtonElement;
		await userEvent.click(saveBtn);
		const err = await waitFor(
			() => {
				const alert = editForm.querySelector('[role="alert"]');
				if (!alert) throw new Error('edit-error alert not yet rendered');
				return alert as HTMLElement;
			},
			{ timeout: 3000 }
		);
		await expect(err).toBeInTheDocument();
		void canvas;
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[1000px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="ToggleErrorInteraction"
	loaders={[
		async () => {
			stubFetchJson<VolatileLinePattern[]>(PATTERNS);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/prompts/volatile-lines'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const toggle = await waitFor(() => {
			const el = canvasElement.querySelector('input[aria-label="toggle cwd-path"]');
			if (!el) throw new Error('toggle checkbox not yet rendered');
			return el as HTMLInputElement;
		});
		globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (init?.method === 'PATCH' && url.includes('/volatile-lines/')) {
				return new Response(JSON.stringify({ error: 'toggle rejected: store unavailable' }), {
					status: 503,
					statusText: 'Service Unavailable',
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404 });
		};
		await userEvent.click(toggle);
		const err = await waitFor(
			() => {
				const row = canvasElement.querySelector('[data-slot="volatile-lines-row"]');
				const alert = row?.querySelector('[role="alert"]');
				if (!alert) throw new Error('toggle-error alert not yet rendered');
				return alert as HTMLElement;
			},
			{ timeout: 3000 }
		);
		await expect(err).toBeInTheDocument();
		void canvas;
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[1000px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="DeleteErrorInteraction"
	loaders={[
		async () => {
			stubFetchJson<VolatileLinePattern[]>(PATTERNS);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/prompts/volatile-lines'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const deleteBtn = await waitFor(() => {
			const el = canvasElement.querySelector('button[aria-label="delete cwd-path"]');
			if (!el) throw new Error('delete button not yet rendered');
			return el as HTMLButtonElement;
		});
		await userEvent.click(deleteBtn);
		const confirmCard = await waitFor(
			() => {
				const el = canvasElement.querySelector('[data-slot="volatile-lines-delete-confirm"]');
				if (!el) throw new Error('delete-confirm card not yet rendered');
				return el as HTMLElement;
			},
			{ timeout: 3000 }
		);
		globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (init?.method === 'DELETE' && url.includes('/volatile-lines/')) {
				return new Response(JSON.stringify({ error: 'delete rejected: store unavailable' }), {
					status: 503,
					statusText: 'Service Unavailable',
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404 });
		};
		const confirmBtn = within(confirmCard).getByRole('button', {
			name: /confirm delete/i
		});
		await userEvent.click(confirmBtn);
		const err = await waitFor(
			() => {
				const alert = confirmCard.querySelector('[role="alert"]');
				if (!alert) throw new Error('delete-error alert not yet rendered');
				return alert as HTMLElement;
			},
			{ timeout: 3000 }
		);
		await expect(err).toHaveTextContent(/delete failed/i);
		void canvas;
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[1000px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>
