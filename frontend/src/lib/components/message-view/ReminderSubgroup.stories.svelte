<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import ReminderSubgroup from './ReminderSubgroup.svelte';
	import SubRow from './SubRow.svelte';
	import Tag from './Tag.svelte';
	import { Webhook, Cpu } from '@lucide/svelte';

	const { Story } = defineMeta({
		title: 'MessageView/ReminderSubgroup',
		component: ReminderSubgroup,
		tags: ['autodocs'],
		parameters: {
			docs: {
				description: {
					component:
						'Collapsible category container inside `.reminders__body`. Used for Hooks / MCP / Context (Global, Project). The `.subgroup__head` is sticky and stacks below the outer reminders head.'
				}
			}
		}
	});
</script>

<Story name="Hooks (expanded)">
	{#snippet template(args)}
		<div class="p-6 bg-[var(--surface-bg)] max-w-[600px]">
			<ReminderSubgroup open={true} ariaLabel="Hooks group" onToggle={() => {}}>
				{#snippet head()}
					<Webhook size={12} aria-hidden="true" />
					<Tag variant="group">Hooks</Tag>
					<span class="text-[0.8125rem] text-[var(--text-2)]">Lifecycle events fired</span>
					<span class="ml-auto text-[0.75rem] text-[var(--text-3)]">2 blocks · ~42 tokens</span>
				{/snippet}
				<SubRow expandable={true} open={false} ariaLabel="SessionStart hook">
					<span class="text-[0.8125rem] text-[var(--text-1)]">SessionStart hook</span>
				</SubRow>
				<SubRow expandable={true} open={false} ariaLabel="UserPromptSubmit hook">
					<span class="text-[0.8125rem] text-[var(--text-1)]">UserPromptSubmit hook</span>
				</SubRow>
			</ReminderSubgroup>
		</div>
	{/snippet}
</Story>

<Story name="MCP (collapsed)">
	{#snippet template(args)}
		<div class="p-6 bg-[var(--surface-bg)] max-w-[600px]">
			<ReminderSubgroup open={false} ariaLabel="MCP group" onToggle={() => {}}>
				{#snippet head()}
					<Cpu size={12} aria-hidden="true" />
					<Tag variant="boot">MCP</Tag>
					<span class="text-[0.8125rem] text-[var(--text-2)]">Servers &amp; tools</span>
					<span class="ml-auto text-[0.75rem] text-[var(--text-3)]">1 block · ~217 tokens</span>
				{/snippet}
				<SubRow expandable={true} open={false} ariaLabel="mike server">
					<span class="text-[0.8125rem] text-[var(--text-1)]">mike</span>
				</SubRow>
			</ReminderSubgroup>
		</div>
	{/snippet}
</Story>

<Story name="Large group (many subrows, expanded)">
	{#snippet template(args)}
		<div class="p-6 bg-[var(--surface-bg)] max-w-[600px]">
			<ReminderSubgroup open={true} ariaLabel="Context group" onToggle={() => {}}>
				{#snippet head()}
					<Tag variant="group">Context</Tag>
					<span class="text-[0.8125rem] text-[var(--text-2)]">claudeMd</span>
					<span class="ml-auto text-[0.75rem] text-[var(--text-3)]">6 files · ~3.2k tokens</span>
				{/snippet}
				<SubRow expandable={true} open={false} ariaLabel="CLAUDE.md global">
					<span class="text-[0.8125rem] text-[var(--text-1)]">~/.claude/CLAUDE.md</span>
				</SubRow>
				<SubRow expandable={true} open={false} ariaLabel="agent-memory-hygiene.md">
					<span class="text-[0.8125rem] text-[var(--text-1)]"
						>~/.claude/rules/agent-memory-hygiene.md</span
					>
				</SubRow>
				<SubRow expandable={true} open={false} ariaLabel="scope-and-agents.md">
					<span class="text-[0.8125rem] text-[var(--text-1)]"
						>~/.claude/rules/scope-and-agents.md</span
					>
				</SubRow>
				<SubRow expandable={true} open={false} ariaLabel="CLAUDE.md project">
					<span class="text-[0.8125rem] text-[var(--text-1)]"
						>~/projects/ai/temet-proxy/CLAUDE.md</span
					>
				</SubRow>
				<SubRow expandable={true} open={false} ariaLabel="testing.md">
					<span class="text-[0.8125rem] text-[var(--text-1)]"
						>~/projects/ai/temet-proxy/.claude/rules/testing.md</span
					>
				</SubRow>
				<SubRow expandable={true} open={false} ariaLabel="MEMORY.md">
					<span class="text-[0.8125rem] text-[var(--text-3)]"
						>~/projects/ai/temet-proxy/.claude/memory/MEMORY.md</span
					>
				</SubRow>
			</ReminderSubgroup>
		</div>
	{/snippet}
</Story>
