<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import ClassChip from './ClassChip.svelte';

	function buildChip(overrides: { label: string; isActive?: boolean; href?: string }): {
		label: string;
		isActive?: boolean;
		href?: string;
	} {
		return {
			label: overrides.label,
			isActive: overrides.isActive ?? false,
			href: overrides.href
		};
	}

	const DEFAULT_CHIP = buildChip({ label: 'main-agent' });

	const ACTIVE_CHIP = buildChip({ label: 'main-agent', isActive: true });

	const LINK_CHIP = buildChip({
		label: 'frontend-developer',
		href: '/prompts/abc123def456'
	});

	const ACTIVE_LINK_CHIP = buildChip({
		label: 'rust-developer',
		isActive: true,
		href: '/prompts/abc123def456'
	});

	const LONG_LABEL_CHIP = buildChip({ label: 'very-long-subagent-persona-identifier' });

	const { Story } = defineMeta({
		title: 'Routes/Prompts/ClassChip',
		component: ClassChip,
		tags: ['autodocs']
	});
</script>

<Story name="Default" args={DEFAULT_CHIP}>
	{#snippet template(args)}
		<div class="flex flex-wrap items-center gap-2 p-4 bg-[var(--surface-card)]">
			<ClassChip {...args} />
		</div>
	{/snippet}
</Story>

<Story name="Active" args={ACTIVE_CHIP}>
	{#snippet template(args)}
		<div class="flex flex-wrap items-center gap-2 p-4 bg-[var(--surface-card)]">
			<ClassChip {...args} />
		</div>
	{/snippet}
</Story>

<Story name="LinkChip" args={LINK_CHIP}>
	{#snippet template(args)}
		<div class="flex flex-wrap items-center gap-2 p-4 bg-[var(--surface-card)]">
			<ClassChip {...args} />
		</div>
	{/snippet}
</Story>

<Story name="ActiveLink" args={ACTIVE_LINK_CHIP}>
	{#snippet template(args)}
		<div class="flex flex-wrap items-center gap-2 p-4 bg-[var(--surface-card)]">
			<ClassChip {...args} />
		</div>
	{/snippet}
</Story>

<Story name="LongLabel" args={LONG_LABEL_CHIP}>
	{#snippet template(args)}
		<div class="flex flex-wrap items-center gap-2 p-4 bg-[var(--surface-card)]">
			<ClassChip {...args} />
		</div>
	{/snippet}
</Story>
