<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect } from 'storybook/test';
	import SessionMetrics from './SessionMetrics.svelte';
	import 'augmented-ui/augmented-ui.min.css';

	const { Story } = defineMeta({
		title: 'Components/Shell/SessionMetrics',
		component: SessionMetrics,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' }
	});

	const MODELS = [
		{ model: 'claude-sonnet-5', count: 512 },
		{ model: 'claude-fable-5', count: 308 },
		{ model: 'claude-haiku-4-5-20251001', count: 85 }
	];

	const TOKEN_USAGE = {
		input_tokens: 301_926,
		output_tokens: 522_377,
		cache_read_tokens: 176_519_394,
		cache_write_tokens: 12_321_453,
		response_count: 896,
		by_model: [
			{
				model: 'claude-sonnet-5',
				response_count: 508,
				input_tokens: 180_112,
				output_tokens: 331_004,
				cache_read_tokens: 120_004_112,
				cache_write_tokens: 8_004_221
			},
			{
				model: 'claude-fable-5',
				response_count: 305,
				input_tokens: 101_440,
				output_tokens: 160_311,
				cache_read_tokens: 50_112_884,
				cache_write_tokens: 3_900_112
			},
			{
				model: 'claude-haiku-4-5-20251001',
				response_count: 83,
				input_tokens: 20_374,
				output_tokens: 31_062,
				cache_read_tokens: 6_402_398,
				cache_write_tokens: 417_120
			}
		]
	};
</script>

<Story
	name="LongSession"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(canvas.getByText('claude-sonnet-5')).toBeTruthy();
		await expect(canvas.getByText('claude-fable-5')).toBeTruthy();
		await expect(canvas.getByText('claude-haiku-4-5-20251001')).toBeTruthy();
		const modelRows = canvasElement.querySelectorAll('.sm-model-name');
		await expect(modelRows.length).toBe(3);
		await expect(canvasElement.textContent).not.toContain('+2');
		const sonnetRow = Array.from(canvasElement.querySelectorAll('.sm-model-row')).find((r) =>
			r.textContent?.includes('claude-sonnet-5')
		);
		await expect(sonnetRow?.textContent?.replace(/\s+/g, ' ')).toContain('512');

		const tokenRow = Array.from(canvasElement.querySelectorAll('.sm-row')).find((r) =>
			r.querySelector('.sm-k')?.textContent?.includes('Tokens')
		);
		await expect(tokenRow).not.toBeUndefined();
		await expect(tokenRow?.querySelector('.sm-v')?.textContent?.trim()).toBe('824,303');
		await expect(tokenRow?.querySelector('.sm-d')?.textContent?.replace(/\s+/g, ' ').trim()).toBe(
			'301,926 in · 522,377 out'
		);

		await expect(canvasElement.textContent).not.toMatch(/\d+\.\d+[KMB]\b/);

		const reqRow = Array.from(canvasElement.querySelectorAll('.sm-row')).find((r) =>
			r.querySelector('.sm-k')?.textContent?.includes('Requests')
		);
		await expect(reqRow?.textContent?.replace(/\s+/g, ' ')).toContain('9 without usage');

		await expect(canvas.getByText('Last request')).toBeTruthy();
		await expect(canvas.getByText('Completion check')).toBeTruthy();
		await expect(canvas.getByText('Operator asks')).toBeTruthy();
		await expect(canvas.getByText('Outcomes')).toBeTruthy();

		const outcomes = canvasElement.querySelectorAll('.sm-outcome');
		await expect(outcomes.length).toBe(5);
		const accepted = Array.from(outcomes).find((o) => o.textContent?.includes('accepted'));
		await expect(accepted?.textContent).toContain('8');
		await expect(canvasElement.textContent?.replace(/\s+/g, ' ')).toContain('22 graded');

		const card = canvasElement.querySelector('.sm-card.edge');
		await expect(card).not.toBeNull();
		await expect(card?.classList.contains('edge-magenta')).toBe(false);
		await expect(card?.classList.contains('edge-cyan')).toBe(true);

		const text = (canvasElement.textContent ?? '').replace(/\s+/g, ' ');
		await expect(text).toContain('2 session recaps');
		await expect(text).not.toContain('session-recap');
	}}
>
	{#snippet template()}
		<div style="background:var(--surface-bg); padding:24px; max-width:380px;">
			<SessionMetrics
				requestCount={905}
				status={{ kind: 'ok', label: 'Complete' }}
				models={MODELS}
				tokenUsage={TOKEN_USAGE}
				firstSeen="2026-08-05T08:25:56Z"
				lastSeen="2026-08-05T17:55:38Z"
				avgLatency={{ ms: 10392, partial: false, sampleCount: 905 }}
				promptKindCounts={{ operator: 20, 'session-recap': 2, 'hook-feedback': 0 }}
				outcomeCounts={{ accepted: 8, shipped: 7, corrected: 3, abandoned: 3, unknown: 1 }}
				validation="pass"
				validationSince="2026-08-05T17:50:00Z"
				now={new Date('2026-08-05T18:00:00Z').getTime()}
			/>
		</div>
	{/snippet}
</Story>
