<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import CopyIconButton from './CopyIconButton.svelte';

	interface CopyProps {
		getText: () => string | null;
		label?: string;
		copiedLabel?: string;
		size?: number;
	}

	function buildCopyProps(overrides: Partial<CopyProps> = {}): CopyProps {
		return {
			getText: () => 'ANTHROPIC_BASE_URL=http://localhost:9988 claude',
			label: 'Copy',
			copiedLabel: 'Copied',
			size: 12,
			...overrides
		};
	}

	const { Story } = defineMeta({
		title: 'Shared/CopyIconButton',
		component: CopyIconButton,
		tags: ['autodocs'],
		argTypes: {
			size: { control: { type: 'number' } }
		}
	});
</script>

<Story name="Idle" args={buildCopyProps()}>
	{#snippet template(args)}
		<div class="flex items-center gap-2 p-4">
			<span class="text-xs text-[var(--text-2)]">Copy a shell command:</span>
			<CopyIconButton {...args} />
		</div>
	{/snippet}
</Story>

<Story name="LargeSize" args={buildCopyProps({ size: 16, label: 'Copy headers' })}>
	{#snippet template(args)}
		<div class="flex items-center gap-2 p-4">
			<span class="text-xs text-[var(--text-2)]">Copy headers (16px icon):</span>
			<CopyIconButton {...args} />
		</div>
	{/snippet}
</Story>

<Story name="NullText" args={buildCopyProps({ getText: () => null, label: 'Copy (no-op)' })}>
	{#snippet template(args)}
		<div class="flex items-center gap-2 p-4">
			<span class="text-xs text-[var(--text-2)]">getText returns null (click is no-op):</span>
			<CopyIconButton {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="CustomLabels"
	args={buildCopyProps({ label: 'Copy request body', copiedLabel: 'Copied!', size: 14 })}
>
	{#snippet template(args)}
		<div class="flex items-center gap-2 p-4">
			<span class="text-xs text-[var(--text-2)]">Custom aria-labels:</span>
			<CopyIconButton {...args} />
		</div>
	{/snippet}
</Story>

<Story name="ClipboardUnavailable">
	{#snippet template()}
		<div class="p-4">
			<p class="mb-2 text-xs text-[var(--text-2)]">
				Component renders nothing when clipboard API is unavailable (non-secure context):
			</p>
			<CopyIconButton getText={() => 'example'} label="Copy" />
		</div>
	{/snippet}
</Story>
