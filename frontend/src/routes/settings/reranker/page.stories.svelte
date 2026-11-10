<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import RerankerPage from './+page.svelte';
	import type { SettingMeta } from '$lib/api/settings';
	import type { RerankerPageData } from './+page';

	const RERANKER = 'Reranker';
	const NIGHT = 'Overnight reranking';
	const RANKING = 'Search ranking';

	function meta(
		o: Partial<SettingMeta> & Pick<SettingMeta, 'key' | 'kind' | 'value' | 'default' | 'group'>
	): SettingMeta {
		return {
			scope: 'global',
			applies: 'next_message',
			label: 'Label',
			description: 'Description.',
			...o
		} as SettingMeta;
	}

	function settings(tuning: boolean): SettingMeta[] {
		return [
			meta({
				key: 'rerank.enabled',
				group: RERANKER,
				kind: 'bool',
				default: false,
				value: true,
				label: 'Enable reranker',
				description: 'Reorder search results with a reranker model.'
			}),
			meta({
				key: 'rerank.url',
				group: RERANKER,
				kind: 'string',
				default: 'http://localhost:8081',
				value: 'http://localhost:8081',
				label: 'Reranker URL',
				description: 'Base URL of the llama.cpp llama-server running with --reranking.',
				max_len: 512
			}),
			meta({
				key: 'rerank.model',
				group: RERANKER,
				kind: 'string',
				default: 'bge-reranker-v2-m3',
				value: 'bge-reranker-v2-m3',
				label: 'Reranker model',
				description: 'Model name sent with each rerank request.',
				max_len: 256
			}),
			meta({
				key: 'rerank.timeout_ms',
				group: RERANKER,
				kind: 'int',
				default: 800,
				value: 800,
				label: 'Reranker timeout (ms)',
				description: 'Hard limit for one rerank request.',
				min_i64: 50,
				max_i64: 30000
			}),
			meta({
				key: 'rerank.candidates',
				group: RERANKER,
				kind: 'int',
				default: 50,
				value: 50,
				label: 'Reranker candidates',
				description: 'How many top search results are sent to the reranker.',
				min_i64: 1,
				max_i64: 200
			}),
			meta({
				key: 'rerank.rescue_score',
				group: RERANKER,
				kind: 'float',
				default: 0,
				value: 0,
				label: 'Rescue score',
				description:
					'A weak match stays in a reranked search when the reranker scores it this high.',
				min_f64: -10,
				max_f64: 10
			}),
			meta({
				key: 'rerank.tuning.enabled',
				group: NIGHT,
				kind: 'bool',
				default: false,
				value: tuning,
				applies: 'next_cycle',
				label: 'Overnight reranker tuning',
				description: "Let the overnight worker grade the day's searches with the reranker."
			}),
			meta({
				key: 'rerank.nightly.start_hour_utc',
				group: NIGHT,
				kind: 'int',
				default: 1,
				value: 1,
				applies: 'next_cycle',
				label: 'Overnight window start (UTC hour)',
				description: 'Hour of the day, in UTC, from which the overnight pass may run.',
				min_i64: 0,
				max_i64: 23
			}),
			meta({
				key: 'rerank.tuning.min_gain',
				group: NIGHT,
				kind: 'float',
				default: 0.02,
				value: 0.02,
				applies: 'next_cycle',
				label: 'Smallest gain kept',
				description:
					'A ranking change is kept only when the share of used results rises this much.',
				min_f64: 0,
				max_f64: 1
			}),
			meta({
				key: 'search.ranking.lexical_weight',
				group: RANKING,
				kind: 'float',
				default: 1,
				value: 1,
				label: 'Keyword match weight',
				description: 'How much a keyword match counts when search results are ordered.',
				min_f64: 0,
				max_f64: 5
			}),
			meta({
				key: 'search.ranking.semantic_floor',
				group: RANKING,
				kind: 'float',
				default: null as unknown as number,
				value: null,
				label: 'Meaning match floor',
				description: "Empty uses the embedding model's own floor; a number overrides it.",
				min_f64: 0,
				max_f64: 1
			}),
			meta({
				key: 'search.ranking.kind_weights',
				group: RANKING,
				kind: 'json',
				default: {},
				value: { seat: 0.5 },
				label: 'Weight per result kind',
				description: 'A JSON object of result kind to a weight from 0 to 5.',
				min_f64: 0,
				max_f64: 5
			})
		];
	}

	function buildData(tuning = true): RerankerPageData {
		return { settings: settings(tuning), settingsLoadError: null };
	}

	type Pass = 'ok' | 'conflict';
	let pass: Pass = 'ok';
	let passCalls = 0;
	let release: (() => void) | null = null;

	function fetchStub(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
		const url =
			typeof input === 'string'
				? input
				: input instanceof URL
					? input.href
					: (input as Request).url;
		const json = (body: unknown, status = 200) =>
			Promise.resolve(
				new Response(JSON.stringify(body), {
					status,
					headers: { 'content-type': 'application/json' }
				})
			);
		if (url.includes('/api/knowledge/rerank-pass')) {
			passCalls += 1;
			if (pass === 'conflict') return json({ error: 'rerank.tuning.enabled is off' }, 409);
			return new Promise((resolve) => {
				release = () =>
					resolve(
						new Response(
							JSON.stringify({
								graded: 12,
								tuned: 'Recorded { kept: 1, tried: 3 }',
								thread_candidates: 40,
								lesson_pairs: 7,
								stopped: null
							}),
							{ status: 200, headers: { 'content-type': 'application/json' } }
						)
					);
			});
		}
		if (url.includes('/api/settings/') && init?.method === 'PATCH') {
			const key = decodeURIComponent(url.split('/api/settings/').pop() ?? 'x');
			const found = settings(true).find((s) => s.key === key);
			const value: unknown = (JSON.parse(String(init.body)) as { value?: unknown }).value;
			return json({ ...found, value });
		}
		if (url.includes('/api/settings')) return json({ settings: settings(true) });
		return json({ error: 'unexpected fetch' }, 503);
	}

	const { Story } = defineMeta({
		title: 'Routes/Settings/Reranker',
		component: RerankerPage,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' },
		argTypes: { data: { control: false } }
	});
</script>

{#snippet template(args: { data: RerankerPageData })}
	<div style="width:100%; min-height:100vh; background:var(--surface-bg);">
		<RerankerPage {...args} />
	</div>
{/snippet}

<Story
	name="Loaded"
	loaders={[
		async () => {
			globalThis.fetch = fetchStub as typeof globalThis.fetch;
			return {};
		}
	]}
	args={{ data: buildData() }}
	{template}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByRole('heading', { name: RERANKER, level: 1 })).toBeInTheDocument();
		for (const group of [RERANKER, NIGHT, RANKING])
			await expect(canvas.getByRole('heading', { name: group, level: 2 })).toBeInTheDocument();
		await expect(canvas.getByLabelText('Reranker timeout (ms)')).toHaveValue(800);
		await expect(canvas.getByLabelText('Meaning match floor')).toHaveValue(null);
		await expect(canvas.getByLabelText('Weight per result kind')).toHaveValue('{"seat":0.5}');
		await expect(canvas.getByRole('link', { name: 'Reranker' })).toHaveAttribute(
			'aria-current',
			'page'
		);
		await expect(canvas.getByRole('button', { name: 'Run overnight pass now' })).toBeEnabled();
	}}
/>

<Story
	name="TuningOff"
	loaders={[
		async () => {
			globalThis.fetch = fetchStub as typeof globalThis.fetch;
			return {};
		}
	]}
	args={{ data: buildData(false) }}
	{template}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByRole('button', { name: 'Run overnight pass now' })).toBeDisabled();
		await expect(canvasElement.textContent).toContain(
			'Turn on “Overnight reranker tuning” to run a pass.'
		);
	}}
/>

<Story
	name="PassResult"
	loaders={[
		async () => {
			pass = 'ok';
			passCalls = 0;
			release = null;
			globalThis.fetch = fetchStub as typeof globalThis.fetch;
			return {};
		}
	]}
	args={{ data: buildData() }}
	{template}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await userEvent.click(canvas.getByRole('button', { name: 'Run overnight pass now' }));
		const busy = await canvas.findByRole('button', { name: 'Running overnight pass' });
		await expect(busy).toBeDisabled();
		await expect(passCalls).toBe(1);
		// The rest of the page stays usable while the pass runs.
		await expect(canvas.getByLabelText('Reranker timeout (ms)')).toBeEnabled();
		release?.();
		await waitFor(() =>
			expect(canvasElement.textContent).toContain(
				'Graded 12 searches; tuning: kept 1 of 3 tried changes; 40 thread candidates; 7 lesson pairs.'
			)
		);
		await expect(canvas.getByRole('button', { name: 'Run overnight pass now' })).toBeEnabled();
	}}
/>

<Story
	name="PassError"
	loaders={[
		async () => {
			pass = 'conflict';
			passCalls = 0;
			globalThis.fetch = fetchStub as typeof globalThis.fetch;
			return {};
		}
	]}
	args={{ data: buildData() }}
	{template}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await userEvent.click(canvas.getByRole('button', { name: 'Run overnight pass now' }));
		await waitFor(() =>
			expect(canvasElement.textContent).toContain('rerank.tuning.enabled is off')
		);
		await expect(canvas.getByRole('button', { name: 'Run overnight pass now' })).toBeEnabled();
	}}
/>
