<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import ReminderRow from './ReminderRow.svelte';
	import Tag from './Tag.svelte';
	import { Zap, BookOpen, Settings } from '@lucide/svelte';

	const { Story } = defineMeta({
		title: 'MessageView/ReminderRow',
		component: ReminderRow,
		tags: ['autodocs'],
		parameters: {
			docs: {
				description: {
					component:
						'Flat leaf entry inside `.reminders__body`. Non-interactive `<div>` — no chevron or toggle. The `.chev-spacer` preserves vertical alignment with sibling subgroup rows. Used for leaf categories like Tools and Skills.'
				}
			}
		}
	});
</script>

<Story name="Tools row">
	{#snippet template(args)}
		<div class="p-6 bg-[var(--surface-bg)] max-w-[600px]">
			<ReminderRow>
				<span class="inline-flex items-center gap-1.5 text-[0.8125rem] text-[var(--text-1)]">
					<Zap size={12} aria-hidden="true" class="text-[var(--text-3)]" />
					<Tag variant="group">Tools</Tag>
					<span class="text-[var(--text-2)]">Available tool schemas</span>
					<span class="ml-auto text-[0.75rem] text-[var(--text-3)] tabular-nums"
						>3 blocks · ~142 tokens</span
					>
				</span>
			</ReminderRow>
		</div>
	{/snippet}
</Story>

<Story name="Skills row">
	{#snippet template(args)}
		<div class="p-6 bg-[var(--surface-bg)] max-w-[600px]">
			<ReminderRow>
				<span class="inline-flex items-center gap-1.5 text-[0.8125rem] text-[var(--text-1)]">
					<BookOpen size={12} aria-hidden="true" class="text-[var(--text-3)]" />
					<Tag variant="group">Skills</Tag>
					<span class="text-[var(--text-2)]">Available skills</span>
					<span class="ml-auto text-[0.75rem] text-[var(--text-3)] tabular-nums"
						>1 block · ~891 tokens</span
					>
				</span>
			</ReminderRow>
		</div>
	{/snippet}
</Story>

<Story name="Generic leaf row">
	{#snippet template(args)}
		<div class="p-6 bg-[var(--surface-bg)] max-w-[600px]">
			<ReminderRow>
				<span class="inline-flex items-center gap-1.5 text-[0.8125rem] text-[var(--text-1)]">
					<Settings size={12} aria-hidden="true" class="text-[var(--text-3)]" />
					<span class="text-[var(--text-2)]">Uncategorised leaf content</span>
				</span>
			</ReminderRow>
		</div>
	{/snippet}
</Story>
