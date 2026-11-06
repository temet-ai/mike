<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import SubRow from './SubRow.svelte';

	const { Story } = defineMeta({
		title: 'MessageView/SubRow',
		component: SubRow,
		tags: ['autodocs'],
		parameters: {
			docs: {
				description: {
					component:
						'Individual item inside a `.subgroup__body`. Three variants: non-expandable div, expandable button (with chevron), and muted. The `fired` prop adds a green-halo dot for hook events.'
				}
			}
		}
	});
</script>

<Story name="Default (non-expandable)">
	{#snippet template(args)}
		<div class="p-6 bg-[var(--surface-bg)] max-w-[600px]">
			<SubRow>
				<span class="text-[0.8125rem] text-[var(--text-1)]">
					/Users/alice/.claude/rules/scope-and-agents.md
				</span>
			</SubRow>
		</div>
	{/snippet}
</Story>

<Story name="Expandable collapsed">
	{#snippet template(args)}
		<div class="p-6 bg-[var(--surface-bg)] max-w-[600px]">
			<SubRow expandable={true} open={false} ariaLabel="SessionStart hook — expand for details">
				<span class="text-[0.8125rem] text-[var(--text-1)]"> SessionStart hook </span>
			</SubRow>
		</div>
	{/snippet}
</Story>

<Story name="Expandable open">
	{#snippet template(args)}
		<div class="p-6 bg-[var(--surface-bg)] max-w-[600px]">
			<SubRow expandable={true} open={true} ariaLabel="UserPromptSubmit hook — collapse">
				<span class="text-[0.8125rem] text-[var(--text-1)]"> UserPromptSubmit hook </span>
			</SubRow>
		</div>
	{/snippet}
</Story>

<Story name="Muted">
	{#snippet template(args)}
		<div class="p-6 bg-[var(--surface-bg)] max-w-[600px]">
			<SubRow muted={true}>
				<span class="text-[0.8125rem]"> mike (no instructions provided) </span>
			</SubRow>
		</div>
	{/snippet}
</Story>

<Story name="Fired (hook event)">
	{#snippet template(args)}
		<div class="p-6 bg-[var(--surface-bg)] max-w-[600px]">
			<SubRow
				expandable={true}
				open={false}
				fired={true}
				ariaLabel="PostToolUse hook — fired this turn"
			>
				<span class="text-[0.8125rem] text-[var(--text-1)]"> PostToolUse hook </span>
			</SubRow>
		</div>
	{/snippet}
</Story>

<Story name="Hide chevron">
	{#snippet template(args)}
		<div class="p-6 bg-[var(--surface-bg)] max-w-[600px]">
			<SubRow hideChevron={true}>
				<span class="text-[0.8125rem] text-[var(--text-1)]"> Row without chevron gutter </span>
			</SubRow>
		</div>
	{/snippet}
</Story>

<Story name="Long text (no-truncation tripwire)">
	{#snippet template(args)}
		<div class="p-6 bg-[var(--surface-bg)] max-w-[600px]">
			<SubRow>
				<span class="text-[0.8125rem] text-[var(--text-1)]">
					/Users/alice/projects/ai/temet-proxy/.claude/rules/agent-memory-hygiene.md (user's private
					global instructions for all projects) — Agent Memory Hygiene: When you have a persistent
					memory directory, follow these rules strictly. Save build/test/lint commands that you
					discovered. Save non-obvious patterns: architectural decisions, naming conventions, error
					handling patterns. Save debugging solutions: root causes and fixes for tricky problems you
					solved.
				</span>
			</SubRow>
		</div>
	{/snippet}
</Story>
