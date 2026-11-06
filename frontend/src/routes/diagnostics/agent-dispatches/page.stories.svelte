<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, waitFor, expect } from 'storybook/test';
	import { stubFetchError } from '$lib/test-support/storyFetchStubs';
	import { __setRetryDelaysForTesting, __resetRetryDelaysForTesting } from '$lib/api';
	import Page from './+page.svelte';
	import type { AgentDispatchDto } from '$lib/types';

	function buildDispatch(
		overrides: Partial<AgentDispatchDto> & { parent_request_id: number }
	): AgentDispatchDto {
		return {
			session_id: '6ed89307-ecac-4248-99ba-babd6b75c185',
			project_id: 28379,
			created_at: '2026-05-29T12:00:00Z',
			tool_id: `toolu_${overrides.parent_request_id}`,
			subagent_type: 'frontend-developer',
			description: 'Build operator-turns view family',
			brief_char_len: 1941,
			brief_sha256: 'afe399394e6432d2964193d2c7f0fa66daea90c4e48d69ecfe769f2112b91355',
			ord: 0,
			...overrides
		};
	}

	const FIVE: AgentDispatchDto[] = [
		buildDispatch({ parent_request_id: 72107, subagent_type: 'frontend-developer' }),
		buildDispatch({
			parent_request_id: 72108,
			subagent_type: 'rust-developer',
			description: 'Add the tool-results error feed endpoint and MCP mirror.',
			brief_char_len: 880
		}),
		buildDispatch({
			parent_request_id: 72109,
			subagent_type: 'db-dev',
			description: 'Add the normalised agent_dispatches table + migration.',
			brief_char_len: 1204
		}),
		buildDispatch({
			parent_request_id: 72110,
			subagent_type: null,
			description: null,
			brief_char_len: 0
		}),
		buildDispatch({
			parent_request_id: 72111,
			subagent_type: 'ui-designer',
			description: 'Design the Badge component states in Storybook.',
			brief_char_len: 640,
			ord: 1
		})
	];

	const LONG_DESC = 'Design and implement the cross-session diagnostics surface. '.repeat(9);
	const LONG: AgentDispatchDto[] = [
		buildDispatch({
			parent_request_id: 90000,
			subagent_type: 'frontend-developer',
			description: LONG_DESC,
			brief_char_len: LONG_DESC.length
		})
	];

	function stub(rows: AgentDispatchDto[]): void {
		globalThis.fetch = async (input: RequestInfo | URL) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (url.includes('/api/agent-dispatches')) {
				return new Response(JSON.stringify(rows), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404 });
		};
	}

	const PAGE_PARAM = {
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/diagnostics/agent-dispatches'),
					params: {}
				}
			}
		}
	};

	const { Story } = defineMeta({
		title: 'Routes/Diagnostics/AgentDispatches',
		component: Page,
		tags: ['autodocs']
	});
</script>

<Story
	name="Loading"
	loaders={[
		async () => {
			globalThis.fetch = () => new Promise(() => {});
			return {};
		}
	]}
	parameters={PAGE_PARAM}
>
	{#snippet template()}
		<Page />
	{/snippet}
</Story>

<Story
	name="Empty"
	loaders={[
		async () => {
			stub([]);
			return {};
		}
	]}
	parameters={PAGE_PARAM}
>
	{#snippet template()}
		<Page />
	{/snippet}
</Story>

<Story
	name="Error"
	loaders={[
		async () => {
			stubFetchError('GET /api/agent-dispatches failed: 503 Service Unavailable');
			return {};
		}
	]}
	parameters={PAGE_PARAM}
	play={async ({ canvasElement }) => {
		__setRetryDelaysForTesting([]);
		try {
			await waitFor(() => {
				const banner = canvasElement.querySelector('[data-slot="load-error-banner"]');
				if (!banner) throw new Error('error banner not yet rendered');
				return banner;
			});
			const retry = await waitFor(() => {
				const el = canvasElement.querySelector('[data-slot="load-error-retry"]');
				if (!el) throw new Error('retry button not yet rendered');
				return el as HTMLButtonElement;
			});
			stub(FIVE);
			await userEvent.click(retry);
			await waitFor(() => {
				const rows = canvasElement.querySelectorAll('.ad-row');
				if (rows.length === 0) throw new Error('rows not yet rendered after retry');
			});
			await expect(
				canvasElement.querySelector('[data-slot="load-error-banner"]')
			).not.toBeInTheDocument();
		} finally {
			__resetRetryDelaysForTesting();
		}
	}}
>
	{#snippet template()}
		<Page />
	{/snippet}
</Story>

<Story
	name="FilteredNoMatch"
	loaders={[
		async () => {
			stub([]);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/diagnostics/agent-dispatches?project=nonexistent-project'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		await waitFor(() => {
			const text = canvasElement.textContent ?? '';
			if (!text.includes('No dispatches match')) {
				throw new Error('filtered-empty message not yet rendered');
			}
		});
		const clearButton = Array.from(canvasElement.querySelectorAll('button')).find(
			(b) => b.textContent?.trim() === 'Clear filters'
		);
		await expect(clearButton).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<Page />
	{/snippet}
</Story>

<Story
	name="WithDispatches"
	loaders={[
		async () => {
			stub(FIVE);
			return {};
		}
	]}
	parameters={PAGE_PARAM}
>
	{#snippet template()}
		<Page />
	{/snippet}
</Story>

<Story
	name="DescriptionExpanded"
	loaders={[
		async () => {
			stub(FIVE);
			return {};
		}
	]}
	parameters={PAGE_PARAM}
	play={async ({ canvasElement }) => {
		const toggle = await waitFor(() => {
			const el = canvasElement.querySelector(
				'button[aria-label="Expand dispatch 72107:toolu_72107:0"]'
			);
			if (!el) throw new Error('row toggle not yet rendered');
			return el as HTMLButtonElement;
		});
		await userEvent.click(toggle);
		await expect(canvasElement.querySelector('pre')).toBeInTheDocument();
		await expect(
			canvasElement.querySelector(
				'a[aria-label="View the full agent brief in the parent request body"]'
			)
		).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<Page />
	{/snippet}
</Story>

<Story
	name="NoTruncationTripwire"
	loaders={[
		async () => {
			stub(LONG);
			return {};
		}
	]}
	parameters={PAGE_PARAM}
	play={async ({ canvasElement }) => {
		const toggle = await waitFor(() => {
			const el = canvasElement.querySelector(
				'button[aria-label="Expand dispatch 90000:toolu_90000:0"]'
			);
			if (!el) throw new Error('row toggle not yet rendered');
			return el as HTMLButtonElement;
		});
		await userEvent.click(toggle);
		const pre = await waitFor(() => {
			const el = canvasElement.querySelector('pre');
			if (!el) throw new Error('no <pre> rendered yet');
			return el as HTMLElement;
		});
		await expect((pre.textContent ?? '').length).toBeGreaterThan(450);
	}}
>
	{#snippet template()}
		<Page />
	{/snippet}
</Story>
