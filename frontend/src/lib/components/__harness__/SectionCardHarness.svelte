<script lang="ts">
	import type { Snippet } from 'svelte';
	import { setContext } from 'svelte';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import SectionCard, { type SectionCardAccent } from '../detail/SectionCard.svelte';

	interface Props {
		collapseStore: CollapseStore;
		title: string;
		ariaLabel?: string;
		children: Snippet;
		headerActions?: Snippet;
		collapsible?: boolean;
		sectionKey?: string;
		defaultCollapsed?: boolean;
		bordered?: boolean;
		expandable?: boolean;
		titleTooltip?: string;
		description?: string;
		accent?: SectionCardAccent;
	}

	let {
		collapseStore,
		title,
		ariaLabel,
		children,
		headerActions,
		collapsible = false,
		sectionKey = '',
		defaultCollapsed = false,
		bordered = true,
		expandable = true,
		titleTooltip,
		description,
		accent
	}: Props = $props();

	// svelte-ignore state_referenced_locally
	setContext(COLLAPSE_STORE_KEY, collapseStore);
</script>

{#snippet sectionChildren()}
	{@render children()}
{/snippet}
{#snippet sectionHeaderActions()}
	{#if headerActions}{@render headerActions()}{/if}
{/snippet}

<SectionCard
	{title}
	{ariaLabel}
	{collapsible}
	{sectionKey}
	{defaultCollapsed}
	{bordered}
	{expandable}
	{titleTooltip}
	{description}
	{accent}
	headerActions={headerActions ? sectionHeaderActions : undefined}
>
	{@render sectionChildren()}
</SectionCard>
