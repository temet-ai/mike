<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import SubRowDetail from './SubRowDetail.svelte';

	const { Story } = defineMeta({
		title: 'MessageView/SubRowDetail',
		component: SubRowDetail,
		tags: ['autodocs'],
		parameters: {
			docs: {
				description: {
					component:
						'Detail expansion panel for an expandable SubRow. Provides the `.detail` chrome (region role, amber left-edge accent on hover/focus). Callers render sections via the default snippet using `.detail__section` + `.detail__label`.'
				}
			}
		}
	});
</script>

<Story name="With KV body">
	{#snippet template(args)}
		<div class="p-6 bg-[var(--surface-bg)] max-w-[600px]">
			<SubRowDetail label="SessionStart hook details">
				<div class="detail__section">
					<span class="detail__label">Hook name</span>
					<span>SessionStart</span>
				</div>
				<div class="detail__section">
					<span class="detail__label">Status</span>
					<span>startup hook success: OK</span>
				</div>
				<div class="detail__section">
					<span class="detail__label">Exit code</span>
					<span>0</span>
				</div>
			</SubRowDetail>
		</div>
	{/snippet}
</Story>

<Story name="With long text body (no-truncation)">
	{#snippet template(args)}
		<div class="p-6 bg-[var(--surface-bg)] max-w-[600px]">
			<SubRowDetail label="claudeMd context — full file body">
				<div class="detail__section">
					<span class="detail__label">Path</span>
					<span>~/.claude/rules/agent-memory-hygiene.md</span>
				</div>
				<div class="detail__section">
					<span class="detail__label">Body</span>
					<pre class="text-[0.75rem] text-[var(--text-1)] whitespace-pre-wrap break-words m-0">
# Agent Memory Hygiene

When you have a persistent memory directory, follow these rules strictly.

## What to save

- Build/test/lint commands that you discovered or confirmed work
- Non-obvious patterns: architectural decisions, naming conventions, error handling patterns
- Debugging solutions: root causes and fixes for tricky problems you solved
- Key file locations: important modules, configs, entry points you had to search for
- User corrections: when the user corrects your approach, save the correct way

## What NOT to save

- Obvious facts ("this is a Python project", "uses git")
- Information already in CLAUDE.md or project rules (check first, don't duplicate)
- Temporary context that won't matter next session
					</pre>
				</div>
			</SubRowDetail>
		</div>
	{/snippet}
</Story>

<Story name="Without body (label only)">
	{#snippet template(args)}
		<div class="p-6 bg-[var(--surface-bg)] max-w-[600px]">
			<SubRowDetail label="Empty detail region" />
		</div>
	{/snippet}
</Story>
