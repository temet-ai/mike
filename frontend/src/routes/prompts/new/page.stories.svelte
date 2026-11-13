<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import Page from './+page.svelte';
	import type { RequestDetail, SystemBlock } from '$lib/types';
	import { stubFetchJson, stubFetch404, stubFetchPending } from '$lib/test-support/storyFetchStubs';

	const SAMPLE_SHA = 'feeddeadbeef0123feeddeadbeef0123feeddeadbeef0123feeddeadbeef0123';

	const LONG_SKELETON =
		'You are a Frontend Development Expert — a senior engineer who builds ' +
		'production-grade UIs with Svelte 5, SvelteKit, and TypeScript.\n\n' +
		Array.from(
			{ length: 30 },
			(_, i) =>
				`Constraint ${i + 1}: full content renders; long bodies scroll, ` +
				'nothing is sliced in any inspection surface.'
		).join('\n');

	function sysBlock(index: number, text: string): SystemBlock {
		return { index, text, cache_control: null, composition_match: null };
	}

	function buildRequestDetail(opts: {
		promptSha256: string | null;
		skeleton: string;
	}): RequestDetail {
		return {
			id: 42,
			session_id: '01J9X4QF7H8K2N3P5R6T7V8W9Y-frontend-route-blitz',
			project: '/Users/dev/temet-proxy',
			human_prompt: 'Author the route-blitz storybook stories.',
			system_blocks: {
				prompt_sha256: opts.promptSha256,
				composition_id: null,
				composition_version: null,
				blocks: [
					sysBlock(0, 'Volatile billing header — injected, read-only.'),
					sysBlock(1, 'You are the identity preamble for the frontend developer agent.'),
					sysBlock(2, opts.skeleton)
				]
			}
		} as RequestDetail;
	}

	const { Story } = defineMeta({
		title: 'Routes/Prompts/New',
		component: Page,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/prompts/new'),
					params: {}
				}
			}
		}
	}}
/>

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
					url: new URL('http://localhost/prompts/new?from=request:42'),
					params: {}
				}
			}
		}
	}}
/>

<Story
	name="LoadError"
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
					url: new URL('http://localhost/prompts/new?from=request:42'),
					params: {}
				}
			}
		}
	}}
/>

<Story
	name="FormServerSha"
	loaders={[
		async () => {
			stubFetchJson<RequestDetail>(
				buildRequestDetail({
					promptSha256: SAMPLE_SHA,
					skeleton: LONG_SKELETON
				})
			);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/prompts/new?from=request:42'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[900px] w-[1000px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="FormComputedSha"
	loaders={[
		async () => {
			stubFetchJson<RequestDetail>(
				buildRequestDetail({
					promptSha256: null,
					skeleton: LONG_SKELETON
				})
			);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/prompts/new?from=request:42'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[900px] w-[1000px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="FormFallbackSha"
	loaders={[
		async () => {
			stubFetchJson<RequestDetail>(
				buildRequestDetail({
					promptSha256: null,
					skeleton: ''
				})
			);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/prompts/new?from=request:42'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[900px] w-[1000px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="WithPersona"
	loaders={[
		async () => {
			stubFetchJson<RequestDetail>(
				buildRequestDetail({
					promptSha256: SAMPLE_SHA,
					skeleton: LONG_SKELETON
				})
			);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(
						'http://localhost/prompts/new?from=request:42&persona=Frontend+Development+Expert'
					),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const input = await waitFor(
			() => {
				const el = canvasElement.querySelector<HTMLInputElement>('#prompt-new-label');
				if (el === null) throw new Error('label field not yet rendered');
				return el;
			},
			{ timeout: 3000 }
		);
		await waitFor(() => expect(input.value).toBe('Frontend Development Expert'));
		await userEvent.clear(input);
		await new Promise((r) => setTimeout(r, 50));
		await expect(input.value).toBe('');
		await userEvent.type(input, 'my-name');
		await expect(input.value).toBe('my-name');
	}}
>
	{#snippet template()}
		<div class="h-[900px] w-[1000px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="SaveErrorInteraction"
	loaders={[
		async () => {
			stubFetchJson<RequestDetail>(
				buildRequestDetail({
					promptSha256: SAMPLE_SHA,
					skeleton: LONG_SKELETON
				})
			);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(
						'http://localhost/prompts/new?from=request:42&persona=Frontend+Development+Expert'
					),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const save = await waitFor(
			() => {
				const btn = canvas.getByRole('button', { name: 'save' });
				if ((btn as HTMLButtonElement).disabled) {
					throw new Error('save not yet enabled (form still prefilling)');
				}
				return btn as HTMLButtonElement;
			},
			{ timeout: 3000 }
		);
		globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (init?.method === 'POST' && url.includes('/prompts/compositions')) {
				return new Response(JSON.stringify({ error: 'composition store rejected the create' }), {
					status: 503,
					statusText: 'Service Unavailable',
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404 });
		};
		await userEvent.click(save);
		const err = await waitFor(
			() => {
				const el = canvasElement.querySelector('[data-slot="prompt-new-save-error"]');
				if (!el) throw new Error('save-error alert not yet rendered');
				return el as HTMLElement;
			},
			{ timeout: 3000 }
		);
		await expect(err).toBeInTheDocument();
		await expect(err).toHaveTextContent(/save failed/i);
		void canvas;
	}}
>
	{#snippet template()}
		<div class="h-[900px] w-[1000px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>
