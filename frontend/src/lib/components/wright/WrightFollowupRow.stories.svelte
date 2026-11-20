<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import { ago, followupFixture, followupLimitsFixture, followups } from '../__fixtures__/wright';
	import WrightFollowupRow from './WrightFollowupRow.svelte';

	const { Story } = defineMeta({
		title: 'Wright/WrightFollowupRow',
		component: WrightFollowupRow,
		tags: ['autodocs']
	});

	const limits = followupLimitsFixture();
	const ALL = followups();
	const byState = (state: string) => ALL.find((f) => f.state === state)!;

	const SESSION = '9b1c2d3e-4f50-4a61-8b72-0c93d4e5f6a7';
	/** Same title, meta and links on both, so only the state differs. */
	const BEING_FIXED = followupFixture({
		id: 31,
		state: 'handed_off',
		attempts: 2,
		fixer_session: SESSION,
		fixer_machine: 'omit',
		handed_off_at: ago(45)
	});
	const STUCK = { ...BEING_FIXED, state: 'stuck' as const, attempts: 3 };
	const STUCK_NO_ISSUE = followupFixture({
		id: 32,
		state: 'stuck',
		attempts: 3,
		issue_number: null,
		issue_url: null,
		fixer_session: SESSION,
		fixer_machine: 'omit'
	});
	const LONG_TITLE = followupFixture({
		id: 33,
		title:
			'mcp__tether__work_thread_checkpoint_with_a_very_long_tool_name_that_has_no_break_points_at_all_so_it_must_wrap_instead_of_being_clipped_by_the_row',
		owning_part:
			'crates/mike-features/src/steerer/note_policy/admission/quiet_period_calculation_for_every_session.rs'
	});
	const SIX_PROBLEMS = followupFixture({
		id: 34,
		problems: [701, 702, 703, 704, 705, 706],
		findings: [5, 6, 7]
	});
	const LONG_BODY = followupFixture({
		id: 35,
		body: Array.from(
			{ length: 14 },
			(_, i) => `Line ${i + 1}: the handler reads tldr before the schema says it is required.`
		).join('\n')
	});

	const frame =
		'list-style: none; margin: 0; padding: 0; max-width: 40rem; --edge-surface: var(--surface-card);';
</script>

<Story
	name="Not filed"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText('Not filed')).toBeVisible();
		await expect(canvas.getByText('No issue yet.')).toBeVisible();
		await expect(canvas.queryByRole('link', { name: /Issue/ })).toBeNull();
		await expect(canvas.queryByRole('link', { name: /Open issue/ })).toBeNull();
	}}
>
	{#snippet template()}
		<ul class="edge" style={frame}>
			<WrightFollowupRow followup={byState('open')} {limits} />
		</ul>
	{/snippet}
</Story>

<Story
	name="Filed"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText('Filed', { selector: '[data-slot="badge"]' })).toBeVisible();
		await expect(
			canvas.getByText('Issue #272 is open. No session has picked it up.')
		).toBeVisible();
		await expect(canvas.getByRole('link', { name: /Issue #272/ })).toHaveAttribute(
			'href',
			'https://github.com/temet-ai/mike/issues/272'
		);
		await expect(canvas.getByRole('link', { name: /Problem 707/ })).toHaveAttribute(
			'href',
			'#problem-707'
		);
	}}
>
	{#snippet template()}
		<ul class="edge" style={frame}>
			<WrightFollowupRow followup={ALL.find((f) => f.id === 14)!} {limits} />
		</ul>
	{/snippet}
</Story>

<Story
	name="Being fixed"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText('Being fixed')).toBeVisible();
		await expect(canvas.getByText(/omit, attempt 2 of 3, since /)).toBeVisible();
		await expect(
			canvas.getByRole('link', { name: new RegExp(`Fixer omit · ${SESSION}`) })
		).toHaveAttribute('href', `/sessions/${SESSION}`);
		await expect(canvas.queryByRole('link', { name: /Open issue/ })).toBeNull();
	}}
>
	{#snippet template()}
		<ul class="edge" style={frame}>
			<WrightFollowupRow followup={BEING_FIXED} {limits} />
		</ul>
	{/snippet}
</Story>

<Story
	name="Fixed"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText('Fixed', { selector: '[data-slot="badge"]' })).toBeVisible();
		await expect(canvas.getByText(/^Closed .*\.$/)).toBeVisible();
		// The session still reads after the fix.
		await expect(canvas.getByRole('link', { name: /Fixer omit/ })).toBeVisible();
	}}
>
	{#snippet template()}
		<ul class="edge" style={frame}>
			<WrightFollowupRow followup={byState('fixed')} {limits} />
		</ul>
	{/snippet}
</Story>

<Story
	name="Dropped"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText('Dropped')).toBeVisible();
		await expect(canvas.getByText(/^Closed .*: Not planned\.$/)).toBeVisible();
	}}
>
	{#snippet template()}
		<ul class="edge" style={frame}>
			<WrightFollowupRow followup={byState('dropped')} {limits} />
		</ul>
	{/snippet}
</Story>

<Story
	name="Stuck"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText('Stuck', { selector: '[data-slot="badge"]' })).toBeVisible();
		await expect(
			canvas.getByText('3 sessions ended without a fix. Wright has stopped trying.')
		).toBeVisible();
		const open = canvas.getByRole('link', { name: /Open issue #277/ });
		await expect(open).toHaveAttribute('href', 'https://github.com/temet-ai/mike/issues/277');
		await expect(open).toHaveAttribute('target', '_blank');
		await expect(open).toHaveAttribute('rel', 'external noopener noreferrer');
	}}
>
	{#snippet template()}
		<ul class="edge" style={frame}>
			<WrightFollowupRow followup={byState('stuck')} {limits} />
		</ul>
	{/snippet}
</Story>

<Story
	name="Stuck without an issue"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText(/No issue was filed\./)).toBeVisible();
		await expect(canvas.queryByRole('link', { name: /Open issue/ })).toBeNull();
		await expect(canvasElement.querySelector('[data-slot="followup-action"]')).not.toBeNull();
	}}
>
	{#snippet template()}
		<ul class="edge" style={frame}>
			<WrightFollowupRow followup={STUCK_NO_ISSUE} {limits} />
		</ul>
	{/snippet}
</Story>

<Story
	name="State change keeps the height"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const fixing = canvasElement.querySelector('#fixing')!.getBoundingClientRect();
		const stuck = canvasElement.querySelector('#stuck')!.getBoundingClientRect();
		await expect(Math.round(stuck.height)).toBe(Math.round(fixing.height));
		await expect(canvas.getAllByRole('link', { name: /Open issue/ })).toHaveLength(1);
	}}
>
	{#snippet template()}
		<div style="display: flex; flex-direction: column; gap: 16px;">
			<ul class="edge" style={frame} id="fixing">
				<WrightFollowupRow followup={BEING_FIXED} {limits} />
			</ul>
			<ul class="edge" style={frame} id="stuck">
				<WrightFollowupRow followup={STUCK} {limits} />
			</ul>
		</div>
	{/snippet}
</Story>

<Story
	name="Long title and owner"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const title = canvas.getByText(/mcp__tether__work_thread_checkpoint_with_a_very_long/);
		await expect(title.textContent).toBe(LONG_TITLE.title);
		await expect(title.scrollWidth).toBeLessThanOrEqual(title.clientWidth);
	}}
>
	{#snippet template()}
		<ul class="edge" style={frame}>
			<WrightFollowupRow followup={LONG_TITLE} {limits} />
		</ul>
	{/snippet}
</Story>

<Story
	name="Six problems"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		for (const id of SIX_PROBLEMS.problems) {
			await expect(canvas.getByRole('link', { name: new RegExp(`Problem ${id}`) })).toBeVisible();
		}
	}}
>
	{#snippet template()}
		<ul class="edge" style={frame}>
			<WrightFollowupRow followup={SIX_PROBLEMS} {limits} />
		</ul>
	{/snippet}
</Story>

<Story
	name="Open with a long body"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const toggle = canvas.getByRole('button', { name: /work_thread_checkpoint/ });
		await expect(toggle).toHaveAttribute('aria-expanded', 'false');
		await userEvent.click(toggle);
		await expect(toggle).toHaveAttribute('aria-expanded', 'true');
		const body = canvas.getByText(/^Line 1: /);
		await expect(body.textContent).toBe(LONG_BODY.body);
		await expect(getComputedStyle(body).whiteSpace).toBe('pre-wrap');
		await expect(canvas.getByText(/^Noted /)).toBeVisible();
		await expect(canvas.getByRole('list', { name: /Findings for follow-up 35/ })).toBeVisible();
	}}
>
	{#snippet template()}
		<ul class="edge" style={frame}>
			<WrightFollowupRow followup={LONG_BODY} {limits} />
		</ul>
	{/snippet}
</Story>
