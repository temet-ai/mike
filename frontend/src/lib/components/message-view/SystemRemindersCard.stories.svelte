<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import SystemRemindersCard from './SystemRemindersCard.svelte';
	import 'augmented-ui/augmented-ui.min.css';
	import ReminderSubgroup from './ReminderSubgroup.svelte';
	import ReminderRow from './ReminderRow.svelte';
	import SubRow from './SubRow.svelte';
	import Tag from './Tag.svelte';
	import { Webhook, Cpu, BookOpen, Zap } from '@lucide/svelte';

	const { Story } = defineMeta({
		title: 'MessageView/SystemRemindersCard',
		component: SystemRemindersCard,
		tags: ['autodocs'],
		parameters: {
			docs: {
				description: {
					component:
						'Outer `.reminders` card for the system-reminder block of a user turn. Shows a sticky header (bell icon, "System reminders", group count + token total) and renders subgroups/rows via the default snippet.'
				}
			}
		}
	});
</script>

<Story name="Zero groups (empty)">
	{#snippet template(args)}
		<div class="p-6 bg-[var(--surface-bg)] max-w-[720px]">
			<SystemRemindersCard groupCount={0} totalChars={0} totalTokens={0}>
				{#snippet children()}
					<p class="text-[0.8125rem] text-[var(--text-3)]">No reminders.</p>
				{/snippet}
			</SystemRemindersCard>
		</div>
	{/snippet}
</Story>

<Story name="Few groups (hooks + MCP)">
	{#snippet template(args)}
		<div class="p-6 bg-[var(--surface-bg)] max-w-[720px]">
			<SystemRemindersCard groupCount={2} totalChars={840} totalTokens={210}>
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
				<ReminderSubgroup open={false} ariaLabel="MCP group" onToggle={() => {}}>
					{#snippet head()}
						<Cpu size={12} aria-hidden="true" />
						<Tag variant="boot">MCP</Tag>
						<span class="text-[0.8125rem] text-[var(--text-2)]">Servers &amp; tools</span>
						<span class="ml-auto text-[0.75rem] text-[var(--text-3)]">1 block · ~168 tokens</span>
					{/snippet}
					<SubRow expandable={true} open={false} ariaLabel="mike server">
						<span class="text-[0.8125rem] text-[var(--text-1)]">mike</span>
					</SubRow>
				</ReminderSubgroup>
			</SystemRemindersCard>
		</div>
	{/snippet}
</Story>

<Story name="Many groups (mixed kinds)">
	{#snippet template(args)}
		<div class="p-6 bg-[var(--surface-bg)] max-w-[720px]">
			<SystemRemindersCard groupCount={4} totalChars={12480} totalTokens={3120}>
				<ReminderSubgroup open={true} ariaLabel="Hooks group" onToggle={() => {}}>
					{#snippet head()}
						<Webhook size={12} aria-hidden="true" />
						<Tag variant="group">Hooks</Tag>
						<span class="text-[0.8125rem] text-[var(--text-2)]">Lifecycle events fired</span>
						<span class="ml-auto text-[0.75rem] text-[var(--text-3)]">3 blocks · ~58 tokens</span>
					{/snippet}
					<SubRow expandable={true} open={false} ariaLabel="SessionStart hook" fired={true}>
						<span class="text-[0.8125rem] text-[var(--text-1)]">SessionStart hook</span>
					</SubRow>
					<SubRow expandable={true} open={false} ariaLabel="UserPromptSubmit hook" fired={true}>
						<span class="text-[0.8125rem] text-[var(--text-1)]">UserPromptSubmit hook</span>
					</SubRow>
					<SubRow expandable={true} open={false} ariaLabel="PostToolUse hook">
						<span class="text-[0.8125rem] text-[var(--text-3)]">PostToolUse hook (not fired)</span>
					</SubRow>
				</ReminderSubgroup>
				<ReminderSubgroup open={false} ariaLabel="MCP group" onToggle={() => {}}>
					{#snippet head()}
						<Cpu size={12} aria-hidden="true" />
						<Tag variant="boot">MCP</Tag>
						<span class="text-[0.8125rem] text-[var(--text-2)]">Servers &amp; tools</span>
						<span class="ml-auto text-[0.75rem] text-[var(--text-3)]">1 block · ~217 tokens</span>
					{/snippet}
					<SubRow muted={true}>
						<span class="text-[0.8125rem]">mike (no instructions)</span>
					</SubRow>
				</ReminderSubgroup>
				<ReminderRow>
					<span class="inline-flex items-center gap-1.5 text-[0.8125rem] text-[var(--text-1)]">
						<Zap size={12} aria-hidden="true" class="text-[var(--text-3)]" />
						<Tag variant="group">Tools</Tag>
						<span class="text-[var(--text-2)]">Available tool schemas</span>
						<span class="ml-auto text-[0.75rem] text-[var(--text-3)]">2 blocks · ~891 tokens</span>
					</span>
				</ReminderRow>
				<ReminderSubgroup open={false} ariaLabel="Context group" onToggle={() => {}}>
					{#snippet head()}
						<BookOpen size={12} aria-hidden="true" />
						<Tag variant="group">Context</Tag>
						<span class="text-[0.8125rem] text-[var(--text-2)]">claudeMd</span>
						<span class="ml-auto text-[0.75rem] text-[var(--text-3)]">5 files · ~1.9k tokens</span>
					{/snippet}
					<SubRow expandable={true} open={false} ariaLabel="CLAUDE.md global">
						<span class="text-[0.8125rem] text-[var(--text-1)]">~/.claude/CLAUDE.md</span>
					</SubRow>
					<SubRow expandable={true} open={false} ariaLabel="rules global">
						<span class="text-[0.8125rem] text-[var(--text-1)]"
							>~/.claude/rules/agent-memory-hygiene.md</span
						>
					</SubRow>
				</ReminderSubgroup>
			</SystemRemindersCard>
		</div>
	{/snippet}
</Story>
