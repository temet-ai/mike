<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect } from 'storybook/test';
	import WorkerCard from './WorkerCard.svelte';
	import type { WorkerModel } from './WorkerCard.svelte';
	import DestinationNav from '$lib/components/shell/DestinationNav.svelte';

	const MODELS: WorkerModel[] = [
		{
			id: 'nomic-embed-text-v1.5',
			released: 'Feb 2024',
			pricing: 'free · local',
			context: '768 dim · 8K ctx',
			description:
				"Nomic's open embedding model, run locally via Ollama/Infinity. 768-dimension vectors, 8K-token input. The proxy's default — no API cost and solid general-purpose retrieval quality."
		},
		{
			id: 'openai/text-embedding-3-small',
			released: 'Jan 2024',
			pricing: '$0.02 / 1M',
			context: '1536 dim · 8K ctx',
			description:
				"OpenAI's small embedding model. Cheap and fast at 1536 dimensions; a strong hosted default when you want better quality than local models without paying for the large tier."
		},
		{
			id: 'openai/text-embedding-3-large',
			released: 'Jan 2024',
			pricing: '$0.13 / 1M',
			context: '3072 dim · 8K ctx',
			description:
				"OpenAI's large embedding model. 3072-dimension vectors give the best quality of the OpenAI tier at ~6× the price of 3-small — worth it when recall accuracy matters more than cost."
		},
		{
			id: 'voyage-3',
			released: 'Sep 2024',
			pricing: '$0.06 / 1M',
			context: '1024 dim · 32K ctx',
			description:
				"Voyage AI's general embedding model. 1024 dimensions with a large 32K-token context; strong on long-document and code retrieval, a mid-priced hosted alternative to OpenAI."
		},
		{
			id: 'BAAI/bge-m3',
			released: 'Jan 2024',
			pricing: 'free · local',
			context: '1024 dim · 8K ctx',
			description:
				"BAAI's open multilingual embedding model. Runs locally, 1024 dimensions, strong cross-lingual retrieval — a good open option when your sessions span multiple languages."
		}
	];

	const CONFIG = [
		{
			label: 'Chunk max chars',
			value: '2048',
			help: 'The longest run of text embedded as a single vector — longer text is split at this boundary first. 2048 characters is roughly 500 tokens: about 350 words, or one to two paragraphs / a medium-sized function.'
		},
		{
			label: 'Endpoint',
			value: 'http://localhost:11434/v1',
			help: 'The OpenAI-compatible base URL embedding requests are sent to. Point it at your local Ollama or Infinity server, or a hosted embedding API. It must match the provider of the selected model above.'
		}
	];

	const { Story } = defineMeta({
		title: 'Routes/Settings/Embedding',
		component: WorkerCard,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' }
	});
</script>

<script lang="ts">
	let enabled = $state(true);
	let model = $state('nomic-embed-text-v1.5');
</script>

<Story
	name="Page"
	parameters={{
		sveltekit_experimental: {
			state: { page: { url: new URL('http://localhost/settings/embedding'), params: {} } }
		}
	}}
	play={async ({ canvasElement }) => {
		await expect(canvasElement.querySelector('a[href="/settings/embedding"]')).not.toBeNull();
		await expect(canvasElement.querySelectorAll('.dsc-card').length).toBe(1);
		await expect(within(canvasElement).getAllByRole('radio').length).toBe(MODELS.length);
	}}
>
	{#snippet template()}
		<div class="shell">
			<div class="nav"><DestinationNav /></div>
			<div class="content">
				<header class="hd">
					<h1 class="title">Embedding</h1>
					<p class="sub">
						Generates semantic embeddings of session turns for memory recall and similarity search.
						Choose the model and tune chunking below.
					</p>
				</header>
				<WorkerCard
					name="embedding"
					label="Embedding"
					description="Generates semantic embeddings of session turns so past decisions and debugging history can be recalled into future sessions."
					{enabled}
					onToggle={(v) => (enabled = v)}
					models={MODELS}
					selectedModelId={model}
					onSelectModel={(id) => (model = id)}
					config={CONFIG}
				/>
			</div>
		</div>
	{/snippet}
</Story>

<style>
	.shell {
		display: flex;
		height: 1100px;
		background: var(--surface-bg);
	}
	.nav {
		width: 240px;
		flex-shrink: 0;
		position: relative;
		display: flex;
		border-right: 1px solid var(--border-subtle);
	}
	.content {
		flex: 1 1 auto;
		min-width: 0;
		overflow-y: auto;
		padding: 28px 44px 64px;
		display: flex;
		flex-direction: column;
		gap: 18px;
	}
	.hd {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.title {
		font-family: var(--font-display);
		font-size: 20px;
		font-weight: 700;
		color: var(--text-strong);
		margin: 0;
		letter-spacing: -0.01em;
	}
	.sub {
		margin: 0;
		font-size: 13px;
		color: var(--text-2);
		line-height: 1.6;
		max-width: 760px;
	}
</style>
