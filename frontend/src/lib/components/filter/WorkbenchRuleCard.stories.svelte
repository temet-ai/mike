<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import { setContext } from 'svelte';
	import WorkbenchRuleCard from './WorkbenchRuleCard.svelte';
	import { FILTER_RULES_STORE_KEY } from '$lib/context-keys';
	import { createMockFilterRulesStore } from '$lib/storybook/mockFilterRulesStore.svelte';
	import type { FilterRuleDto } from '$lib/types';

	function buildRule(overrides: Partial<FilterRuleDto> = {}): FilterRuleDto {
		return {
			id: 'rule-card-1',
			kind: 'text_substring',
			value: 'Avoid using emojis in all communication unless asked.',
			action: 'drop',
			text: null,
			title: null,
			enabled: true,
			source: 'ui',
			updated_at: '2026-04-17T12:00:00Z',
			sort_order: 1,
			trigger_token: null,
			fires_24h: 4,
			fires_7d: 30,
			last_fired_at: '2026-09-20T10:00:00Z',
			...overrides
		};
	}

	const BLOCK_TEXTS = [
		'You are Claude Code, running within the Claude Agent SDK.',
		'# Tone and style\n - Avoid using emojis in all communication unless asked.\n - Be concise.'
	];

	const { Story } = defineMeta({
		title: 'Routes/Filter/WorkbenchRuleCard',
		component: WorkbenchRuleCard,
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	let lastAsk = $state<{ rule: FilterRuleDto; quote: string } | null>(null);
	let lastShowInPrompt = $state<FilterRuleDto | null>(null);
	let open = $state(false);

	const store = createMockFilterRulesStore([buildRule()]);
	// svelte-ignore state_referenced_locally -- one-time context registration for the story
	setContext(FILTER_RULES_STORE_KEY, store);

	const rule = $derived(store.rules.find((r) => r.id === 'rule-card-1') ?? store.rules[0]!);
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		// Derived title shown, no rule id or action code anywhere.
		await expect(
			await canvas.findByText('Removes “Avoid using emojis in all communication unless asked.”')
		).toBeInTheDocument();
		await expect(canvas.queryByText('rule-card-1')).toBeNull();
		await expect(canvas.queryByText('drop')).toBeNull();
		await expect(await canvas.findByText('Tone and style')).toBeInTheDocument();

		// A global rule (no project_path) shows a "Global" badge, not a "Scoped to project" one.
		await expect(canvas.queryByTitle(/Scoped to project/)).toBeNull();
		await expect(await canvas.findByTitle('Applies to every project')).toHaveTextContent('Global');

		// Click the row header (no caret) to open it.
		const toggle = await canvas.findByRole('button', { expanded: false });
		await userEvent.click(toggle);
		await expect(await canvas.findByRole('button', { expanded: true })).toBeInTheDocument();
		await expect(
			await canvas.findByText('Avoid using emojis in all communication unless asked.')
		).toBeInTheDocument();

		const rewordBtn = await canvas.findByRole('button', { name: 'Reword with assistant' });
		await userEvent.click(rewordBtn);

		const showBtn = await canvas.findByRole('button', { name: 'Show in prompt' });
		await userEvent.click(showBtn);

		const handBtn = await canvas.findByRole('button', { name: 'Edit by hand' });
		await userEvent.click(handBtn);
		await expect(await canvas.findByText('Edit filter rule')).toBeInTheDocument();
		await expect(await canvas.findByLabelText('Title')).toBeInTheDocument();

		const enableSwitch = await canvas.findByRole('switch');
		await userEvent.click(enableSwitch);

		// "Only <project>" scopes this global rule to the workbench's picked project (PATCH
		// project_path), and the badge/action flip to match; "Make global" reverses it.
		const scopeBtn = await canvas.findByRole('button', { name: 'Only Picker target' });
		await userEvent.click(scopeBtn);
		await expect(rule.project_path).toBe('/mnt/data/projects/picker-target');
		await expect(
			await canvas.findByTitle('Scoped to project: /mnt/data/projects/picker-target')
		).toHaveTextContent('Picker target');
		const unscopeBtn = await canvas.findByRole('button', { name: 'Make global' });
		await userEvent.click(unscopeBtn);
		await expect(rule.project_path).toBeNull();
		await expect(
			await canvas.findByRole('button', { name: 'Only Picker target' })
		).toBeInTheDocument();

		// Delete asks for inline confirmation — no browser confirm(), no modal — and Cancel
		// backs out without touching the store. Scoped to the confirm group itself: the
		// still-open hand-edit form above has its own "Cancel" button.
		const deleteBtn = await canvas.findByRole('button', { name: /Delete edit:/ });
		await userEvent.click(deleteBtn);
		const confirmGroup = await canvas.findByRole('group', { name: /Confirm delete:/ });
		await expect(within(confirmGroup).getByText('Delete?')).toBeInTheDocument();
		await userEvent.click(within(confirmGroup).getByRole('button', { name: 'Cancel' }));
		await expect(canvas.queryByText('Delete?')).toBeNull();
		await expect(await canvas.findByRole('button', { name: /Delete edit:/ })).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="w-[420px] bg-[var(--surface-bg)] p-2">
			<WorkbenchRuleCard
				{rule}
				blockTexts={BLOCK_TEXTS}
				appliesTo="Applies to: Main session (1 of 2 models)"
				{open}
				onToggleOpen={() => (open = !open)}
				onAsk={(r, quote) => (lastAsk = { rule: r, quote })}
				onShowInPrompt={(r) => (lastShowInPrompt = r)}
				workbenchProjectPath="/mnt/data/projects/picker-target"
				workbenchProjectName="Picker target"
			/>
			{#if lastAsk}
				<p class="mt-2 text-caption text-[var(--text-3)]">
					last ask: {lastAsk.quote}
				</p>
			{/if}
			{#if lastShowInPrompt}
				<p class="text-caption text-[var(--text-3)]">show-in-prompt requested</p>
			{/if}
		</div>
	{/snippet}
</Story>
