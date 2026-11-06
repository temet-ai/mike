<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, userEvent } from 'storybook/test';
	import ModelTable from './ModelTable.svelte';
	import type { ModelEntry } from '$lib/types';

	const MODELS: ModelEntry[] = [
		{
			id: 'google/gemini-3.8-flash',
			name: 'Claude Haiku 4.5',
			prompt_price_per_1m: 1,
			completion_price_per_1m: 5,
			context_length: 200000,
			known_good: true,
			supports_structured_output: true
		},
		{
			id: 'google/gemini-3.5-flash-lite',
			name: 'Gemini 2.5 Flash Lite',
			prompt_price_per_1m: 0.1,
			completion_price_per_1m: 0.4,
			context_length: 1000000,
			known_good: true,
			supports_structured_output: true
		},
		{
			id: 'openai/gpt-5.6-luna',
			name: 'GPT 5.6 Luna',
			prompt_price_per_1m: 0.15,
			completion_price_per_1m: 0.6,
			context_length: 128000,
			known_good: false,
			supports_structured_output: true,
			description:
				"OpenAI's GPT 5.6 Luna is a fast, low-cost multimodal model. It accepts text and image input and supports tool calling and reasoning, making it a capable general-purpose choice for steering workloads.",
			input_modalities: ['text', 'image'],
			supports_tools: true,
			supports_reasoning: true
		},
		{
			id: 'moonshotai/kimi-k2.7-code',
			name: 'Kimi K2.7 Code',
			prompt_price_per_1m: 0.3,
			completion_price_per_1m: 1.2,
			context_length: 256000,
			known_good: false,
			supports_structured_output: true
		},
		{
			id: 'meta-llama/llama-3-8b-instruct',
			name: 'Llama 3 8B Instruct',
			prompt_price_per_1m: 0.05,
			completion_price_per_1m: 0.1,
			context_length: 8192,
			known_good: false,
			supports_structured_output: false,
			description:
				'A small open-weight instruction-tuned model. Fast and cheap, but this OpenRouter listing does not advertise structured-output support, so it cannot run the steerer verdict schema.'
		}
	];

	const { Story } = defineMeta({
		title: 'Routes/Settings/detail/ModelTable',
		component: ModelTable,
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	let current = $state('openai/gpt-5.6-luna');
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const rowFor = (id: string): HTMLTableRowElement => {
			const cells = Array.from(canvasElement.querySelectorAll('tbody .mt-model-id'));
			const cell = cells.find((el) => el.textContent?.trim() === id);
			if (!cell) throw new Error(`no row for ${id}`);
			return cell.closest('tr') as HTMLTableRowElement;
		};

		const banner = canvasElement.querySelector('.mt-now') as HTMLElement;
		await expect(banner).not.toBeNull();
		await expect(banner.textContent).toContain('Model in use');
		await expect(banner.querySelector('.mt-now__id')?.textContent).toBe('openai/gpt-5.6-luna');
		await expect(banner.textContent).toContain('128K context');
		await expect(banner.textContent).toContain('$0.15 in');
		await expect(banner.textContent).toContain('$0.60 out');

		const firstGroupLabel = canvasElement.querySelector('.mt-group-label');
		await expect(firstGroupLabel?.textContent?.trim()).toBe('In use');
		const pinned = canvasElement.querySelector('.mt-row--pinned');
		await expect(pinned?.querySelector('.mt-model-id')?.textContent).toBe('openai/gpt-5.6-luna');
		await expect(pinned?.textContent).toContain('In use');

		const gptCells = Array.from(canvasElement.querySelectorAll('tbody .mt-model-id')).filter(
			(el) => el.textContent?.trim() === 'openai/gpt-5.6-luna'
		);
		await expect(gptCells).toHaveLength(1);

		await expect(canvasElement.textContent).toContain('google/gemini-3.8-flash');
		await expect(canvasElement.textContent).toContain('google/gemini-3.5-flash-lite');
		await expect(canvasElement.querySelectorAll('.mt-badge--recommended')).toHaveLength(2);

		await expect(canvasElement.textContent).toContain('Input $/1M');
		await expect(canvasElement.textContent).toContain('Output $/1M');

		await expect(canvasElement.textContent).toContain('moonshotai/kimi-k2.7-code');

		await expect(canvasElement.textContent).toContain('meta-llama/llama-3-8b-instruct');
		await expect(canvasElement.textContent).toContain('No structured output');
		const incompatibleRow = rowFor('meta-llama/llama-3-8b-instruct');
		await expect(incompatibleRow.getAttribute('aria-disabled')).toBe('true');
		await userEvent.click(incompatibleRow);
		await expect(current).toBe('openai/gpt-5.6-luna');

		const gptRow = rowFor('openai/gpt-5.6-luna');
		await expect(gptRow.getAttribute('aria-current')).toBe('true');

		const gptTags = Array.from(gptRow!.querySelectorAll('.mt-cap-tag')).map((el) => el.textContent);
		await expect(gptTags).toEqual(['tools', 'reasoning', 'vision']);

		const kimiRowBeforeSelect = rowFor('moonshotai/kimi-k2.7-code');
		await expect(kimiRowBeforeSelect.querySelectorAll('.mt-cap-tag')).toHaveLength(0);
		await expect(kimiRowBeforeSelect.querySelector('.mt-desc-toggle')).toBeNull();

		const gptDescToggle = within(gptRow).getByRole('button', { name: 'Show description' });
		await expect(gptDescToggle.getAttribute('aria-expanded')).toBe('false');
		await expect(canvasElement.textContent).not.toContain('fast, low-cost multimodal model');
		await userEvent.click(gptDescToggle);
		await expect(gptDescToggle.getAttribute('aria-expanded')).toBe('true');
		await expect(canvasElement.textContent).toContain('fast, low-cost multimodal model');
		await expect(current).toBe('openai/gpt-5.6-luna');
		await userEvent.click(gptDescToggle);
		await expect(canvasElement.textContent).not.toContain('fast, low-cost multimodal model');

		const llamaDescToggle = within(incompatibleRow).getByRole('button', {
			name: 'Show description'
		});
		await userEvent.click(llamaDescToggle);
		await expect(canvasElement.textContent).toContain('cannot run the steerer verdict schema');
		await expect(current).toBe('openai/gpt-5.6-luna');

		const kimiRow = rowFor('moonshotai/kimi-k2.7-code');
		await userEvent.click(kimiRow);
		await expect(current).toBe('moonshotai/kimi-k2.7-code');

		await expect(banner.querySelector('.mt-now__id')?.textContent).toBe(
			'moonshotai/kimi-k2.7-code'
		);
		await expect(banner.textContent).toContain('256K context');
		await expect(canvasElement.querySelector('.mt-row--pinned .mt-model-id')?.textContent).toBe(
			'moonshotai/kimi-k2.7-code'
		);
		await expect(rowFor('openai/gpt-5.6-luna').classList.contains('mt-row--pinned')).toBe(false);

		const searchInput = canvas.getByPlaceholderText('Filter models by id or name…');
		await userEvent.type(searchInput, 'claude');
		await expect(canvasElement.textContent).toContain('google/gemini-3.8-flash');
		await expect(canvasElement.textContent).not.toContain('gemini-3.5-flash-lite');
		await expect(canvasElement.querySelector('.mt-row--pinned .mt-model-id')?.textContent).toBe(
			'moonshotai/kimi-k2.7-code'
		);
		await userEvent.clear(searchInput);
		await expect(canvasElement.textContent).toContain('google/gemini-3.5-flash-lite');
	}}
>
	{#snippet template()}
		<div style="max-width:680px;">
			<ModelTable models={MODELS} currentValue={current} onSelect={(id) => (current = id)} />
		</div>
	{/snippet}
</Story>
