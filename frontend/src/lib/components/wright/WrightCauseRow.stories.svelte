<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import {
		LONG_RESOURCE,
		WAITED_HOURS,
		absorbedFixture,
		ago,
		causeFixture,
		causes
	} from '../__fixtures__/wright';
	import WrightCauseRow from './WrightCauseRow.svelte';

	const { Story } = defineMeta({
		title: 'Wright/WrightCauseRow',
		component: WrightCauseRow,
		tags: ['autodocs']
	});

	const ALL = causes();
	const byId = (id: number) => ALL.find((c) => c.cause_id === id)!;
	const WAITING = byId(10);
	const FOUND = byId(18);
	const FILED = byId(21);
	const KNOWN_FIX = byId(22);
	const QUEUED = byId(23);
	/** A filed follow-up that has no issue yet: the state is still "filed", with no issue link. */
	const FILED_NO_ISSUE = causeFixture({ cause_id: 30, followup_id: 15, opened_at: ago(60 * 2) });

	const frame =
		'list-style: none; margin: 0; padding: 0; max-width: 40rem; --edge-surface: var(--surface-card);';

	/** Pinned, moved by the operator, in a group, with two merges: every mark the row can carry. */
	const OPERATOR = causeFixture({
		cause_id: 50,
		check_name: 'slow_queries',
		resource: 'query 1: SELECT * FROM memory.operator_turns',
		problems: 3,
		pinned: true,
		pinned_at: ago(30),
		operator_priority: 120,
		group_id: 3,
		group_name: 'Mail delivery',
		absorbed: [absorbedFixture(), absorbedFixture({ merge_id: 6, cause_id: 32, by: 'engineer' })]
	});
	const PLAIN = causeFixture({
		cause_id: 51,
		check_name: 'slow_queries',
		resource: 'query 2: SELECT * FROM memory.operator_turns',
		problems: 3
	});
	const onSelect = fn();
	const onPin = fn();
	const onMove = fn();
	const onReset = fn();
	const onGroup = fn();
	const WIDE = 'list-style: none; margin: 0; padding: 0; max-width: 64rem; --edge-surface: var(--surface-card);';
</script>

<Story
	name="Waiting"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText('Waiting', { selector: '[data-slot="badge"] *' })).toBeVisible();
		await expect(canvas.getByText('decisions_health')).toBeVisible();
		await expect(canvas.getByText('blocked_dialog')).toBeVisible();
		await expect(canvas.getByText(/^16 problems · Open 30h · no attempts$/)).toBeVisible();
		await expect(canvasElement.querySelector('[data-state="waiting"]')).not.toBeNull();
		await expect(canvas.getByRole('link', { name: /Problem 707/ })).toHaveAttribute(
			'href',
			'#problem-707'
		);
	}}
>
	{#snippet template()}
		<ul class="edge" style={frame}>
			<WrightCauseRow cause={WAITING} waitedHours={WAITED_HOURS} />
		</ul>
	{/snippet}
</Story>

<Story
	name="Found"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvasElement.querySelector('[data-state="found"]')).not.toBeNull();
		await expect(canvas.getByText('Found', { selector: '[data-slot="badge"] *' })).toBeVisible();
		await expect(canvas.getByRole('link', { name: /Finding 5/ })).toHaveAttribute(
			'href',
			'#finding-5'
		);
		await expect(canvas.getByText(/^7 problems · Open 2d · 2 attempts$/)).toBeVisible();
	}}
>
	{#snippet template()}
		<ul class="edge" style={frame}>
			<WrightCauseRow cause={FOUND} waitedHours={WAITED_HOURS} />
		</ul>
	{/snippet}
</Story>

<Story
	name="Filed with an issue"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvasElement.querySelector('[data-state="filed"]')).not.toBeNull();
		await expect(canvas.getByText('Filed', { selector: '[data-slot="badge"] *' })).toBeVisible();
		const issue = canvas.getByRole('link', { name: /Issue #272/ });
		await expect(issue).toHaveAttribute('href', 'https://github.com/temet-ai/mike/issues/272');
		await expect(issue).toHaveAttribute('target', '_blank');
	}}
>
	{#snippet template()}
		<ul class="edge" style={frame}>
			<WrightCauseRow cause={FILED} waitedHours={WAITED_HOURS} />
		</ul>
	{/snippet}
</Story>

<Story
	name="Filed without an issue"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvasElement.querySelector('[data-state="filed"]')).not.toBeNull();
		await expect(canvas.queryByRole('link', { name: /Issue/ })).toBeNull();
	}}
>
	{#snippet template()}
		<ul class="edge" style={frame}>
			<WrightCauseRow cause={FILED_NO_ISSUE} waitedHours={WAITED_HOURS} />
		</ul>
	{/snippet}
</Story>

<Story
	name="Known fix"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvasElement.querySelector('[data-state="known_fix"]')).not.toBeNull();
		await expect(
			canvas.getByText('Known fix', { selector: '[data-slot="badge"] *' })
		).toBeVisible();
		// One problem: the count is left out.
		await expect(canvas.getByText(/^Open 3h · no attempts$/)).toBeVisible();
	}}
>
	{#snippet template()}
		<ul class="edge" style={frame}>
			<WrightCauseRow cause={KNOWN_FIX} waitedHours={WAITED_HOURS} />
		</ul>
	{/snippet}
</Story>

<Story
	name="Queued"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvasElement.querySelector('[data-state="queued"]')).not.toBeNull();
		await expect(canvas.getByText('Queued', { selector: '[data-slot="badge"] *' })).toBeVisible();
		await expect(canvas.getByText(/^Open 25m · no attempts$/)).toBeVisible();
	}}
>
	{#snippet template()}
		<ul class="edge" style={frame}>
			<WrightCauseRow cause={QUEUED} waitedHours={WAITED_HOURS} />
		</ul>
	{/snippet}
</Story>

<Story
	name="Long resource on one line"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const line = canvasElement.querySelector('.cause__resource') as HTMLElement;
		await expect(line.textContent).toBe(LONG_RESOURCE);
		await expect(line).toHaveAttribute('title', LONG_RESOURCE);
		await expect(line.scrollHeight).toBeLessThanOrEqual(line.clientHeight + 1);
		// The whole text is one click away.
		await userEvent.click(canvas.getByRole('button', { name: /slow_queries/, expanded: false }));
		const full = canvasElement.querySelector('.cause__full') as HTMLElement;
		await expect(full.textContent).toBe(LONG_RESOURCE);
		await expect(canvas.getByText(/^7 problems open · 1733 observations/)).toBeVisible();
	}}
>
	{#snippet template()}
		<ul class="edge" style={frame}>
			<WrightCauseRow cause={FOUND} waitedHours={WAITED_HOURS} />
		</ul>
	{/snippet}
</Story>

<Story
	name="State change keeps the height"
	play={async ({ canvasElement }) => {
		const waiting = canvasElement.querySelector('#waiting')!.getBoundingClientRect();
		const queued = canvasElement.querySelector('#queued')!.getBoundingClientRect();
		await expect(Math.round(queued.height)).toBe(Math.round(waiting.height));
	}}
>
	{#snippet template()}
		<div style="display: flex; flex-direction: column; gap: 16px;">
			<ul class="edge" style={frame} id="waiting">
				<WrightCauseRow
					cause={causeFixture({ cause_id: 40, opened_at: ago(60 * 30) })}
					waitedHours={WAITED_HOURS}
				/>
			</ul>
			<ul class="edge" style={frame} id="queued">
				<WrightCauseRow
					cause={causeFixture({ cause_id: 40, opened_at: ago(60) })}
					waitedHours={WAITED_HOURS}
				/>
			</ul>
		</div>
	{/snippet}
</Story>

<Story
	name="Operator actions"
	play={async ({ canvasElement }) => {
		const pinned = canvasElement.querySelector('#cause-50') as HTMLElement;
		const plain = canvasElement.querySelector('#cause-51') as HTMLElement;
		const mine = within(pinned);

		// The marks the operator's actions leave on the row.
		await expect(mine.getByText('Pinned')).toBeVisible();
		await expect(mine.getByText('memory tables')).toBeVisible();
		await expect(mine.getByText('Mail delivery')).toBeVisible();
		await expect(mine.getByText('Moved by you')).toBeVisible();
		await expect(mine.getByText('2 merged')).toBeVisible();
		await expect(pinned).toHaveAttribute('data-pinned');
		await expect(pinned).toHaveAttribute('data-drop', 'before');

		// The actions sit beside the toggle and each does one thing.
		await userEvent.click(mine.getByRole('button', { name: 'Unpin slow_queries' }));
		await expect(onPin).toHaveBeenCalledWith(false);
		await userEvent.click(mine.getByRole('button', { name: 'Move slow_queries up' }));
		await expect(onMove).toHaveBeenCalledWith('up');
		await expect(mine.getByRole('button', { name: 'Move slow_queries down' })).toBeDisabled();
		await userEvent.click(mine.getByRole('button', { name: 'Reset the order of slow_queries' }));
		await expect(onReset).toHaveBeenCalled();
		await userEvent.click(mine.getByRole('button', { name: 'Add slow_queries to a group' }));
		await expect(onGroup).toHaveBeenCalled();
		await userEvent.click(mine.getByRole('checkbox', { name: 'Select slow_queries' }));
		await expect(onSelect).toHaveBeenCalledWith(true);

		// Reset keeps its slot and is off while there is nothing to reset.
		const reset = plain.querySelector('.cause__act--idle') as HTMLElement;
		await expect(reset).toHaveAttribute('aria-label', 'Reset the order of slow_queries');
		await expect(reset).toBeDisabled();
		await expect(reset).not.toBeVisible();

		// No action is inside the toggle button.
		const toggle = mine.getByRole('button', { name: /query 1/, expanded: false });
		await expect(toggle.querySelector('button, input, a')).toBeNull();

		// Pinning or moving a row does not change its height.
		const main = (row: HTMLElement): number =>
			(row.querySelector('.cause__main') as HTMLElement).getBoundingClientRect().height;
		await expect(main(pinned)).toBe(main(plain));
	}}
>
	{#snippet template()}
		<ul class="edge" style={WIDE}>
			<WrightCauseRow
				cause={OPERATOR}
				waitedHours={WAITED_HOURS}
				selected={false}
				{onSelect}
				canMoveUp={true}
				canMoveDown={false}
				{onPin}
				{onMove}
				{onReset}
				{onGroup}
				draggable={true}
				dropEdge="before"
			/>
			<WrightCauseRow
				cause={PLAIN}
				waitedHours={WAITED_HOURS}
				onSelect={() => {}}
				canMoveUp={true}
				canMoveDown={true}
				{onPin}
				{onMove}
				{onReset}
				{onGroup}
			/>
		</ul>
	{/snippet}
</Story>
