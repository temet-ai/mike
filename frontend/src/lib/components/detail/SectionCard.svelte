<script lang="ts">
	import type { Snippet } from 'svelte';
	import { getContext } from 'svelte';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import { disclose } from '$lib/components/ui/motion';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { Info } from '@lucide/svelte';

	export type SectionCardAccent = 'info' | 'danger' | 'teal';

	let {
		title,
		titleIcon: TitleIcon,
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
	}: {
		title: string;
		titleIcon?: typeof Info;
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
	} = $props();

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);

	let collapsed = $derived(
		!expandable
			? true
			: collapsible && collapseStore
				? !collapseStore.isExpanded(sectionKey, !defaultCollapsed)
				: defaultCollapsed
	);

	const interactive = $derived(collapsible && expandable);
	const activeAccent = $derived(expandable ? accent : undefined);

	function handleToggle(): void {
		if (!expandable) return;
		collapseStore?.toggle(sectionKey);
	}
</script>

<div
	class="dsc-elev"
	data-elevation={bordered ? 'card' : 'flat'}
	data-elevation-raise={interactive && bordered ? '' : undefined}
	data-motion="hover"
>
	<section
		class="dsc-card {bordered ? '' : 'dsc-card--borderless'} {interactive
			? 'dsc-card--collapsible'
			: 'dsc-card--static'}"
		data-augmented-ui={bordered ? 'tl-clip br-clip border' : undefined}
		data-accent={activeAccent}
		aria-label={ariaLabel ?? title}
	>
		<div class="flex flex-wrap items-center justify-between gap-2">
			{#if interactive}
				<button
					type="button"
					class="flex min-w-0 flex-auto items-center gap-2 text-left"
					data-elevation-trigger
					data-motion="hover"
					aria-expanded={!collapsed}
					onclick={handleToggle}
				>
					<DisclosureMarker open={!collapsed} size={12} />
					{#if TitleIcon}<TitleIcon
							size={13}
							aria-hidden="true"
							class="shrink-0 text-[var(--text-2)]"
						/>{/if}
					<span class="panel-hd">{title}</span>
				</button>
			{:else}
				<h3 class="panel-hd flex min-w-0 flex-auto items-center gap-2">
					{#if TitleIcon}<TitleIcon size={13} aria-hidden="true" class="shrink-0" />{/if}
					{title}
				</h3>
			{/if}
			{#if titleTooltip}
				<Tooltip text={titleTooltip}>
					<Info size={12} aria-hidden="true" class="dsc-title-help" />
				</Tooltip>
			{/if}
			{#if headerActions}
				<div class="flex min-w-0 max-w-full flex-wrap items-center gap-2">
					{@render headerActions()}
				</div>
			{/if}
		</div>
		{#if description}
			<p class="dsc-description">{description}</p>
		{/if}
		{#if interactive}
			{#if !collapsed}
				<div class="dsc-body" transition:disclose>
					<div class="dsc-body__inner">{@render children()}</div>
				</div>
			{/if}
		{:else if expandable}
			<div class="mt-3">{@render children()}</div>
		{/if}
	</section>
</div>

<style>
	.dsc-card {
		--edge: 8px;
		--edge-tl: var(--sec-tl, var(--edge));
		--edge-br: var(--sec-br, var(--edge));
		--aug-tl: var(--edge-tl);
		--aug-br: var(--edge-br);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(135deg, var(--border-strong) 0%, var(--border-default) 60%);
		background: var(--surface-card);
		box-shadow: inset 0 1px 0 var(--surface-shine);
	}

	.dsc-card--borderless {
		--edge-pad: calc(var(--edge) + 4px);
		background: transparent;
		box-shadow: none;
	}

	.dsc-card[data-accent='info'] {
		--aug-border-bg: linear-gradient(
			135deg,
			var(--status-info-fg),
			var(--status-info-border) 35%,
			var(--border-default) 70%
		);
	}
	.dsc-card[data-accent='teal'] {
		--aug-border-bg: linear-gradient(
			135deg,
			var(--data-teal-fg),
			var(--data-teal-border) 35%,
			var(--border-default) 70%
		);
	}
	.dsc-card[data-accent='danger'] {
		--aug-border-bg: linear-gradient(
			135deg,
			var(--status-danger-fg),
			var(--status-danger-border) 35%,
			var(--border-default) 70%
		);
	}

	.dsc-body {
		display: grid;
	}

	.dsc-body__inner {
		min-height: 0;
		overflow: hidden;
		padding-top: var(--space-2);
	}

	.dsc-card--collapsible {
		padding: calc(var(--edge) + 4px);
	}

	.dsc-card--static {
		padding: var(--edge-pad, calc(var(--edge) + 4px));
	}

	:global(.dsc-title-help) {
		color: var(--text-faint);
		flex-shrink: 0;
	}

	.dsc-description {
		margin: 4px 0 0;
		font-size: var(--text-sm, 0.8125rem);
		color: var(--text-2);
		line-height: 1.5;
		overflow-wrap: break-word;
	}
</style>
