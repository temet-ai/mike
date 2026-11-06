<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, userEvent, waitFor } from 'storybook/test';
	import WorkerCard from './WorkerCard.svelte';
	import CollapseStoreProvider from '$lib/components/__harness__/CollapseStoreProvider.svelte';

	const MODELS = [
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
			help: 'The longest run of text embedded as a single vector — longer text is split at this boundary first. 2048 characters is roughly 500 tokens: about 350 words, or one to two paragraphs / a medium-sized function. Big enough to keep a coherent passage together, small enough that a match points at a precise spot rather than a whole document.'
		},
		{
			label: 'Endpoint',
			value: 'http://localhost:11434/v1',
			help: 'The OpenAI-compatible base URL embedding requests are sent to. Point it at your local Ollama or Infinity server, or a hosted embedding API. It must match the provider of the selected model above.'
		}
	];

	const { Story } = defineMeta({
		title: 'Routes/Settings/detail/WorkerCard',
		component: WorkerCard,
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	let enabled = $state(true);
	let selectedModelId = $state('nomic-embed-text-v1.5');

	const cardError = 'Could not save: request timed out.';
</script>

<Story
	name="Worker"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const card = canvasElement.querySelector('.dsc-card') as HTMLElement;
		const sw = canvas.getByRole('switch');

		const note = canvas.getByLabelText('Applies on the next restart.');
		await expect(note).toHaveTextContent('On restart');
		const errorEl = canvasElement.querySelector('.wc__error');
		await expect(errorEl?.textContent?.trim()).toBe(cardError);
		await expect(errorEl).toHaveAttribute('role', 'alert');

		await expect(canvasElement.querySelector('.dsc-body')).not.toBeNull();
		await expect(canvas.getByRole('radiogroup', { name: 'Embedding model' })).toBeInTheDocument();
		await expect(canvas.getAllByRole('radio').length).toBe(MODELS.length);
		const bgOn = getComputedStyle(card).backgroundColor;

		await userEvent.click(sw);
		await waitFor(() => expect(canvasElement.querySelector('.dsc-body')).toBeNull());
		await expect(
			getComputedStyle(card).backgroundColor,
			'off must not paint a red/error background'
		).toBe(bgOn);

		await userEvent.click(sw);
		await waitFor(() => expect(canvasElement.querySelector('.dsc-body')).not.toBeNull());
		await waitFor(
			() => {
				const body = canvasElement.querySelector('.dsc-body') as HTMLElement | null;
				expect(body ? getComputedStyle(body).opacity : null).toBe('1');
			},
			{ timeout: 500 }
		);

		const unselected = canvas
			.getAllByRole('radio')
			.find((r): r is HTMLInputElement => !(r as HTMLInputElement).checked);
		await expect(unselected).not.toBeUndefined();
		await userEvent.click(unselected!);
		await expect(unselected!.checked).toBe(true);
		await new Promise((r) => setTimeout(r, 150));

		const modelRows = canvasElement.querySelectorAll('[data-slot="radio-group-item"]');
		await expect(modelRows.length).toBe(MODELS.length);
		modelRows.forEach((row, i) => expect(row.textContent ?? '').toContain(MODELS[i]!.description));

		const cfgHelp = Array.from(canvasElement.querySelectorAll('.wc-cfg-help'));
		await expect(cfgHelp.length).toBe(CONFIG.length);
		await expect(cfgHelp[0]).toHaveAttribute('aria-label', `About ${CONFIG[0]!.label}`);
		await userEvent.hover(cfgHelp[0]!);
		const cfgTip = await waitFor(() => {
			const el = document.body.querySelector('[data-slot="tooltip-text"]') as HTMLElement | null;
			if (!el) throw new Error('config help tooltip not yet rendered');
			return el;
		});
		await expect(cfgTip.textContent?.trim()).toBe(CONFIG[0]!.help);
		await userEvent.unhover(cfgHelp[0]!);
	}}
>
	{#snippet template()}
		<div style="max-width:920px; padding:24px; background:var(--surface-bg);">
			<WorkerCard
				name="embedding"
				label="Embedding"
				description="Generates semantic embeddings of session turns for memory recall and similarity search."
				{enabled}
				onToggle={(v) => (enabled = v)}
				models={MODELS}
				{selectedModelId}
				onSelectModel={(id) => (selectedModelId = id)}
				config={CONFIG}
				error={cardError}
				applies="on_restart"
			/>
		</div>
	{/snippet}
</Story>
