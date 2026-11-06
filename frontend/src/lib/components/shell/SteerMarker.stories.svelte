<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, userEvent } from 'storybook/test';
	import SteerMarker from './SteerMarker.svelte';
	import type { SessionSteeringRow } from '$lib/api/steering';

	const SESSION_ID = 'sess-93cab942-54b8-41e3-8ef8-a505d7353dee';
	const STATIC_NOW = new Date('2026-07-03T10:00:00Z').getTime();

	const VERIFIER_STEER: SessionSteeringRow = {
		intervention_id: 557,
		session_id: SESSION_ID,
		session_uuid: SESSION_ID,
		project_path: '/Users/dev/projects/my-app',
		steerer_name: 'verifier',
		steerer_display_name: 'Verifier',
		verdict: 'fired',
		authority: 'command',
		mode: 'drive',
		trigger_request_id: 92818,
		sent_request_id: 92822,
		sent_response_id: 92823,
		operator_turn_id: null,
		gate_reason: 'stop_response claimed completion without a verification step',
		off_thread_topic: null,
		confidence: 0.91,
		anchor_text: null,
		verdict_json: { drift_score: 0.91, reasoning: 'no test run before the done claim' },
		payload_text: 'You claimed the tests pass, but no test command ran this turn.',
		steerer_model: 'google/gemini-3.8-flash',
		sent_at: '2026-07-03T09:58:00Z',
		processed_at: null,
		expires_at: '2026-07-03T10:08:00Z',
		reverted_at: null,
		revert_reason: null,
		created_at: '2026-07-03T09:57:59Z'
	};

	const { Story } = defineMeta({
		title: 'Shell/SteerMarker',
		component: SteerMarker,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(canvas.getByText('Steer fired')).toBeInTheDocument();
		await expect(canvas.getByText('Verifier')).toBeInTheDocument();
		await expect(canvas.getByText('delivered to #92822')).toBeInTheDocument();
		await expect(canvas.getByText('command')).toBeInTheDocument();
		await expect(canvas.getByText('drive')).toBeInTheDocument();
		await expect(canvas.queryByText(/Steering note from the operator/)).not.toBeInTheDocument();

		const toggle = canvas.getByRole('button', { name: /expand steer fired by Verifier/i });
		await expect(toggle).toHaveAttribute('aria-expanded', 'false');
		await userEvent.click(toggle);
		await expect(toggle).toHaveAttribute('aria-expanded', 'true');

		await expect(
			canvas.getByRole('link', { name: /delivered to request #92822/i })
		).toBeInTheDocument();
		await expect(
			canvas.getByRole('link', { name: /triggered by request #92818/i })
		).toBeInTheDocument();

		const pre = canvasElement.querySelector('pre.isc-steer__pre');
		await expect(pre).toBeTruthy();
		await expect(pre?.textContent).toContain("Steering note from the operator's proxy");
		await expect(pre?.textContent).toContain(
			'You claimed the tests pass, but no test command ran this turn.'
		);
		await expect(pre?.textContent).toContain('say so in a line');

		const nudgeText = canvasElement.querySelector('.isc-steer__nudge-text');
		await expect(nudgeText?.textContent).toBe(
			'You claimed the tests pass, but no test command ran this turn.'
		);
	}}
>
	{#snippet template()}
		<div style="max-width: 640px; background: var(--surface-bg); padding: 16px;">
			<SteerMarker
				intervention={VERIFIER_STEER}
				sessionBasePath={`/sessions/${SESSION_ID}`}
				now={STATIC_NOW}
			/>
		</div>
	{/snippet}
</Story>
