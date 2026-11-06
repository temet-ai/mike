<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor, fireEvent } from 'storybook/test';
	import AddRulePopover from './AddRulePopover.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Pencil } from '@lucide/svelte';
	import type { FilterRuleDto } from '$lib/types';

	function buildRule(overrides: Partial<FilterRuleDto> = {}): FilterRuleDto {
		return {
			id: 'rule-pop-1',
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

	const RULE_TEXT_SUB = buildRule();
	const RULE_XML_REPLACE = buildRule({
		id: 'rule-pop-xml',
		kind: 'xml_tag',
		value: 'system-reminder',
		action: 'replace',
		text: '[REDACTED]'
	});

	const { Story } = defineMeta({
		title: 'Routes/Filter/AddRulePopover',
		component: AddRulePopover,
		tags: ['autodocs'],
		parameters: { a11y: { test: 'error' } }
	});
</script>

<Story name="DefaultClosed">
	{#snippet template()}
		<div class="p-12 bg-[var(--surface-bg)] inline-flex items-center gap-2">
			<span class="font-mono text-[11px] text-[var(--text-2)]"
				>Rule: text_substring "system-reminder"</span
			>
			<AddRulePopover
				initialRule={RULE_TEXT_SUB}
				triggerLabel="Edit rule: text_substring system-reminder"
			>
				{#snippet trigger({ props })}
					<Button {...props} variant="ghost" size="icon" class="size-7" aria-label="Edit this rule">
						<Pencil class="size-3.5" aria-hidden="true" />
					</Button>
				{/snippet}
			</AddRulePopover>
		</div>
	{/snippet}
</Story>

<Story name="OpenTextSubstring">
	{#snippet template()}
		<div class="p-12 bg-[var(--surface-bg)] min-h-[520px]">
			<AddRulePopover
				initialRule={RULE_TEXT_SUB}
				triggerLabel="Edit rule: text_substring system-reminder"
			>
				{#snippet trigger({ props })}
					<Button {...props} variant="ghost" size="icon" class="size-7" aria-label="Edit this rule">
						<Pencil class="size-3.5" aria-hidden="true" />
					</Button>
				{/snippet}
			</AddRulePopover>
		</div>
	{/snippet}
</Story>

<Story name="OpenXmlTag">
	{#snippet template()}
		<div class="p-12 bg-[var(--surface-bg)] min-h-[520px]">
			<AddRulePopover
				initialRule={RULE_XML_REPLACE}
				triggerLabel="Edit rule: xml_tag system-reminder"
			>
				{#snippet trigger({ props })}
					<Button {...props} variant="ghost" size="icon" class="size-7" aria-label="Edit this rule">
						<Pencil class="size-3.5" aria-hidden="true" />
					</Button>
				{/snippet}
			</AddRulePopover>
		</div>
	{/snippet}
</Story>

<Story
	name="TriggerOpensPopover"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const body = within(document.body);

		const triggerBtn = canvas.getByRole('button', { name: 'Edit this rule' });
		await expect(body.queryByText('Edit filter rule')).toBeNull();

		await userEvent.click(triggerBtn);

		await expect(body.getByText('Edit filter rule')).toBeInTheDocument();
		await expect(body.getByRole('button', { name: 'Cancel' })).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="p-12 bg-[var(--bg)] min-h-[520px]">
			<AddRulePopover
				initialRule={RULE_TEXT_SUB}
				triggerLabel="Edit rule: text_substring system-reminder"
			>
				{#snippet trigger({ props })}
					<Button {...props} variant="ghost" size="icon" class="size-7" aria-label="Edit this rule">
						<Pencil class="size-3.5" aria-hidden="true" />
					</Button>
				{/snippet}
			</AddRulePopover>
		</div>
	{/snippet}
</Story>

<Story
	name="CancelClosesPopover"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const body = within(document.body);

		const triggerBtn = canvas.getByRole('button', { name: 'Edit this rule' });
		await userEvent.click(triggerBtn);

		await expect(body.getByText('Edit filter rule')).toBeInTheDocument();

		const cancelBtn = body.getByRole('button', { name: 'Cancel' });
		await userEvent.click(cancelBtn);

		await waitFor(() => expect(body.queryByText('Edit filter rule')).toBeNull());
	}}
>
	{#snippet template()}
		<div class="p-12 bg-[var(--bg)] min-h-[520px]">
			<AddRulePopover
				initialRule={RULE_TEXT_SUB}
				triggerLabel="Edit rule: text_substring system-reminder"
			>
				{#snippet trigger({ props })}
					<Button {...props} variant="ghost" size="icon" class="size-7" aria-label="Edit this rule">
						<Pencil class="size-3.5" aria-hidden="true" />
					</Button>
				{/snippet}
			</AddRulePopover>
		</div>
	{/snippet}
</Story>

<Story
	name="SubmitFiresOnComplete"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const body = within(document.body);

		const triggerBtn = canvas.getByRole('button', { name: 'Edit this rule' });
		await userEvent.click(triggerBtn);

		await expect(body.getByText('Edit filter rule')).toBeInTheDocument();

		const textarea = body.getByRole('textbox', { name: /value/i });
		await fireEvent.input(textarea, { target: { value: 'system-reminder' } });

		const saveBtn = body.getByRole('button', { name: 'Save rule' });
		await waitFor(
			() => {
				expect(textarea).toHaveValue('system-reminder');
				expect(saveBtn).toBeEnabled();
			},
			{ timeout: 5000 }
		);

		await userEvent.click(saveBtn);

		await expect(body.getByRole('alert')).toBeInTheDocument();
		await expect(body.getByText(/store not mounted/i)).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="p-12 bg-[var(--bg)] min-h-[520px]">
			<AddRulePopover
				initialRule={RULE_TEXT_SUB}
				triggerLabel="Edit rule: text_substring system-reminder"
			>
				{#snippet trigger({ props })}
					<Button {...props} variant="ghost" size="icon" class="size-7" aria-label="Edit this rule">
						<Pencil class="size-3.5" aria-hidden="true" />
					</Button>
				{/snippet}
			</AddRulePopover>
		</div>
	{/snippet}
</Story>
