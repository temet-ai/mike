<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect } from 'storybook/test';
	import SessionOverviewSummary from './SessionOverviewSummary.svelte';
	import type { SessionQualityDto } from './SessionOverviewSummary.svelte';

	const JUDGED_QUALITY: SessionQualityDto = {
		outcome: 'success',
		quality_score: 0.87,
		rationale:
			'The agent successfully implemented the requested JWT middleware refactor across three files. ' +
			'It correctly identified the sliding-window expiry requirement, added structured error bodies ' +
			'to the 401 path, and covered the new code with 14 integration tests. The implementation ' +
			'stays within the originally scoped file list and does not introduce any build warnings. ' +
			'Minor gap: the frontend refresh-token effect was noted as untested in the result, but this ' +
			'was acknowledged in the handoff summary and left to a follow-on turn. Overall the session ' +
			'delivered a working, tested, production-ready implementation of the stated goal.',
		judge_model: 'claude-opus-4-8',
		rubric_version: 2,
		judged_at: '2026-06-25T09:14:32Z'
	};

	const SESSION_META = {
		id: 'a3f8b2c1-4e5d-6f7a-8b9c-0d1e2f3a4b5c',
		project: '/Users/dev/projects/temet/temet-proxy',
		title: 'JWT middleware refactor',
		request_count: 47,
		turn_count: 12,
		status: 'Complete',
		models: ['claude-opus-4-8', 'claude-haiku-4-5-20251001']
	};

	const { Story } = defineMeta({
		title: 'Shell/SessionOverviewSummary',
		component: SessionOverviewSummary,
		tags: ['autodocs'],
		parameters: {
			docs: {
				description: {
					component:
						'Session judge verdict block. Sits above the kickoff prompt on the session page. ' +
						'Handles quality==null (not-yet-judged) as a first-class state. ' +
						'Rationale renders in full — no truncation. ' +
						'Design brief: artifacts/2026-06-25/graders-ui-design.md §1.'
				}
			}
		}
	});
</script>

<Story
	name="JudgedSession"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const outcomeChip = canvas.getByRole('status');
		await expect(outcomeChip).toHaveTextContent('SUCCESS');

		const meter = canvasElement.querySelector('[role="meter"]');
		await expect(meter).not.toBeNull();
		await expect(meter?.getAttribute('aria-valuenow')).toBe('87');

		const rationaleEl = canvasElement.querySelector('.sos-rationale');
		await expect(rationaleEl).not.toBeNull();
		await expect(rationaleEl?.textContent).toContain('production-ready implementation');

		const metaEl = canvasElement.querySelector('.sos-meta');
		await expect(metaEl?.textContent).toContain('claude-opus-4-8');
		await expect(metaEl?.textContent).toContain('rubric v2');
	}}
>
	{#snippet template()}
		<div style="width: 720px; background: var(--surface-bg); padding: 20px;">
			<SessionOverviewSummary quality={JUDGED_QUALITY} />

			<div style="margin-top: 24px;">
				<p
					style="font-size: 10px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--text-3); margin-bottom: 8px;"
				>
					← null quality (not-yet-judged state, same component)
				</p>
				<SessionOverviewSummary quality={null} />
			</div>
		</div>
	{/snippet}
</Story>
