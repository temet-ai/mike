<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect } from 'storybook/test';
	import CapabilityPanel from './CapabilityPanel.svelte';
	import type { CapabilityRow } from '$lib/api/capabilities';

	const MIXED_ROWS: CapabilityRow[] = [
		{
			id: 'steering',
			label: 'Steering & judges',
			state: 'inert',
			detail:
				'Steering is enabled but no OpenRouter API key is configured — every evaluation fails fast and skips',
			fix: 'Set MIKE_STEERER_OPENROUTER_API_KEY (or OPENROUTER_API_KEY)'
		},
		{
			id: 'semantic_recall',
			label: 'Semantic recall',
			state: 'degraded',
			detail:
				'Embedding server at http://127.0.0.1:7997 is unreachable — recall is falling back to full-text search',
			fix: 'Check that the embedding server is running at http://127.0.0.1:7997'
		},
		{
			id: 'weasel_intercept',
			label: 'Weasel intercept',
			state: 'active',
			detail: 'Weasel intercept active — scanning responses against 42 forbidden words',
			fix: ''
		},
		{
			id: 'persistence',
			label: 'Persistence',
			state: 'off',
			detail:
				'Persistence is disabled via MIKE_PERSIST_ENABLED=0 — captures are ephemeral, nothing is stored',
			fix: 'Set MIKE_PERSIST_ENABLED=1 and restart to persist captures'
		}
	];

	const { Story } = defineMeta({
		title: 'Routes/Settings/detail/CapabilityPanel',
		component: CapabilityPanel,
		tags: ['autodocs'],
		parameters: {
			docs: {
				description: {
					component:
						'Per-subsystem honesty panel on /settings: one row per subsystem showing ' +
						'its ACTUAL operational state (active/degraded/inert/off), not just whether ' +
						'its feature toggle is on. Closes the gap where a toggle reads ON but the ' +
						'subsystem behind it is silently doing nothing.'
				}
			}
		},
		argTypes: {
			capabilities: { control: false }
		}
	});
</script>

<Story
	name="Populated"
	tags={['interactive', 'data-driven']}
	args={{ capabilities: MIXED_ROWS }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const rows = canvasElement.querySelectorAll('[data-testid="capability-row"]');
		await expect(rows.length, 'all four capability rows must render').toBe(4);

		await expect(canvasElement.querySelector('.cap-badge--inert')?.textContent).toContain('Inert');
		await expect(canvasElement.querySelector('.cap-badge--degraded')?.textContent).toContain(
			'Degraded'
		);
		await expect(canvasElement.querySelector('.cap-badge--active')?.textContent).toContain(
			'Active'
		);
		await expect(canvasElement.querySelector('.cap-badge--off')?.textContent).toContain('Off');

		await expect(
			canvas.getByText(
				'Steering is enabled but no OpenRouter API key is configured — every evaluation fails fast and skips'
			),
			'the full steering detail string must render, unclipped'
		).toBeInTheDocument();
		await expect(
			canvas.getByText(
				'Embedding server at http://127.0.0.1:7997 is unreachable — recall is falling back to full-text search'
			),
			'the full semantic_recall detail string must render, unclipped'
		).toBeInTheDocument();

		await expect(canvas.getByText(/Set MIKE_STEERER_OPENROUTER_API_KEY/)).toBeInTheDocument();
		await expect(
			canvas.getByText(/Check that the embedding server is running/)
		).toBeInTheDocument();
		await expect(canvas.getByText(/Set MIKE_PERSIST_ENABLED=1/)).toBeInTheDocument();

		const activeRow = canvasElement.querySelector('[data-capability="weasel_intercept"]');
		await expect(activeRow?.querySelector('.cap-fix'), 'active rows render no fix line').toBeNull();
	}}
>
	{#snippet template(args)}
		<div style="max-width:640px; padding:24px; background:var(--surface-bg);">
			<CapabilityPanel {...args} />
		</div>
	{/snippet}
</Story>
