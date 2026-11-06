<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect } from 'storybook/test';
	import InterventionRow from './InterventionRow.svelte';
	import type { SessionSteeringRow } from '$lib/api/steering';

	const BASE_PATH = '/sessions/f47ac10b-58cc-4372-a567-0e02b2c3d479';

	function makeRow(overrides: Partial<SessionSteeringRow> = {}): SessionSteeringRow {
		return {
			intervention_id: 1001,
			session_id: 'f47ac10b-58cc-4372-a567-0e02b2c3d479',
			session_uuid: 'f47ac10b-58cc-4372-a567-0e02b2c3d479',
			project_path: '/Users/dev/projects/my-app',
			steerer_name: 'drift-guard',
			steerer_display_name: 'Drift Guard',
			verdict: 'no_drift',
			authority: null,
			mode: null,
			trigger_request_id: null,
			sent_request_id: null,
			sent_response_id: null,
			operator_turn_id: null,
			gate_reason: null,
			off_thread_topic: null,
			confidence: 0.12,
			anchor_text: null,
			verdict_json: null,
			payload_text: null,
			steerer_model: 'google/gemini-3.8-flash',
			sent_at: null,
			processed_at: '2026-06-17T10:34:00Z',
			expires_at: null,
			reverted_at: null,
			revert_reason: null,
			created_at: '2026-06-17T10:33:59Z',
			...overrides
		};
	}

	const firedNoLinks = makeRow({
		intervention_id: 1006,
		verdict: 'fired',
		authority: 'redirect',
		mode: 'augment',
		confidence: 0.87,
		trigger_request_id: null,
		sent_request_id: null,
		sent_response_id: null,
		off_thread_topic:
			'User asked about general cooking techniques instead of the assigned debugging task.',
		anchor_text:
			'## Thread: Debug the authentication middleware\n\nFocus on tracing the JWT validation path...',
		payload_text:
			'<system-reminder>\nPlease return to the task: debugging the authentication middleware.\n</system-reminder>',
		verdict_json: { verdict: 'fired', confidence: 0.87, topic_drift: true },
		sent_at: '2026-06-17T10:32:45Z',
		processed_at: '2026-06-17T10:32:46Z',
		expires_at: '2026-06-17T10:37:45Z',
		created_at: '2026-06-17T10:32:44Z'
	});

	const fullContentRow = makeRow({
		intervention_id: 1013,
		verdict: 'fired',
		authority: 'redirect',
		mode: 'augment',
		confidence: 0.91,
		off_thread_topic:
			'The user asked a detailed question about competitor pricing models and licensing structures, going well outside the agreed task scope of implementing a Rust async runtime.',
		anchor_text: [
			'Turn #42 context window snapshot (full verbatim):',
			'',
			'User: How does Claude pricing compare to GPT-4? What about volume discounts?',
			'',
			'Assistant (prev): I can help with the Rust async patterns you mentioned—',
			'',
			'[3 tool calls: Read, Edit, Bash]'
		].join('\n'),
		payload_text:
			'You claimed the tests pass, but no test command ran this turn - run the suite and show its output.',
		verdict_json: {
			model: 'google/gemini-3.8-flash',
			drift_score: 0.91,
			topic_vector_distance: 0.83,
			detected_categories: ['off_topic_commercial', 'competitor_mention'],
			threshold_used: 0.75,
			reasoning: 'High cosine distance from task embedding + two detected off-topic categories.'
		},
		gate_reason: null,
		sent_request_id: null,
		sent_response_id: null,
		trigger_request_id: null,
		sent_at: '2026-06-17T10:30:05Z',
		processed_at: '2026-06-17T10:30:08Z',
		created_at: '2026-06-17T10:30:00Z'
	});

	const { Story } = defineMeta({
		title: 'Components/Shell/InterventionRow',
		component: InterventionRow,
		tags: ['autodocs'],
		parameters: {
			layout: 'padded'
		}
	});
</script>

<Story
	name="InspectionFidelityFullContent"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const headerBtn = canvas.getByRole('button', { name: /toggle intervention detail/i });
		await expect(headerBtn.getAttribute('aria-expanded')).toBe('false');

		const topic = canvasElement.querySelector('.iv-row__topic');
		await expect(topic).toBeTruthy();
		await expect(topic?.textContent).toContain('competitor pricing models');

		const conf = canvasElement.querySelector('.iv-row__confidence');
		await expect(conf).toBeTruthy();
		await expect(conf?.textContent?.trim()).toBe('0.91');

		const dot = canvasElement.querySelector('.iv-chip__dot');
		await expect(dot).toBeTruthy();
		await expect(dot?.getAttribute('aria-hidden')).toBe('true');

		await userEvent.click(headerBtn);
		await expect(headerBtn.getAttribute('aria-expanded')).toBe('true');

		const proseParagraphs = Array.from(canvasElement.querySelectorAll('.iv-prose'));
		const topicProse = proseParagraphs.find((p) => p.textContent?.includes('competitor pricing'));
		await expect(topicProse).toBeTruthy();

		const pres = Array.from(canvasElement.querySelectorAll('pre.iv-pre'));
		await expect(pres.length).toBeGreaterThanOrEqual(2);

		const anchorPre = pres.find((p) => p.textContent?.includes('context window snapshot'));
		await expect(anchorPre).toBeTruthy();
		await expect(anchorPre?.textContent).toContain('[3 tool calls: Read, Edit, Bash]');

		const steerPre = canvasElement.querySelector('pre.isc-steer__pre');
		await expect(steerPre).toBeTruthy();
		await expect(steerPre?.textContent).toContain("Steering note from the operator's proxy");
		await expect(steerPre?.textContent).toContain(
			'You claimed the tests pass, but no test command ran this turn - run the suite and show its output.'
		);
		await expect(steerPre?.textContent).toContain('say so in a line');

		const nudgeText = canvasElement.querySelector('.isc-steer__nudge-text');
		await expect(nudgeText).toBeTruthy();
		await expect(nudgeText?.textContent).toContain(
			'You claimed the tests pass, but no test command ran this turn - run the suite and show its output.'
		);

		const jsonPre = pres.find((p) => p.textContent?.includes('drift_score'));
		await expect(jsonPre).toBeTruthy();
		await expect(jsonPre?.textContent).toContain('"reasoning"');

		const revertBtn = canvas.getByRole('button', { name: /revert this intervention/i });
		await expect((revertBtn as HTMLButtonElement).disabled).toBe(false);

		await userEvent.click(headerBtn);
		await expect(headerBtn.getAttribute('aria-expanded')).toBe('false');
		const bodyGone = canvasElement.querySelector('.iv-row__body');
		await expect(bodyGone).toBeFalsy();
	}}
>
	{#snippet template()}
		<div class="wrap">
			<InterventionRow row={fullContentRow} sessionBasePath={BASE_PATH} />
		</div>
	{/snippet}
</Story>

<style>
	.wrap {
		max-width: 760px;
		padding: 16px 20px;
	}
</style>
