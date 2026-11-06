<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import PinToggle from './PinToggle.svelte';

	interface PinToggleProps {
		pinned: boolean;
		onToggle: () => void;
		ariaLabel?: string;
		size?: number;
	}

	function buildPinToggleProps(overrides: Partial<PinToggleProps> = {}): PinToggleProps {
		return {
			pinned: false,
			onToggle: () => {},
			size: 12,
			...overrides
		};
	}

	const { Story } = defineMeta({
		title: 'Shared/PinToggle',
		component: PinToggle,
		tags: ['autodocs'],
		argTypes: {
			pinned: { control: 'boolean' },
			size: { control: { type: 'number' } }
		}
	});
</script>

<Story name="Unpinned" args={buildPinToggleProps({ pinned: false })}>
	{#snippet template(args)}
		<div class="flex items-center gap-3 p-4">
			<span class="text-xs text-[var(--text-2)]">Session row (unpinned):</span>
			<PinToggle {...args} />
		</div>
	{/snippet}
</Story>

<Story name="Pinned" args={buildPinToggleProps({ pinned: true })}>
	{#snippet template(args)}
		<div class="flex items-center gap-3 p-4">
			<span class="text-xs text-[var(--text-2)]">Session row (pinned):</span>
			<PinToggle {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="CustomAriaLabel"
	args={buildPinToggleProps({ pinned: false, ariaLabel: 'Pin this agent run' })}
>
	{#snippet template(args)}
		<div class="flex items-center gap-3 p-4">
			<span class="text-xs text-[var(--text-2)]">Custom aria-label:</span>
			<PinToggle {...args} />
		</div>
	{/snippet}
</Story>

<Story name="LargeSize" args={buildPinToggleProps({ pinned: false, size: 16 })}>
	{#snippet template(args)}
		<div class="flex items-center gap-3 p-4">
			<span class="text-xs text-[var(--text-2)]">16px icon (larger surface):</span>
			<PinToggle {...args} />
		</div>
	{/snippet}
</Story>
