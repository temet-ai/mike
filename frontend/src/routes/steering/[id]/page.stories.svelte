<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { fn, userEvent, within, expect, waitFor } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import Page from './+page.svelte';
	import type { SteererDto } from '$lib/types';

	function hoursAgo(h: number): string {
		return new Date(Date.now() - (h + 0.5) * 3_600_000).toISOString();
	}

	function buildSteerer(overrides: Partial<SteererDto> = {}): SteererDto {
		return {
			id: 1,
			uuid: 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee',
			name: 'refocus',
			display_name: 'Refocus',
			description:
				'Re-focuses the session when it drifts away from the primary goal. Monitors for scope creep and injects a corrective reminder.',
			agent_id: 3,
			authority: 'supply_context',
			mode: 'augment',
			model: 'openai/gpt-5.6-luna',
			priority: 100,
			enabled: true,
			auto_enrol: false,
			prompt:
				'The session must stay focused on the assigned task. If it starts touching unrelated files or switching topics, redirect it.',
			project_id: null,
			project_path: null,
			trigger_point: 'response',
			invocation: 'every_turn',
			pattern: null,
			config: { confidence_threshold: 0.7, debounce_turns: 2 },
			created_at: '2026-06-01T10:00:00Z',
			updated_at: '2026-06-14T15:30:00Z',
			...overrides
		};
	}

	const PROJECTS_FIXTURE = [
		{
			project_path: '/Users/dev/projects/my-project',
			session_count: 12,
			request_count: 340,
			first_seen: '2026-05-01T00:00:00Z',
			last_seen: hoursAgo(3),
			pinned: false,
			custom_name: null,
			aliases: []
		},
		{
			project_path: '/Users/dev/projects/other-project',
			session_count: 4,
			request_count: 88,
			first_seen: '2026-05-10T00:00:00Z',
			last_seen: hoursAgo(25),
			pinned: false,
			custom_name: null,
			aliases: []
		}
	];

	function stubGetSteerer(steerer: SteererDto): void {
		globalThis.fetch = async (input: RequestInfo | URL) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (url.includes('/api/steerers/')) {
				return new Response(JSON.stringify(steerer), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.includes('/api/projects')) {
				return new Response(JSON.stringify(PROJECTS_FIXTURE), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.includes('/api/sessions')) {
				return new Response(JSON.stringify([]), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404 });
		};
	}

	function stubGetSteerer404(): void {
		globalThis.fetch = async () =>
			new Response(JSON.stringify({ error: 'no steerer with id 999' }), {
				status: 404,
				statusText: 'Not Found',
				headers: { 'content-type': 'application/json' }
			});
	}

	function stubGetSteererError(): void {
		globalThis.fetch = async () =>
			new Response(JSON.stringify({ error: 'steerer store unavailable' }), {
				status: 503,
				statusText: 'Service Unavailable',
				headers: { 'content-type': 'application/json' }
			});
	}

	function stubGetThenPatchError(steerer: SteererDto): void {
		globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (url.includes('/api/steerers/')) {
				if (init?.method === 'PATCH') {
					return new Response(JSON.stringify({ error: 'steerer store unavailable' }), {
						status: 503,
						statusText: 'Service Unavailable',
						headers: { 'content-type': 'application/json' }
					});
				}
				return new Response(JSON.stringify(steerer), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404 });
		};
	}

	function stubGetThenPatchWithWarnings(steerer: SteererDto): void {
		globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (url.includes('/api/steerers/')) {
				if (init?.method === 'PATCH') {
					return new Response(
						JSON.stringify({
							steerer,
							warnings: [
								'Model "openai/gpt-5.6-luna-legacy" could not be verified against the current model list — ensure it resolves via OpenRouter.',
								'Confidence threshold 0.95 is unusually high; the steerer may fire infrequently.'
							]
						}),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				return new Response(JSON.stringify(steerer), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404 });
		};
	}

	let lastPatchBody: string | null = null;

	function stubGetThenPatchOk(steerer: SteererDto): void {
		lastPatchBody = null;
		globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (url.includes('/api/steerers/')) {
				if (init?.method === 'PATCH') {
					lastPatchBody = typeof init.body === 'string' ? init.body : null;
					return new Response(JSON.stringify({ steerer, warnings: [] }), {
						status: 200,
						headers: { 'content-type': 'application/json' }
					});
				}
				return new Response(JSON.stringify(steerer), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.includes('/api/projects')) {
				return new Response(JSON.stringify(PROJECTS_FIXTURE), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404 });
		};
	}

	function stubSessionsList(): void {
		globalThis.fetch = async (input: RequestInfo | URL) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (url.includes('/api/sessions')) {
				return new Response(
					JSON.stringify([
						{
							id: '64b50581-7065-4013-8f77-dd7d8adeddb4',
							project: '/Users/dev/projects/my-project',
							title: 'Implement steering backend',
							first_seen: '2026-06-14T09:00:00Z',
							last_seen: hoursAgo(3),
							request_count: 84,
							models: ['claude-sonnet-4-5'],
							last_status: 'Complete'
						},
						{
							id: 'a1b2c3d4-e5f6-7890-abcd-ef1234567890',
							project: '/Users/dev/projects/other-project',
							title: 'Fix routing bug',
							first_seen: '2026-06-13T10:00:00Z',
							last_seen: hoursAgo(25),
							request_count: 41,
							models: ['claude-opus-4-5'],
							last_status: 'Complete'
						}
					]),
					{ status: 200, headers: { 'content-type': 'application/json' } }
				);
			}
			return new Response('Not Found', { status: 404 });
		};
	}

	const { Story } = defineMeta({
		title: 'Routes/Steering/Editor',
		component: Page,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' }
	});
</script>

<Story
	name="EditLoaded"
	tags={['data-driven']}
	loaders={[
		async () => {
			stubGetSteerer(buildSteerer());
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/1'),
					params: { id: '1' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const select = await waitFor(
			() => {
				const el = canvasElement.querySelector('#se-project-scope') as HTMLSelectElement | null;
				if (!el || el.options.length < 3) throw new Error('project scope select not yet populated');
				return el;
			},
			{ timeout: 5000 }
		);
		await expect(select.value).toBe('');
		await expect(select.options.item(0)?.textContent).toBe('Global (all projects)');
		const optionValues = Array.from(select.options).map((o) => o.value);
		await expect(optionValues).toContain('/Users/dev/projects/my-project');
		await expect(optionValues).toContain('/Users/dev/projects/other-project');
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="EditCommand"
	tags={['data-driven']}
	loaders={[
		async () => {
			stubGetSteerer(
				buildSteerer({
					authority: 'command',
					mode: 'drive',
					display_name: 'Override Agent',
					name: 'override-agent',
					description: 'Full takeover steerer — reserved for emergency course corrections.'
				})
			);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/2'),
					params: { id: '2' }
				}
			}
		}
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="EditDisabled"
	tags={['data-driven']}
	loaders={[
		async () => {
			stubGetSteerer(
				buildSteerer({ enabled: false, display_name: 'Disabled Steerer', name: 'disabled-steerer' })
			);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/3'),
					params: { id: '3' }
				}
			}
		}
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="Loading"
	tags={['data-driven']}
	loaders={[
		async () => {
			globalThis.fetch = () => new Promise(() => {});
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/1'),
					params: { id: '1' }
				}
			}
		}
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="LoadError"
	tags={['data-driven']}
	loaders={[
		async () => {
			stubGetSteererError();
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/1'),
					params: { id: '1' }
				}
			}
		}
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="LoadNotFound"
	tags={['data-driven']}
	loaders={[
		async () => {
			stubGetSteerer404();
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/999'),
					params: { id: '999' }
				}
			}
		}
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="NewBlank"
	tags={['data-driven']}
	loaders={[
		async () => {
			globalThis.fetch = async () => new Response('Not Found', { status: 404 });
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/new?mode=blank'),
					params: { id: 'new' }
				}
			}
		}
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="GuidedStep1"
	tags={['data-driven']}
	loaders={[
		async () => {
			stubSessionsList();
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/new?mode=guided'),
					params: { id: 'new' }
				}
			}
		}
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="GuidedStep1Loading"
	tags={['data-driven']}
	loaders={[
		async () => {
			globalThis.fetch = () => new Promise(() => {});
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/new?mode=guided'),
					params: { id: 'new' }
				}
			}
		}
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="GuidedStep1Error"
	tags={['data-driven']}
	loaders={[
		async () => {
			globalThis.fetch = async () =>
				new Response(JSON.stringify({ error: 'sessions unavailable' }), {
					status: 503,
					statusText: 'Service Unavailable',
					headers: { 'content-type': 'application/json' }
				});
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/new?mode=guided'),
					params: { id: 'new' }
				}
			}
		}
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="VisualEditLoaded"
	tags={['visual', 'data-driven']}
	loaders={[
		async () => {
			stubGetSteerer(buildSteerer());
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/1'),
					params: { id: '1' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		await waitFor(
			() => {
				const input = canvasElement.querySelector('#se-display-name') as HTMLInputElement | null;
				if (!input || !input.value) throw new Error('form not yet populated');
				return input;
			},
			{ timeout: 5000 }
		);
		await captureStorySnapshot(canvasElement, 'page/EditLoaded');
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="VisualNewBlank"
	tags={['visual', 'data-driven']}
	loaders={[
		async () => {
			globalThis.fetch = async () => new Response('Not Found', { status: 404 });
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/new?mode=blank'),
					params: { id: 'new' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		await waitFor(
			() => {
				const onboarding = canvasElement.querySelector('.se-onboarding');
				if (!onboarding) throw new Error('onboarding card not yet rendered');
				return onboarding;
			},
			{ timeout: 3000 }
		);
		await captureStorySnapshot(canvasElement, 'page/NewBlank');
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="VisualGuidedStep1"
	tags={['visual', 'data-driven']}
	loaders={[
		async () => {
			stubSessionsList();
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/new?mode=guided'),
					params: { id: 'new' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		await waitFor(
			() => {
				const list = canvasElement.querySelector('[role="listbox"]');
				if (!list || list.querySelectorAll('[role="option"]').length === 0)
					throw new Error('session list not yet populated');
				return list;
			},
			{ timeout: 5000 }
		);
		await captureStorySnapshot(canvasElement, 'page/GuidedStep1');
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="VisualEditCommand"
	tags={['visual', 'data-driven']}
	loaders={[
		async () => {
			stubGetSteerer(
				buildSteerer({
					authority: 'command',
					mode: 'drive',
					display_name: 'Override Agent',
					name: 'override-agent',
					description: 'Full takeover steerer — reserved for emergency course corrections.'
				})
			);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/2'),
					params: { id: '2' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		await waitFor(
			() => {
				const input = canvasElement.querySelector('#se-display-name') as HTMLInputElement | null;
				if (!input || !input.value) throw new Error('form not yet populated');
				return input;
			},
			{ timeout: 5000 }
		);
		await captureStorySnapshot(canvasElement, 'page/EditCommand');
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="VisualEditDisabled"
	tags={['visual', 'data-driven']}
	loaders={[
		async () => {
			stubGetSteerer(
				buildSteerer({ enabled: false, display_name: 'Disabled Steerer', name: 'disabled-steerer' })
			);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/3'),
					params: { id: '3' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		await waitFor(
			() => {
				const toggle = canvasElement.querySelector('#se-enabled') as HTMLButtonElement | null;
				if (!toggle) throw new Error('toggle not yet rendered');
				if (toggle.getAttribute('aria-pressed') !== 'false')
					throw new Error('toggle not yet in off state');
				return toggle;
			},
			{ timeout: 5000 }
		);
		await captureStorySnapshot(canvasElement, 'page/EditDisabled');
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="VisualGuidedStep3"
	tags={['visual', 'data-driven', 'interactive']}
	loaders={[
		async () => {
			globalThis.fetch = async (input: RequestInfo | URL) => {
				const url = typeof input === 'string' ? input : input.toString();
				if (url.includes('/api/sessions') && url.includes('/overview')) {
					return new Response(
						JSON.stringify({
							session_id: '64b50581-7065-4013-8f77-dd7d8adeddb4',
							total_turns: 0,
							turns: []
						}),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				if (url.includes('/api/sessions')) {
					return new Response(
						JSON.stringify([
							{
								id: '64b50581-7065-4013-8f77-dd7d8adeddb4',
								project: '/Users/dev/projects/my-project',
								title: 'Implement steering backend',
								first_seen: '2026-06-14T09:00:00Z',
								last_seen: hoursAgo(3),
								request_count: 84,
								models: ['claude-sonnet-4-5'],
								last_status: 'Complete'
							}
						]),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				return new Response('Not Found', { status: 404 });
			};
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/new?mode=guided'),
					params: { id: 'new' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const sessionRow = await waitFor(
			() => {
				const rows = canvasElement.querySelectorAll('[role="option"]');
				if (rows.length === 0) throw new Error('session rows not yet rendered');
				return rows[0] as HTMLButtonElement;
			},
			{ timeout: 5000 }
		);
		await userEvent.click(sessionRow);
		const nextBtn1 = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /next/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Next button not found');
				return el;
			},
			{ timeout: 2000 }
		);
		await userEvent.click(nextBtn1);
		await waitFor(
			() => {
				const heading = Array.from(canvasElement.querySelectorAll('h3')).find((h) =>
					/what is session/i.test(h.textContent ?? '')
				);
				if (!heading) throw new Error('Step 2 not rendered');
				return heading;
			},
			{ timeout: 5000 }
		);
		const nextBtn2 = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /next.*steer/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Next: Define the steer button not found');
				return el;
			},
			{ timeout: 3000 }
		);
		await userEvent.click(nextBtn2);
		await waitFor(
			() => {
				const heading = Array.from(canvasElement.querySelectorAll('h3')).find((h) =>
					/what should the steerer watch for/i.test(h.textContent ?? '')
				);
				if (!heading) throw new Error('Step 3 not rendered');
				return heading;
			},
			{ timeout: 5000 }
		);
		await captureStorySnapshot(canvasElement, 'page/GuidedStep3');
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="VisualGuidedStep4"
	tags={['visual', 'data-driven', 'interactive']}
	loaders={[
		async () => {
			globalThis.fetch = async (input: RequestInfo | URL) => {
				const url = typeof input === 'string' ? input : input.toString();
				if (url.includes('/api/sessions') && url.includes('/overview')) {
					return new Response(
						JSON.stringify({
							session_id: '64b50581-7065-4013-8f77-dd7d8adeddb4',
							total_turns: 0,
							turns: []
						}),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				if (url.includes('/api/sessions')) {
					return new Response(
						JSON.stringify([
							{
								id: '64b50581-7065-4013-8f77-dd7d8adeddb4',
								project: '/Users/dev/projects/my-project',
								title: 'Implement steering backend',
								first_seen: '2026-06-14T09:00:00Z',
								last_seen: hoursAgo(3),
								request_count: 84,
								models: ['claude-sonnet-4-5'],
								last_status: 'Complete'
							}
						]),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				return new Response('Not Found', { status: 404 });
			};
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/new?mode=guided'),
					params: { id: 'new' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const sessionRow = await waitFor(
			() => {
				const rows = canvasElement.querySelectorAll('[role="option"]');
				if (rows.length === 0) throw new Error('session rows not yet rendered');
				return rows[0] as HTMLButtonElement;
			},
			{ timeout: 5000 }
		);
		await userEvent.click(sessionRow);
		const nextBtn1 = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /next/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Next button not found');
				return el;
			},
			{ timeout: 2000 }
		);
		await userEvent.click(nextBtn1);
		await waitFor(
			() => {
				const heading = Array.from(canvasElement.querySelectorAll('h3')).find((h) =>
					/what is session/i.test(h.textContent ?? '')
				);
				if (!heading) throw new Error('Step 2 not rendered');
				return heading;
			},
			{ timeout: 5000 }
		);
		const nextBtn2 = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /next.*steer/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Next: Define the steer button not found');
				return el;
			},
			{ timeout: 3000 }
		);
		await userEvent.click(nextBtn2);
		await waitFor(
			() => {
				const heading = Array.from(canvasElement.querySelectorAll('h3')).find((h) =>
					/what should the steerer watch for/i.test(h.textContent ?? '')
				);
				if (!heading) throw new Error('Step 3 not rendered');
				return heading;
			},
			{ timeout: 5000 }
		);
		const nextBtn3 = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /review.*create/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Review + create button not found');
				return el;
			},
			{ timeout: 3000 }
		);
		await userEvent.click(nextBtn3);
		await waitFor(
			() => {
				const card = canvasElement.querySelector('.gf-summary-card') as HTMLElement | null;
				if (!card) throw new Error('summary card not yet rendered');
				return card;
			},
			{ timeout: 5000 }
		);
		await captureStorySnapshot(canvasElement, 'page/GuidedStep4');
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="EnabledToggleInteraction"
	tags={['interactive']}
	loaders={[
		async () => {
			stubGetSteerer(buildSteerer({ enabled: true }));
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/1'),
					params: { id: '1' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const toggle = await waitFor(
			() => {
				const el = canvasElement.querySelector('#se-enabled') as HTMLButtonElement | null;
				if (!el) throw new Error('enabled toggle not yet rendered');
				return el;
			},
			{ timeout: 5000 }
		);
		await expect(toggle.getAttribute('aria-pressed')).toBe('true');
		await userEvent.click(toggle);
		await expect(toggle.getAttribute('aria-pressed')).toBe('false');
		await userEvent.click(toggle);
		await expect(toggle.getAttribute('aria-pressed')).toBe('true');
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="SavedOkInteraction"
	tags={['interactive']}
	loaders={[
		async () => {
			stubGetThenPatchOk(buildSteerer());
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/1'),
					params: { id: '1' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		await waitFor(
			() => {
				const input = canvasElement.querySelector('#se-display-name') as HTMLInputElement | null;
				if (!input || !input.value) throw new Error('form not yet populated');
				return input;
			},
			{ timeout: 5000 }
		);
		const scopeSelect = await waitFor(
			() => {
				const el = canvasElement.querySelector('#se-project-scope') as HTMLSelectElement | null;
				if (!el || el.options.length < 3) throw new Error('project scope select not yet populated');
				return el;
			},
			{ timeout: 5000 }
		);
		await expect(scopeSelect.value).toBe('');
		await userEvent.selectOptions(scopeSelect, '/Users/dev/projects/my-project');
		await expect(scopeSelect.value).toBe('/Users/dev/projects/my-project');
		const saveBtn = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /save changes/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Save button not found');
				return el;
			},
			{ timeout: 3000 }
		);
		await userEvent.click(saveBtn);
		const banner = await waitFor(
			() => {
				const el = canvasElement.querySelector('.se-banner--ok') as HTMLElement | null;
				if (!el) throw new Error('Saved. banner not yet rendered');
				return el;
			},
			{ timeout: 5000 }
		);
		await expect(banner.textContent?.trim()).toBe('Saved.');
		if (!lastPatchBody) throw new Error('PATCH body was not captured');
		const patchPayload = JSON.parse(lastPatchBody) as { project_path?: string | null };
		await expect(patchPayload.project_path).toBe('/Users/dev/projects/my-project');
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="SavedWarningsInteraction"
	tags={['interactive']}
	loaders={[
		async () => {
			stubGetThenPatchWithWarnings(
				buildSteerer({
					model: 'openai/gpt-5.6-luna-legacy',
					config: { confidence_threshold: 0.95, debounce_turns: 2 }
				})
			);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/1'),
					params: { id: '1' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		await waitFor(
			() => {
				const input = canvasElement.querySelector('#se-display-name') as HTMLInputElement | null;
				if (!input || !input.value) throw new Error('form not yet populated');
				return input;
			},
			{ timeout: 5000 }
		);
		const saveBtn = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /save changes/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Save button not found');
				return el;
			},
			{ timeout: 3000 }
		);
		await userEvent.click(saveBtn);
		const banner = await waitFor(
			() => {
				const el = canvasElement.querySelector('.se-banner--warn') as HTMLElement | null;
				if (!el) throw new Error('warning banner not yet rendered');
				return el;
			},
			{ timeout: 5000 }
		);
		const items = banner.querySelectorAll('.se-warning-list li');
		await expect(items.length).toBeGreaterThanOrEqual(2);
		const canvas = within(canvasElement);
		await expect(
			canvas.getByText(
				'Model "openai/gpt-5.6-luna-legacy" could not be verified against the current model list — ensure it resolves via OpenRouter.',
				{ exact: true }
			)
		).toBeInTheDocument();
		await expect(
			canvas.getByText(
				'Confidence threshold 0.95 is unusually high; the steerer may fire infrequently.',
				{ exact: true }
			)
		).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="GuidedStep1SelectSession"
	tags={['interactive']}
	loaders={[
		async () => {
			stubSessionsList();
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/new?mode=guided'),
					params: { id: 'new' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const sessionRow = await waitFor(
			() => {
				const rows = canvasElement.querySelectorAll('[role="option"]');
				if (rows.length === 0) throw new Error('session rows not yet rendered');
				return rows[0] as HTMLButtonElement;
			},
			{ timeout: 5000 }
		);
		await expect(sessionRow.getAttribute('aria-selected')).toBe('false');
		await userEvent.click(sessionRow);
		await expect(sessionRow.getAttribute('aria-selected')).toBe('true');
		const nextBtn = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /next/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Next button not found');
				return el;
			},
			{ timeout: 2000 }
		);
		await expect(nextBtn.disabled).toBe(false);
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="GuidedStep2Navigation"
	tags={['interactive']}
	loaders={[
		async () => {
			globalThis.fetch = async (input: RequestInfo | URL) => {
				const url = typeof input === 'string' ? input : input.toString();
				if (url.includes('/api/sessions') && url.includes('/overview')) {
					return new Response(
						JSON.stringify({
							session_id: '64b50581-7065-4013-8f77-dd7d8adeddb4',
							total_turns: 0,
							turns: []
						}),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				if (url.includes('/api/sessions')) {
					return new Response(
						JSON.stringify([
							{
								id: '64b50581-7065-4013-8f77-dd7d8adeddb4',
								project: '/Users/dev/projects/my-project',
								title: 'Implement steering backend',
								first_seen: '2026-06-14T09:00:00Z',
								last_seen: hoursAgo(3),
								request_count: 84,
								models: ['claude-sonnet-4-5'],
								last_status: 'Complete'
							}
						]),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				return new Response('Not Found', { status: 404 });
			};
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/new?mode=guided'),
					params: { id: 'new' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const sessionRow = await waitFor(
			() => {
				const rows = canvasElement.querySelectorAll('[role="option"]');
				if (rows.length === 0) throw new Error('session rows not yet rendered');
				return rows[0] as HTMLButtonElement;
			},
			{ timeout: 5000 }
		);
		await userEvent.click(sessionRow);
		const nextBtn = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /next/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Next button not found');
				return el;
			},
			{ timeout: 2000 }
		);
		await userEvent.click(nextBtn);
		const step2Heading = await waitFor(
			() => {
				const headings = Array.from(canvasElement.querySelectorAll('h3'));
				const el = headings.find((h) => /what is session/i.test(h.textContent ?? '')) as
					HTMLElement | undefined;
				if (!el) throw new Error('Step 2 heading not yet rendered');
				return el;
			},
			{ timeout: 5000 }
		);
		await expect(step2Heading.textContent?.toLowerCase()).toContain('session');
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="GuidedStep3Navigation"
	tags={['interactive']}
	loaders={[
		async () => {
			globalThis.fetch = async (input: RequestInfo | URL) => {
				const url = typeof input === 'string' ? input : input.toString();
				if (url.includes('/api/sessions') && url.includes('/overview')) {
					return new Response(
						JSON.stringify({
							session_id: '64b50581-7065-4013-8f77-dd7d8adeddb4',
							total_turns: 2,
							turns: [
								{
									turn_id: 2,
									user_prompt: 'Add the enrol endpoint',
									stop_response: 'Done.',
									created_at: hoursAgo(7)
								},
								{
									turn_id: 1,
									user_prompt: 'Implement the steering backend',
									stop_response: 'Working on it...',
									created_at: hoursAgo(9)
								}
							]
						}),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				if (url.includes('/api/sessions')) {
					return new Response(
						JSON.stringify([
							{
								id: '64b50581-7065-4013-8f77-dd7d8adeddb4',
								project: '/Users/dev/projects/my-project',
								title: 'Implement steering backend',
								first_seen: '2026-06-14T09:00:00Z',
								last_seen: hoursAgo(3),
								request_count: 84,
								models: ['claude-sonnet-4-5'],
								last_status: 'Complete'
							}
						]),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				return new Response('Not Found', { status: 404 });
			};
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/new?mode=guided'),
					params: { id: 'new' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const sessionRow = await waitFor(
			() => {
				const rows = canvasElement.querySelectorAll('[role="option"]');
				if (rows.length === 0) throw new Error('session rows not rendered');
				return rows[0] as HTMLButtonElement;
			},
			{ timeout: 5000 }
		);
		await userEvent.click(sessionRow);
		const nextBtn1 = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /next/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Next button not found');
				return el;
			},
			{ timeout: 2000 }
		);
		await userEvent.click(nextBtn1);
		await waitFor(
			() => {
				const heading = Array.from(canvasElement.querySelectorAll('h3')).find((h) =>
					/what is session/i.test(h.textContent ?? '')
				);
				if (!heading) throw new Error('Step 2 not rendered');
				return heading;
			},
			{ timeout: 5000 }
		);
		const nextBtn2 = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /next.*steer/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Next: Define the steer button not found');
				return el;
			},
			{ timeout: 3000 }
		);
		await userEvent.click(nextBtn2);
		const step3Heading = await waitFor(
			() => {
				const heading = Array.from(canvasElement.querySelectorAll('h3')).find((h) =>
					/what should the steerer watch for/i.test(h.textContent ?? '')
				) as HTMLElement | undefined;
				if (!heading) throw new Error('Step 3 heading not rendered');
				return heading;
			},
			{ timeout: 5000 }
		);
		await expect(step3Heading).toBeInTheDocument();
		const intentTextarea = await waitFor(
			() => {
				const el = canvasElement.querySelector('#gf-intent') as HTMLTextAreaElement | null;
				if (!el) throw new Error('#gf-intent textarea not rendered in Step 3');
				return el;
			},
			{ timeout: 3000 }
		);
		await expect(intentTextarea).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="SlugAutoFillInteraction"
	tags={['interactive']}
	loaders={[
		async () => {
			globalThis.fetch = async () => new Response('Not Found', { status: 404 });
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/new?mode=blank'),
					params: { id: 'new' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const displayNameInput = await waitFor(
			() => {
				const el = canvasElement.querySelector('#se-display-name') as HTMLInputElement | null;
				if (!el) throw new Error('display name input not rendered');
				return el;
			},
			{ timeout: 3000 }
		);
		const slugInput = canvasElement.querySelector('#se-slug') as HTMLInputElement;
		await userEvent.clear(displayNameInput);
		const nativeInputValueSetter = Object.getOwnPropertyDescriptor(
			window.HTMLInputElement.prototype,
			'value'
		)?.set;
		if (nativeInputValueSetter) {
			nativeInputValueSetter.call(displayNameInput, 'My Focus Steerer');
			displayNameInput.dispatchEvent(new Event('input', { bubbles: true }));
		} else {
			await userEvent.click(displayNameInput);
			await userEvent.paste('My Focus Steerer');
		}
		await waitFor(
			() => {
				if (slugInput.value !== 'my-focus-steerer')
					throw new Error(`slug not yet synced, got: "${slugInput.value}"`);
				return slugInput;
			},
			{ timeout: 3000 }
		);
		await expect(slugInput.value).toBe('my-focus-steerer');
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="SlugValidationErrorInteraction"
	tags={['interactive']}
	loaders={[
		async () => {
			globalThis.fetch = async () => new Response('Not Found', { status: 404 });
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/new?mode=blank'),
					params: { id: 'new' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		await waitFor(
			() => {
				const el = canvasElement.querySelector('#se-slug');
				if (!el) throw new Error('slug input not rendered');
				return el;
			},
			{ timeout: 3000 }
		);
		const slugInput = canvasElement.querySelector('#se-slug') as HTMLInputElement;
		await userEvent.click(slugInput);
		await userEvent.keyboard('{Control>}a{/Control}');
		await userEvent.keyboard('{Delete}');
		const createBtn = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /create steerer/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Create steerer button not found');
				return el;
			},
			{ timeout: 2000 }
		);
		await userEvent.click(createBtn);
		const slugError = await waitFor(
			() => {
				const el = canvasElement.querySelector('.se-field-error') as HTMLElement | null;
				if (!el) throw new Error('slug error not rendered');
				return el;
			},
			{ timeout: 3000 }
		);
		await expect(slugError).toBeInTheDocument();
		await expect(slugError.textContent?.trim()).toBe('Slug is required.');
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="LongPromptAnchor"
	tags={['data-driven', 'interactive']}
	loaders={[
		async () => {
			stubGetSteerer(
				buildSteerer({
					display_name: 'Comprehensive Scope Guard',
					name: 'comprehensive-scope-guard',
					description:
						'Monitors the session for ANY deviation from the assigned task scope. This includes switching to unrelated files, refactoring code not on the task list, running non-essential commands, or spending more than one turn on a tangential concern without explicit operator approval.',
					prompt:
						'The session is assigned to implement the steering backend in crates/mike/src/api/steering.rs and crates/mike/src/store/steering.rs. The following deviations are NOT permitted: (1) editing any file outside those two modules and their direct test files without operator approval; (2) running cargo build --all when only -p mike is needed; (3) touching the frontend without an explicit operator instruction; (4) creating new DB migrations not on the sprint plan; (5) installing new Cargo dependencies without asking. If the session touches any of these prohibited areas, the steerer must fire immediately with authority=command and inject a binding directive to return to scope.'
				})
			);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/1'),
					params: { id: '1' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const fullPrompt =
			'The session is assigned to implement the steering backend in crates/mike/src/api/steering.rs and crates/mike/src/store/steering.rs. The following deviations are NOT permitted: (1) editing any file outside those two modules and their direct test files without operator approval; (2) running cargo build --all when only -p mike is needed; (3) touching the frontend without an explicit operator instruction; (4) creating new DB migrations not on the sprint plan; (5) installing new Cargo dependencies without asking. If the session touches any of these prohibited areas, the steerer must fire immediately with authority=command and inject a binding directive to return to scope.';
		const textarea = await waitFor(
			() => {
				const el = canvasElement.querySelector('#se-prompt') as HTMLTextAreaElement | null;
				if (!el || el.value !== fullPrompt)
					throw new Error('prompt textarea not yet populated with full text');
				return el;
			},
			{ timeout: 5000 }
		);
		await expect(textarea.value).toBe(fullPrompt);
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="AuthorityModeSelectInteraction"
	tags={['interactive', 'data-driven']}
	loaders={[
		async () => {
			stubGetSteerer(buildSteerer({ authority: 'supply_context', mode: 'augment' }));
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/1'),
					params: { id: '1' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		await waitFor(
			() => {
				const el = canvasElement.querySelector('#se-authority') as HTMLSelectElement | null;
				if (!el) throw new Error('authority select not yet rendered');
				return el;
			},
			{ timeout: 5000 }
		);
		const authoritySelect = canvasElement.querySelector('#se-authority') as HTMLSelectElement;
		const modeSelect = canvasElement.querySelector('#se-mode') as HTMLSelectElement;
		await expect(authoritySelect.value).toBe('supply_context');
		await expect(modeSelect.value).toBe('augment');
		await userEvent.selectOptions(authoritySelect, 'command');
		await expect(authoritySelect.value).toBe('command');
		await userEvent.selectOptions(modeSelect, 'drive');
		await expect(modeSelect.value).toBe('drive');
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="ModelPickerInteraction"
	tags={['interactive', 'data-driven']}
	loaders={[
		async () => {
			globalThis.fetch = async (input: RequestInfo | URL) => {
				const url = typeof input === 'string' ? input : input.toString();
				if (url.includes('/api/steerers/')) {
					return new Response(JSON.stringify(buildSteerer()), {
						status: 200,
						headers: { 'content-type': 'application/json' }
					});
				}
				if (url.includes('/api/models')) {
					return new Response(
						JSON.stringify({
							models: [
								{
									id: 'openai/gpt-5.6-luna',
									name: 'GPT 5.6 Luna',
									context_length: 128000,
									prompt_price_per_1m: 0.15,
									completion_price_per_1m: 0.6,
									known_good: true,
									supports_structured_output: true,
									description:
										'Fast, low-cost multimodal model — a strong default for steerer verdicts.',
									input_modalities: ['text', 'image'],
									supports_tools: true,
									supports_reasoning: false
								},
								{
									id: 'google/gemini-3.5-flash-lite',
									name: 'Gemini 3.5 Flash Lite',
									context_length: 200000,
									prompt_price_per_1m: 0.8,
									completion_price_per_1m: 4.0,
									known_good: true,
									supports_structured_output: true
								},
								{
									id: 'google/gemini-3.8-flash',
									name: 'Gemini 3.8 Flash',
									context_length: 200000,
									prompt_price_per_1m: 3.0,
									completion_price_per_1m: 15.0,
									known_good: false,
									supports_structured_output: true,
									supports_tools: true,
									supports_reasoning: true
								}
							]
						}),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				return new Response('Not Found', { status: 404 });
			};
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/1'),
					params: { id: '1' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		await waitFor(
			() => {
				const el = canvasElement.querySelector('#se-model') as HTMLInputElement | null;
				if (!el || !el.value) throw new Error('model input not yet populated');
				return el;
			},
			{ timeout: 5000 }
		);
		const selectBtn = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /select/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Select (picker) button not found');
				return el;
			},
			{ timeout: 3000 }
		);
		await userEvent.click(selectBtn);
		const searchInput = await waitFor(
			() => {
				const el = document.body.querySelector('.mpp-search') as HTMLInputElement | null;
				if (!el) throw new Error('model picker search input not rendered');
				return el;
			},
			{ timeout: 3000 }
		);
		await expect(searchInput).toBeInTheDocument();
		const firstPresetRow = await waitFor(
			() => {
				const rows = document.body.querySelectorAll('.mpp-row:not(.mpp-row--inherit)');
				if (rows.length === 0) throw new Error('no model rows yet rendered');
				return rows[0] as HTMLButtonElement;
			},
			{ timeout: 5000 }
		);
		const firstModelName = firstPresetRow.querySelector('.mpp-name')?.textContent?.trim() ?? '';
		await expect(firstModelName).toBeTruthy();
		await userEvent.click(firstPresetRow);
		const modelInput = canvasElement.querySelector('#se-model') as HTMLInputElement;
		await expect(modelInput.value).toBe(firstModelName);
		await waitFor(
			() => {
				const el = document.body.querySelector('.mpp-search');
				if (el) throw new Error('popover still open');
				return true;
			},
			{ timeout: 3000 }
		);
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="ModelPickerSortInteraction"
	tags={['interactive', 'data-driven']}
	loaders={[
		async () => {
			globalThis.fetch = async (input: RequestInfo | URL) => {
				const url = typeof input === 'string' ? input : input.toString();
				if (url.includes('/api/steerers/')) {
					return new Response(JSON.stringify(buildSteerer({ model: '' })), {
						status: 200,
						headers: { 'content-type': 'application/json' }
					});
				}
				if (url.includes('/api/models')) {
					return new Response(
						JSON.stringify({
							models: [
								{
									id: 'openai/gpt-5.6-luna',
									name: 'GPT 5.6 Luna',
									context_length: 128000,
									prompt_price_per_1m: 0.15,
									completion_price_per_1m: 0.6,
									known_good: false,
									supports_structured_output: true
								},
								{
									id: 'google/gemini-3.8-flash',
									name: 'Gemini 3.8 Flash',
									context_length: 200000,
									prompt_price_per_1m: 3.0,
									completion_price_per_1m: 15.0,
									known_good: false,
									supports_structured_output: true,
									description: 'Google mid-tier model — stronger judgment at a higher price.',
									supports_tools: true,
									supports_reasoning: true
								}
							]
						}),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				return new Response('Not Found', { status: 404 });
			};
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/1'),
					params: { id: '1' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		await waitFor(
			() => {
				const el = canvasElement.querySelector('#se-display-name') as HTMLInputElement | null;
				if (!el || !el.value) throw new Error('form not yet populated');
				return el;
			},
			{ timeout: 5000 }
		);
		const selectBtn = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /select/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Select button not found');
				return el;
			},
			{ timeout: 3000 }
		);
		await userEvent.click(selectBtn);
		await waitFor(
			() => {
				const rows = document.body.querySelectorAll('.mpp-row');
				if (rows.length === 0) throw new Error('no model rows yet');
				return rows;
			},
			{ timeout: 5000 }
		);
		const inHeader = await waitFor(
			() => {
				const ths = Array.from(document.body.querySelectorAll('.mpp-th'));
				const el = ths.find((h) => /\$\/M in/i.test(h.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('$/M in column header not found');
				return el;
			},
			{ timeout: 3000 }
		);
		const firstRowBeforeSort = document.body.querySelector('.mpp-list .mpp-row .mpp-name');
		const nameBeforeSort = firstRowBeforeSort?.textContent?.trim() ?? '';
		await expect(
			nameBeforeSort,
			'first sortable row name must be non-empty before sort'
		).toBeTruthy();

		await userEvent.click(inHeader);

		await waitFor(
			() => {
				const firstRow = document.body.querySelector('.mpp-list .mpp-row .mpp-name');
				const nameAfter = firstRow?.textContent?.trim() ?? '';
				if (nameAfter === nameBeforeSort) throw new Error('sort did not change row order');
				return firstRow;
			},
			{ timeout: 2000 }
		);
		const firstRowAfterSort = document.body.querySelector('.mpp-list .mpp-row .mpp-name');
		await expect(firstRowAfterSort?.textContent?.trim()).not.toBe(nameBeforeSort);
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="GuidedStep4CreateInteraction"
	tags={['interactive', 'data-driven']}
	loaders={[
		async () => {
			const postSpy = fn() as typeof fetch;
			globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
				const url = typeof input === 'string' ? input : input.toString();
				if (url.includes('/api/sessions') && url.includes('/overview')) {
					return new Response(
						JSON.stringify({
							session_id: '64b50581-7065-4013-8f77-dd7d8adeddb4',
							total_turns: 0,
							turns: []
						}),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				if (url.includes('/api/sessions') && !url.includes('/enrol')) {
					return new Response(
						JSON.stringify([
							{
								id: '64b50581-7065-4013-8f77-dd7d8adeddb4',
								project: '/Users/dev/projects/my-project',
								title: 'Implement steering backend',
								first_seen: '2026-06-14T09:00:00Z',
								last_seen: hoursAgo(3),
								request_count: 84,
								models: ['claude-sonnet-4-5'],
								last_status: 'Complete'
							}
						]),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				if (url.includes('/api/steerers') && init?.method === 'POST') {
					postSpy(input, init);
					return new Response(
						JSON.stringify({
							steerer: buildSteerer({
								id: 99,
								name: 'guided-steerer',
								display_name: 'Guided Steerer'
							}),
							warnings: []
						}),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				if (url.includes('/enrol')) {
					return new Response(JSON.stringify({ ok: true }), {
						status: 200,
						headers: { 'content-type': 'application/json' }
					});
				}
				return new Response('Not Found', { status: 404 });
			};
			(globalThis as Record<string, unknown>).__postSpy = postSpy;
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/new?mode=guided'),
					params: { id: 'new' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const sessionRow = await waitFor(
			() => {
				const rows = canvasElement.querySelectorAll('[role="option"]');
				if (rows.length === 0) throw new Error('session rows not yet rendered');
				return rows[0] as HTMLButtonElement;
			},
			{ timeout: 5000 }
		);
		await userEvent.click(sessionRow);
		const nextBtn1 = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /next/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Next button not found');
				return el;
			},
			{ timeout: 2000 }
		);
		await userEvent.click(nextBtn1);
		await waitFor(
			() => {
				const heading = Array.from(canvasElement.querySelectorAll('h3')).find((h) =>
					/what is session/i.test(h.textContent ?? '')
				);
				if (!heading) throw new Error('Step 2 not rendered');
				return heading;
			},
			{ timeout: 5000 }
		);
		const nextBtn2 = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /next.*steer/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Next: Define the steer button not found');
				return el;
			},
			{ timeout: 3000 }
		);
		await userEvent.click(nextBtn2);
		await waitFor(
			() => {
				const heading = Array.from(canvasElement.querySelectorAll('h3')).find((h) =>
					/what should the steerer watch for/i.test(h.textContent ?? '')
				);
				if (!heading) throw new Error('Step 3 not rendered');
				return heading;
			},
			{ timeout: 5000 }
		);
		const nextBtn3 = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /review.*create/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Review + create button not found');
				return el;
			},
			{ timeout: 3000 }
		);
		await userEvent.click(nextBtn3);
		const summaryCard = await waitFor(
			() => {
				const el = canvasElement.querySelector('.gf-summary-card') as HTMLElement | null;
				if (!el) throw new Error('summary card not yet rendered');
				return el;
			},
			{ timeout: 5000 }
		);
		await expect(summaryCard).toBeInTheDocument();
		const createBtn = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /create.*enrol/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Create + enrol session button not found');
				return el;
			},
			{ timeout: 3000 }
		);
		await expect(createBtn).toBeInTheDocument();
		await userEvent.click(createBtn);
		const postSpy = (globalThis as Record<string, unknown>).__postSpy as ReturnType<typeof fn>;
		await waitFor(
			() => {
				if (!postSpy.mock.calls.length) throw new Error('POST /api/steerers not yet called');
				return true;
			},
			{ timeout: 5000 }
		);
		await expect(postSpy).toHaveBeenCalled();
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="GuidedStep4CreateError"
	tags={['interactive', 'data-driven']}
	loaders={[
		async () => {
			globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
				const url = typeof input === 'string' ? input : input.toString();
				if (url.includes('/api/sessions') && url.includes('/overview')) {
					return new Response(
						JSON.stringify({
							session_id: '64b50581-7065-4013-8f77-dd7d8adeddb4',
							total_turns: 0,
							turns: []
						}),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				if (url.includes('/api/sessions') && !url.includes('/enrol')) {
					return new Response(
						JSON.stringify([
							{
								id: '64b50581-7065-4013-8f77-dd7d8adeddb4',
								project: '/Users/dev/projects/my-project',
								title: 'Implement steering backend',
								first_seen: '2026-06-14T09:00:00Z',
								last_seen: hoursAgo(3),
								request_count: 84,
								models: ['claude-sonnet-4-5'],
								last_status: 'Complete'
							}
						]),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				if (url.includes('/api/steerers') && init?.method === 'POST') {
					return new Response(JSON.stringify({ error: 'steerer store unavailable' }), {
						status: 503,
						statusText: 'Service Unavailable',
						headers: { 'content-type': 'application/json' }
					});
				}
				return new Response('Not Found', { status: 404 });
			};
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/new?mode=guided'),
					params: { id: 'new' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const sessionRow = await waitFor(
			() => {
				const rows = canvasElement.querySelectorAll('[role="option"]');
				if (rows.length === 0) throw new Error('session rows not yet rendered');
				return rows[0] as HTMLButtonElement;
			},
			{ timeout: 5000 }
		);
		await userEvent.click(sessionRow);
		const nextBtn1 = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /next/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Next button not found');
				return el;
			},
			{ timeout: 2000 }
		);
		await userEvent.click(nextBtn1);
		await waitFor(
			() => {
				const heading = Array.from(canvasElement.querySelectorAll('h3')).find((h) =>
					/what is session/i.test(h.textContent ?? '')
				);
				if (!heading) throw new Error('Step 2 not rendered');
				return heading;
			},
			{ timeout: 5000 }
		);
		const nextBtn2 = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /next.*steer/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Next: Define button not found');
				return el;
			},
			{ timeout: 3000 }
		);
		await userEvent.click(nextBtn2);
		await waitFor(
			() => {
				const heading = Array.from(canvasElement.querySelectorAll('h3')).find((h) =>
					/what should the steerer watch for/i.test(h.textContent ?? '')
				);
				if (!heading) throw new Error('Step 3 not rendered');
				return heading;
			},
			{ timeout: 5000 }
		);
		const nextBtn3 = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /review.*create/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Review + create button not found');
				return el;
			},
			{ timeout: 3000 }
		);
		await userEvent.click(nextBtn3);
		await waitFor(
			() => {
				const el = canvasElement.querySelector('.gf-summary-card') as HTMLElement | null;
				if (!el) throw new Error('summary card not yet rendered');
				return el;
			},
			{ timeout: 5000 }
		);
		const createBtn = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /create.*enrol/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Create + enrol button not found');
				return el;
			},
			{ timeout: 3000 }
		);
		await userEvent.click(createBtn);
		const errorBanner = await waitFor(
			() => {
				const el = canvasElement.querySelector(
					'.se-banner--error[role="alert"]'
				) as HTMLElement | null;
				if (!el) throw new Error('error banner not yet rendered');
				return el;
			},
			{ timeout: 5000 }
		);
		await expect(errorBanner.textContent?.trim()).toBe(
			'POST /api/steerers failed: 503 Service Unavailable'
		);
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="DeleteErrorInteraction"
	tags={['interactive', 'data-driven']}
	loaders={[
		async () => {
			globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
				const url = typeof input === 'string' ? input : input.toString();
				if (url.includes('/api/steerers/') && init?.method === 'DELETE') {
					return new Response(JSON.stringify({ error: 'steerer store unavailable' }), {
						status: 503,
						statusText: 'Service Unavailable',
						headers: { 'content-type': 'application/json' }
					});
				}
				if (url.includes('/api/steerers/')) {
					return new Response(JSON.stringify(buildSteerer()), {
						status: 200,
						headers: { 'content-type': 'application/json' }
					});
				}
				return new Response('Not Found', { status: 404 });
			};
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/1'),
					params: { id: '1' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		await waitFor(
			() => {
				const input = canvasElement.querySelector('#se-display-name') as HTMLInputElement | null;
				if (!input || !input.value) throw new Error('form not yet populated');
				return input;
			},
			{ timeout: 5000 }
		);
		const deleteBtn = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => b.textContent?.trim() === 'Delete') as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Delete button not found');
				return el;
			},
			{ timeout: 3000 }
		);
		await userEvent.click(deleteBtn);
		const confirmBtn = await waitFor(
			() => {
				const el = canvasElement.querySelector('.se-delete-confirm') as HTMLElement | null;
				if (!el) throw new Error('delete confirmation card not yet rendered');
				const btns = Array.from(el.querySelectorAll('button'));
				const confirm = btns.find((b) => /confirm delete/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!confirm) throw new Error('Confirm delete button not found');
				return confirm;
			},
			{ timeout: 3000 }
		);
		await userEvent.click(confirmBtn);
		const errorMsg = await waitFor(
			() => {
				const el = canvasElement.querySelector(
					'.se-delete-confirm__err[role="alert"]'
				) as HTMLElement | null;
				if (!el) throw new Error('delete error message not yet rendered');
				return el;
			},
			{ timeout: 5000 }
		);
		await expect(errorMsg.textContent?.trim()).toBe(
			'DELETE /api/steerers/1 failed: 503 Service Unavailable'
		);
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="VisualGuidedStep2"
	tags={['visual', 'data-driven', 'interactive']}
	loaders={[
		async () => {
			globalThis.fetch = async (input: RequestInfo | URL) => {
				const url = typeof input === 'string' ? input : input.toString();
				if (url.includes('/api/sessions') && url.includes('/overview')) {
					return new Response(
						JSON.stringify({
							session_id: '64b50581-7065-4013-8f77-dd7d8adeddb4',
							total_turns: 0,
							turns: []
						}),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				if (url.includes('/api/sessions')) {
					return new Response(
						JSON.stringify([
							{
								id: '64b50581-7065-4013-8f77-dd7d8adeddb4',
								project: '/Users/dev/projects/my-project',
								title: 'Implement steering backend',
								first_seen: '2026-06-14T09:00:00Z',
								last_seen: hoursAgo(3),
								request_count: 84,
								models: ['claude-sonnet-4-5'],
								last_status: 'Complete'
							}
						]),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				return new Response('Not Found', { status: 404 });
			};
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/new?mode=guided'),
					params: { id: 'new' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const sessionRow = await waitFor(
			() => {
				const rows = canvasElement.querySelectorAll('[role="option"]');
				if (rows.length === 0) throw new Error('session rows not yet rendered');
				return rows[0] as HTMLButtonElement;
			},
			{ timeout: 5000 }
		);
		await userEvent.click(sessionRow);
		const nextBtn = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /next/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Next button not found');
				return el;
			},
			{ timeout: 2000 }
		);
		await userEvent.click(nextBtn);
		await waitFor(
			() => {
				const heading = Array.from(canvasElement.querySelectorAll('h3')).find((h) =>
					/what is session/i.test(h.textContent ?? '')
				);
				if (!heading) throw new Error('Step 2 heading not yet rendered');
				return heading;
			},
			{ timeout: 5000 }
		);
		await captureStorySnapshot(canvasElement, 'page/GuidedStep2');
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="GuidedStep2WithTurns"
	tags={['data-driven', 'interactive']}
	loaders={[
		async () => {
			globalThis.fetch = async (input: RequestInfo | URL) => {
				const url = typeof input === 'string' ? input : input.toString();
				if (url.includes('/api/sessions') && url.includes('/overview')) {
					return new Response(
						JSON.stringify({
							session_id: '64b50581-7065-4013-8f77-dd7d8adeddb4',
							total_turns: 12,
							truncated: true,
							offset: 0,
							turns: [
								{
									turn_id: 12,
									user_prompt: 'Write the integration tests for the steerer endpoints',
									stop_response:
										'Tests written in tests/e2e/steerers.rs covering create, patch, delete, and enrol flows.',
									created_at: hoursAgo(4)
								},
								{
									turn_id: 11,
									user_prompt: 'Add the REST endpoints for create, patch, and delete steerers',
									stop_response:
										'Added POST /api/steerers, PATCH /api/steerers/{id}, and DELETE /api/steerers/{id} handlers.',
									created_at: hoursAgo(6)
								},
								{
									turn_id: 10,
									user_prompt: 'Wire the steerer registry into AppState so the fleet can read it',
									stop_response:
										'Registry is now an ArcSwap field on AppState, hot-reloaded on PATCH.',
									created_at: hoursAgo(7)
								},
								{
									turn_id: 9,
									user_prompt: 'Add the enrol/disenrol endpoints for per-session enrolment',
									stop_response:
										'Added POST /api/steerers/{id}/enrol and DELETE .../enrol/{session_id}.',
									created_at: hoursAgo(8)
								},
								{
									turn_id: 8,
									user_prompt:
										'Implement the steerer store module in crates/mike/src/store/steering.rs',
									stop_response:
										'Working on the store module. Created the SteererStore struct with ArcSwap registry.',
									created_at: hoursAgo(9)
								}
							]
						}),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				if (url.includes('/api/sessions')) {
					return new Response(
						JSON.stringify([
							{
								id: '64b50581-7065-4013-8f77-dd7d8adeddb4',
								project: '/Users/dev/projects/my-project',
								title: 'Implement steering backend',
								first_seen: '2026-06-14T09:00:00Z',
								last_seen: hoursAgo(3),
								request_count: 84,
								models: ['claude-sonnet-4-5'],
								last_status: 'Complete'
							}
						]),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				return new Response('Not Found', { status: 404 });
			};
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/new?mode=guided'),
					params: { id: 'new' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const sessionRow = await waitFor(
			() => {
				const rows = canvasElement.querySelectorAll('[role="option"]');
				if (rows.length === 0) throw new Error('session rows not yet rendered');
				return rows[0] as HTMLButtonElement;
			},
			{ timeout: 5000 }
		);
		await userEvent.click(sessionRow);
		const nextBtn = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /next/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Next button not found');
				return el;
			},
			{ timeout: 2000 }
		);
		await userEvent.click(nextBtn);
		await waitFor(
			() => {
				const heading = Array.from(canvasElement.querySelectorAll('h3')).find((h) =>
					/what is session/i.test(h.textContent ?? '')
				);
				if (!heading) throw new Error('Step 2 heading not yet rendered');
				return heading;
			},
			{ timeout: 5000 }
		);
		const turnRows = await waitFor(
			() => {
				const turns = canvasElement.querySelectorAll('.gf-turn');
				if (turns.length === 0) throw new Error('turn rows not yet rendered');
				return Array.from(turns) as HTMLElement[];
			},
			{ timeout: 5000 }
		);
		await expect(turnRows).toHaveLength(5);

		for (const row of turnRows) {
			const prompt = row.querySelector('.gf-turn__prompt')?.textContent ?? '';
			await expect(prompt.replace(/^\s*User:\s*/, '').trim()).not.toBe('');
			const response = row.querySelector('.gf-turn__response')?.textContent ?? '';
			await expect(response.replace(/^\s*Claude:\s*/, '').trim()).not.toBe('');
		}

		const [oldest] = turnRows;
		const newest = turnRows[turnRows.length - 1];
		if (!oldest || !newest) throw new Error('expected 5 turn rows');
		await expect(oldest.textContent).toContain('Implement the steerer store module');
		await expect(oldest.querySelector('.gf-turn__meta')?.textContent).toContain('Turn 8');
		await expect(newest.textContent).toContain('Write the integration tests');
		await expect(newest.querySelector('.gf-turn__meta')?.textContent).toContain('Turn 12');

		const count = canvasElement.querySelector('.gf-overview-count')?.textContent ?? '';
		await expect(count).toContain('last 5 of 12');
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="SaveErrorInteraction"
	tags={['interactive', 'data-driven']}
	loaders={[
		async () => {
			stubGetThenPatchError(buildSteerer());
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/steering/1'),
					params: { id: '1' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		await waitFor(
			() => {
				const input = canvasElement.querySelector('#se-display-name') as HTMLInputElement | null;
				if (!input || !input.value) throw new Error('form not yet populated');
				return input;
			},
			{ timeout: 5000 }
		);
		const saveBtn = await waitFor(
			() => {
				const btns = Array.from(canvasElement.querySelectorAll('button'));
				const el = btns.find((b) => /save changes/i.test(b.textContent ?? '')) as
					HTMLButtonElement | undefined;
				if (!el) throw new Error('Save button not found');
				return el;
			},
			{ timeout: 3000 }
		);
		await userEvent.click(saveBtn);
		const errorBanner = await waitFor(
			() => {
				const el = canvasElement.querySelector(
					'.se-banner--error[role="alert"]'
				) as HTMLElement | null;
				if (!el) throw new Error('save error banner not yet rendered');
				return el;
			},
			{ timeout: 5000 }
		);
		await expect(errorBanner.textContent?.trim()).toBe(
			'PATCH /api/steerers/1 failed: 503 Service Unavailable'
		);
	}}
>
	{#snippet template()}
		<div style="height:100vh;overflow:auto;background:var(--surface-bg)">
			<Page />
		</div>
	{/snippet}
</Story>
