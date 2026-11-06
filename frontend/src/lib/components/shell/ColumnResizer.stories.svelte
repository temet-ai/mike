<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import ColumnResizer from './ColumnResizer.svelte';

	interface ResizerFixture {
		width: number;
		min: number;
		max: number;
		ariaLabel: string;
	}

	function buildResizer(overrides: Partial<ResizerFixture> = {}): ResizerFixture {
		return {
			width: 240,
			min: 160,
			max: 480,
			ariaLabel: 'Resize sessions column',
			...overrides
		};
	}

	const { Story } = defineMeta({
		title: 'Shell/ColumnResizer',
		component: ColumnResizer,
		tags: ['autodocs'],
		argTypes: {
			width: { control: 'number' },
			min: { control: 'number' },
			max: { control: 'number' },
			ariaLabel: { control: 'text' }
		}
	});
</script>

<Story name="Default">
	{#snippet template()}
		<div
			class="relative w-[240px] h-20 bg-[var(--surface-card)] border border-[var(--border-subtle)]"
		>
			<ColumnResizer {...buildResizer()} onResize={() => {}} />
			<span class="block text-[11px] text-[var(--text-3)] p-2">Sessions column (240px)</span>
		</div>
	{/snippet}
</Story>

<Story name="AtMinWidth">
	{#snippet template()}
		<div
			class="relative w-[160px] h-20 bg-[var(--surface-card)] border border-[var(--border-subtle)]"
		>
			<ColumnResizer {...buildResizer({ width: 160 })} onResize={() => {}} />
			<span class="block text-[11px] text-[var(--text-3)] p-2">Min width (160px)</span>
		</div>
	{/snippet}
</Story>

<Story name="AtMaxWidth">
	{#snippet template()}
		<div
			class="relative w-[480px] h-20 bg-[var(--surface-card)] border border-[var(--border-subtle)]"
		>
			<ColumnResizer {...buildResizer({ width: 480 })} onResize={() => {}} />
			<span class="block text-[11px] text-[var(--text-3)] p-2">Max width (480px)</span>
		</div>
	{/snippet}
</Story>
