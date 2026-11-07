<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import { setContext } from 'svelte';
	import WorkbenchOtherPromptRow from './WorkbenchOtherPromptRow.svelte';
	import { FILTER_RULES_STORE_KEY } from '$lib/context-keys';
	import { createMockFilterRulesStore } from '$lib/storybook/mockFilterRulesStore.svelte';
	import type { FilterRuleDto } from '$lib/types';

	function buildRule(overrides: Partial<FilterRuleDto> = {}): FilterRuleDto {
		return {
			id: 'other-row-1',
			kind: 'text_substring',
			value: 'Avoid using emojis in all communication unless asked.',
			action: 'drop',
			text: null,
			title: null,
			enabled: true,
			source: 'ui',
			updated_at: '2026-09-20T12:00:00Z',
			sort_order: 1,
			trigger_token: null,
			fires_24h: 2,
			fires_7d: 12,
			last_fired_at: '2026-09-19T08:00:00Z',
			...overrides
		};
	}

	const { Story } = defineMeta({
		title: 'Routes/Filter/WorkbenchOtherPromptRow',
		component: WorkbenchOtherPromptRow,
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	const store = createMockFilterRulesStore([buildRule()]);
	// svelte-ignore state_referenced_locally -- one-time context registration for the story
	setContext(FILTER_RULES_STORE_KEY, store);

	const rule = $derived(store.rules.find((r) => r.id === 'other-row-1') ?? store.rules[0]!);
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(
			await canvas.findByText('Removes “Avoid using emojis in all communication unless asked.”')
		).toBeInTheDocument();
		await expect(canvas.getByText(/^last fired /)).toBeInTheDocument();

		const jump = await canvas.findByRole('link', { name: /Open in workbench:/ });
		await expect(jump).toHaveAttribute(
			'href',
			'/filter/workbench?from=request:9001&rule=other-row-1'
		);

		// Delete asks for inline confirmation — no browser confirm(), no modal — and Cancel backs
		// out without touching the store.
		const deleteBtn = await canvas.findByRole('button', { name: /Delete edit:/ });
		await userEvent.click(deleteBtn);
		const confirmGroup = await canvas.findByRole('group', { name: /Confirm delete:/ });
		await expect(within(confirmGroup).getByText('Delete?')).toBeInTheDocument();
		await userEvent.click(within(confirmGroup).getByRole('button', { name: 'Cancel' }));
		await expect(canvas.queryByText('Delete?')).toBeNull();

		const enableSwitch = await canvas.findByRole('switch');
		await expect(enableSwitch).toHaveAttribute('aria-checked', 'true');
		await userEvent.click(enableSwitch);
		await expect(enableSwitch).toHaveAttribute('aria-checked', 'false');
	}}
>
	{#snippet template()}
		<div class="w-[420px] bg-[var(--surface-bg)] p-2">
			<WorkbenchOtherPromptRow {rule} latestRequestId="9001" />
		</div>
	{/snippet}
</Story>
