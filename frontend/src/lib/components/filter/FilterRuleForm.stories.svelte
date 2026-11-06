<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { fn, userEvent, within, expect } from 'storybook/test';
	import FilterRuleForm from './FilterRuleForm.svelte';
	import type { FilterRuleDto } from '$lib/types';

	function buildRule(overrides: Partial<FilterRuleDto> = {}): FilterRuleDto {
		return {
			id: 'rule-form-1',
			kind: 'text_substring',
			value: 'system-reminder',
			action: 'drop',
			text: null,
			title: null,
			enabled: true,
			source: 'ui',
			updated_at: '2026-04-17T12:00:00Z',
			sort_order: 1,
			trigger_token: null,
			fires_24h: 0,
			fires_7d: 0,
			last_fired_at: null,
			...overrides
		};
	}

	const XML_BLOCK_TEXT = `<system-reminder>
The following skills are available for use with the Skill tool:
- update-config: Use this skill to configure the Claude Code harness.
</system-reminder>`;

	const CANDIDATE_BLOCK_TEXT = `<system-reminder>
This is a hook reminder block.
</system-reminder>`;

	const CANDIDATE_BLOCKS: readonly string[] = [
		'<system-reminder>\nThis is a hook reminder block.\n</system-reminder>',
		'<system-reminder>\nThis is a second reminder block.\n</system-reminder>',
		'Some other block without the tag.'
	];

	const { Story } = defineMeta({
		title: 'Routes/Filter/FilterRuleForm',
		component: FilterRuleForm,
		tags: ['autodocs']
	});
</script>

<Story name="NewEmpty">
	{#snippet template()}
		<div class="p-4 bg-[var(--surface-bg)] w-[420px]">
			<FilterRuleForm density="inline" onCancel={() => {}} onComplete={() => {}} />
		</div>
	{/snippet}
</Story>

<Story name="NewTextSubstringPrefill">
	{#snippet template()}
		<div class="p-4 bg-[var(--surface-bg)] w-[420px]">
			<FilterRuleForm
				blockText="The task tools haven't been used recently. If you're working on tasks that would benefit from tracking progress, consider using TaskCreate to add new tasks."
				density="inline"
				onCancel={() => {}}
				onComplete={() => {}}
			/>
		</div>
	{/snippet}
</Story>

<Story name="NewXmlTagPrefill">
	{#snippet template()}
		<div class="p-4 bg-[var(--surface-bg)] w-[420px]">
			<FilterRuleForm
				blockText={XML_BLOCK_TEXT}
				density="inline"
				onCancel={() => {}}
				onComplete={() => {}}
			/>
		</div>
	{/snippet}
</Story>

<Story name="EditDropPopover">
	{#snippet template()}
		<div class="p-4 bg-[var(--surface-bg)] w-[360px]">
			<FilterRuleForm
				initialRule={buildRule()}
				density="popover"
				onCancel={() => {}}
				onComplete={() => {}}
			/>
		</div>
	{/snippet}
</Story>

<Story name="EditReplacePopover">
	{#snippet template()}
		<div class="p-4 bg-[var(--surface-bg)] w-[360px]">
			<FilterRuleForm
				initialRule={buildRule({
					action: 'replace',
					text: '[REDACTED]',
					kind: 'xml_tag',
					value: 'system-reminder'
				})}
				density="popover"
				onCancel={() => {}}
				onComplete={() => {}}
			/>
		</div>
	{/snippet}
</Story>

<Story name="EditAppendInline">
	{#snippet template()}
		<div class="p-4 bg-[var(--surface-bg)] w-[420px]">
			<FilterRuleForm
				initialRule={buildRule({
					id: 'rule-form-append',
					action: 'append',
					text: '\n\n[Note: block processed by filter pipeline — 2026-04-17]',
					kind: 'xml_tag',
					value: 'system-reminder'
				})}
				density="inline"
				onCancel={() => {}}
				onComplete={() => {}}
			/>
		</div>
	{/snippet}
</Story>

<Story name="WithCandidateBlocks">
	{#snippet template()}
		<div class="p-4 bg-[var(--surface-bg)] w-[420px]">
			<FilterRuleForm
				blockText={CANDIDATE_BLOCK_TEXT}
				candidateBlocks={CANDIDATE_BLOCKS}
				density="inline"
				onCancel={() => {}}
				onComplete={() => {}}
			/>
		</div>
	{/snippet}
</Story>

<Story name="PrefillValue">
	{#snippet template()}
		<div class="p-4 bg-[var(--surface-bg)] w-[420px]">
			<FilterRuleForm
				prefillValue="sensitive-data-pattern"
				density="inline"
				onCancel={() => {}}
				onComplete={() => {}}
			/>
		</div>
	{/snippet}
</Story>

<Story
	name="KindSwitch"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const textSubstringRadio = canvas.getByRole('radio', { name: 'Text substring' });
		const xmlTagRadio = canvas.getByRole('radio', { name: 'XML tag wrapping' });
		const lazySectionRadio = canvas.getByRole('radio', { name: 'Lazy section (heading)' });

		await expect(textSubstringRadio).toBeChecked();
		await expect(canvas.queryByLabelText(/trigger token/i)).toBeNull();

		await userEvent.click(xmlTagRadio);
		await expect(xmlTagRadio).toBeChecked();
		await expect(canvas.queryByLabelText(/trigger token/i)).toBeNull();

		await userEvent.click(lazySectionRadio);
		await expect(lazySectionRadio).toBeChecked();
		await expect(canvas.getByPlaceholderText(/e\.g\. !!CHROME!!/i)).toBeVisible();

		const dropRadio = canvas.getByRole('radio', { name: 'Drop block' });
		const appendRadio = canvas.getByRole('radio', { name: 'Insert text after block' });
		await expect(dropRadio).toBeDisabled();
		await expect(appendRadio).toBeDisabled();

		await userEvent.click(textSubstringRadio);
		await expect(textSubstringRadio).toBeChecked();
		await expect(canvas.queryByLabelText(/trigger token/i)).toBeNull();
	}}
>
	{#snippet template()}
		<div class="p-4 bg-[var(--bg)] w-[420px]">
			<FilterRuleForm density="inline" onCancel={() => {}} onComplete={() => {}} />
		</div>
	{/snippet}
</Story>

<Story
	name="ActionSwitch"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const dropRadio = canvas.getByRole('radio', { name: 'Drop block' });
		const replaceRadio = canvas.getByRole('radio', { name: /Replace block with/i });
		const appendRadio = canvas.getByRole('radio', { name: 'Insert text after block' });

		await expect(dropRadio).toBeChecked();
		await expect(canvas.queryByPlaceholderText(/Replacement text/i)).toBeNull();

		await userEvent.click(replaceRadio);
		await expect(replaceRadio).toBeChecked();
		await expect(canvas.getByPlaceholderText(/Replacement text/i)).toBeVisible();

		await userEvent.click(appendRadio);
		await expect(appendRadio).toBeChecked();
		await expect(
			canvas.getByPlaceholderText(/Text to append after each matching block/i)
		).toBeVisible();

		await userEvent.click(dropRadio);
		await expect(dropRadio).toBeChecked();
		await expect(canvas.queryByPlaceholderText(/Replacement text/i)).toBeNull();
		await expect(
			canvas.queryByPlaceholderText(/Text to append after each matching block/i)
		).toBeNull();
	}}
>
	{#snippet template()}
		<div class="p-4 bg-[var(--bg)] w-[420px]">
			<FilterRuleForm density="inline" onCancel={() => {}} onComplete={() => {}} />
		</div>
	{/snippet}
</Story>

<Story
	name="CancelFiresCallback"
	tags={['interactive']}
	args={{ onCancel: fn() }}
	play={async ({ canvasElement, args }) => {
		const canvas = within(canvasElement);
		const cancelBtn = canvas.getByRole('button', { name: 'Cancel' });

		await userEvent.click(cancelBtn);

		await expect(args.onCancel).toHaveBeenCalledTimes(1);
	}}
>
	{#snippet template(args)}
		<div class="p-4 bg-[var(--bg)] w-[420px]">
			<FilterRuleForm density="inline" onCancel={args.onCancel} onComplete={() => {}} />
		</div>
	{/snippet}
</Story>

<Story
	name="NewInjectReminder"
	play={async ({ canvasElement }) => {
		const form = canvasElement.querySelector('[data-slot="filter-rule-form"]') as HTMLElement;
		await expect(form, 'form rendered').toBeTruthy();
		const kindRadio = form.querySelector(
			'input[type="radio"][value="inject_reminder"]'
		) as HTMLInputElement;
		await expect(kindRadio, 'inject_reminder kind radio present').toBeTruthy();
		await userEvent.click(kindRadio);
		const actionRadio = form.querySelector('input[type="radio"][name="action"]');
		await expect(actionRadio, 'action selector hidden for inject_reminder').toBeNull();
		const labelInput = form.querySelector('#filter-rule-form-label-label');
		const markerInput = form.querySelector('#filter-rule-form-marker-label');
		await expect(labelInput, 'Label field present').toBeTruthy();
		await expect(markerInput, 'Trigger marker field present').toBeTruthy();
		const submit = form.querySelector('button[type="submit"]') as HTMLButtonElement;
		await expect(submit?.disabled, 'submit disabled when fields empty').toBe(true);
	}}
>
	{#snippet template()}
		<div class="p-4 bg-[var(--bg)] w-[420px]">
			<FilterRuleForm density="inline" onCancel={() => {}} onComplete={() => {}} />
		</div>
	{/snippet}
</Story>

<Story name="EditInjectReminder">
	{#snippet template()}
		<div class="p-4 bg-[var(--bg)] w-[420px]">
			<FilterRuleForm
				initialRule={buildRule({
					id: 'rule-form-inject',
					kind: 'inject_reminder',
					value: 'browser-guidance',
					action: 'replace',
					text: 'Use the browser_action tool to interact with web browsers. Always call screenshot() after each interaction to verify the result.',
					trigger_token: '<chrome>'
				})}
				density="inline"
				onCancel={() => {}}
				onComplete={() => {}}
			/>
		</div>
	{/snippet}
</Story>
