<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import { setContext } from 'svelte';
	import WorkbenchRuleStack from './WorkbenchRuleStack.svelte';
	import { FILTER_RULES_STORE_KEY } from '$lib/context-keys';
	import { createMockFilterRulesStore } from '$lib/storybook/mockFilterRulesStore.svelte';
	import type { FilterRuleDto } from '$lib/types';

	function buildRule(overrides: Partial<FilterRuleDto> = {}): FilterRuleDto {
		return {
			id: 'rule-stack-1',
			kind: 'text_substring',
			value: 'system-reminder',
			action: 'drop',
			text: null,
			title: null,
			enabled: true,
			source: 'ui',
			updated_at: '2026-06-17T12:00:00Z',
			sort_order: 1,
			trigger_token: null,
			fires_24h: 0,
			fires_7d: 0,
			last_fired_at: null,
			...overrides
		};
	}

	const BLOCK_TEXTS = [
		'You are Claude Code. Avoid using emojis in all communication unless asked.',
		'# Claude in Chrome browser automation\nUse browser tools when instructed.'
	];

	const PICKED_PROJECT_PATH = '/mnt/data/projects/picked';
	const OTHER_PROJECT_PATH = '/mnt/data/projects/other';

	const RULES: FilterRuleDto[] = [
		buildRule({
			id: 'r-emoji',
			value: 'Avoid using emojis in all communication unless asked.',
			action: 'drop',
			sort_order: 1
		}),
		buildRule({
			id: 'r-chrome',
			kind: 'xml_tag',
			value: 'Claude in Chrome browser automation',
			action: 'replace',
			text: 'Browser tools are unavailable in this environment.',
			title: 'Swaps the Chrome section for an unavailable notice',
			sort_order: 2,
			// Scoped to the WORKBENCH'S PICKED project (not Global) — still fires, and its badge
			// shows the picked project's name rather than "Global".
			project_path: PICKED_PROJECT_PATH
		}),
		buildRule({
			id: 'r-no-match',
			value: 'this text never appears anywhere in the sample prompt',
			action: 'drop',
			sort_order: 3
		}),
		// Disabled, sorted first (sort_order: 0) — must still render, at the BOTTOM of the list
		// (after the two enabled edits above), dimmed, switch off (G4: the switch is two-way and
		// a disabled edit stays reachable to turn back on).
		buildRule({
			id: 'r-disabled',
			value: 'Be concise.',
			action: 'drop',
			enabled: false,
			sort_order: 0
		}),
		// Scoped to a project OTHER than the one picked — its anchor DOES match this prompt
		// (same value as r-emoji) and the dry run below even lists it as firing, but it must
		// still land under "Edits for other prompts" grouped by its own project, never in the
		// firing list: project scope is excluded before the dry-run signal is even consulted.
		buildRule({
			id: 'r-other-project',
			value: 'Avoid using emojis in all communication unless asked.',
			action: 'drop',
			sort_order: 4,
			project_path: OTHER_PROJECT_PATH
		})
	];

	const { Story } = defineMeta({
		title: 'Routes/Filter/WorkbenchRuleStack',
		component: WorkbenchRuleStack,
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	let createRequest = $state<{ value: string; action: FilterRuleDto['action'] } | null>(null);

	const store = createMockFilterRulesStore(RULES);
	// svelte-ignore state_referenced_locally -- one-time context registration for the story
	setContext(FILTER_RULES_STORE_KEY, store);
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(await canvas.findByText('Your edits')).toBeInTheDocument();
		// Only 2 of the 4 rules fire on this sample prompt (`r-no-match`'s value never appears,
		// and `r-other-project` is excluded by project scope even though its anchor matches and
		// the dry run lists it as firing); the "N changes this prompt" count reflects that, not
		// the raw enabled-rule count (round-4 defect #4).
		await expect(await canvas.findByText('2 changes this prompt')).toBeInTheDocument();
		await expect(
			await canvas.findByText('Swaps the Chrome section for an unavailable notice')
		).toBeInTheDocument();
		await expect(
			await canvas.findByText('Removes “Avoid using emojis in all communication unless asked.”')
		).toBeInTheDocument();

		// No rule ids anywhere.
		await expect(canvas.queryByText('r-emoji')).toBeNull();
		await expect(canvas.queryByText('r-chrome')).toBeNull();

		// Labels: the global firing edit says "Global"; the one scoped to the WORKBENCH'S PICKED
		// project says the picked project's name, never its raw path.
		await expect(await canvas.findByTitle('Applies to every project')).toHaveTextContent('Global');
		await expect(
			await canvas.findByTitle(`Scoped to project: ${PICKED_PROJECT_PATH}`)
		).toHaveTextContent('Picked Project');

		// Neither the disabled edit, the one whose anchor never appears in this prompt, nor the
		// one scoped to a DIFFERENT project shows in the firing list (round-4 regroup: "Your
		// edits" only lists what fires HERE, in scope).
		await expect(canvas.queryByText('Removes “Be concise.”')).toBeNull();
		await expect(
			canvas.queryByText('Removes “this text never appears anywhere in the sample…”')
		).toBeNull();
		await expect(canvas.queryByText('Not in this prompt')).toBeNull();

		// All three land in the collapsed-by-default "Edits for other prompts" group: the two
		// matching no catalogue class, plus the other-project one under ITS OWN project heading.
		const otherToggle = await canvas.findByRole('button', { name: 'Edits for other prompts (3)' });
		await expect(otherToggle).toHaveAttribute('aria-expanded', 'false');
		await userEvent.click(otherToggle);
		await expect(otherToggle).toHaveAttribute('aria-expanded', 'true');

		await expect(await canvas.findByText('Other Project')).toBeInTheDocument();
		await expect(await canvas.findByText('Matches no recent prompt')).toBeInTheDocument();
		const disabledRow = (await canvas.findByText('Removes “Be concise.”')).closest(
			'[data-slot="workbench-other-prompt-row"]'
		);
		expect(disabledRow).not.toBeNull();
		const disabledSwitch = within(disabledRow as HTMLElement).getByRole('switch');
		await expect(disabledSwitch).toHaveAttribute('aria-checked', 'false');
		await userEvent.click(disabledSwitch);
		await expect(disabledSwitch).toHaveAttribute('aria-checked', 'true');
		await userEvent.click(disabledSwitch);
		await expect(disabledSwitch).toHaveAttribute('aria-checked', 'false');

		const addBtn = await canvas.findByRole('button', { name: 'Add edit manually' });
		await userEvent.click(addBtn);
		await expect(await canvas.findByText('Add filter rule')).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="h-[520px] w-[380px] bg-[var(--surface-bg)]">
			<WorkbenchRuleStack
				orderedRules={store.rules}
				blockTexts={BLOCK_TEXTS}
				firingRuleIds={new Set(['r-emoji', 'r-chrome', 'r-other-project'])}
				hasSource={true}
				loading={false}
				error={null}
				catalogueClasses={[]}
				catalogueDetailsSettled={true}
				{createRequest}
				onCreateConsumed={() => (createRequest = null)}
				onAsk={() => {}}
				onShowInPrompt={() => {}}
				onToggleCollapse={() => {}}
				pickedProject={PICKED_PROJECT_PATH}
				pickedProjectName="Picked Project"
				projects={[
					{
						project_id: 1,
						project_uuid: 'a',
						project_path: PICKED_PROJECT_PATH,
						session_count: 3,
						request_count: 10,
						first_seen: '2026-01-01T00:00:00Z',
						last_seen: '2026-09-20T00:00:00Z',
						pinned: false,
						custom_name: 'Picked Project'
					},
					{
						project_id: 2,
						project_uuid: 'b',
						project_path: OTHER_PROJECT_PATH,
						session_count: 1,
						request_count: 2,
						first_seen: '2026-01-01T00:00:00Z',
						last_seen: '2026-09-10T00:00:00Z',
						pinned: false,
						custom_name: 'Other Project'
					}
				]}
			/>
		</div>
	{/snippet}
</Story>
