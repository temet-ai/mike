<script lang="ts">
	import { onMount, tick, getContext } from 'svelte';
	import { resolve } from '$app/paths';
	import type { Diagnostic, DiagnosticCategory } from '$lib/types';
	import { CATEGORY_META, checkLabel } from '$lib/diagnostics';
	import SectionCard from './SectionCard.svelte';
	import Tooltip from '../shared/Tooltip.svelte';
	import { ArrowRight } from '@lucide/svelte';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';

	interface CategoryHelp {
		text: string;
		slug: string;
	}
	const CATEGORY_HELP: Record<DiagnosticCategory, CategoryHelp> = {
		justifying_shortcuts: {
			text: 'Language framing scope cuts as virtuous (`pragmatically`, `for now`).',
			slug: 'cat-justifying-shortcuts'
		},
		deflecting_blame: {
			text: 'Language attributing problems elsewhere (`pre-existing`, `it appears`).',
			slug: 'cat-deflecting-blame'
		},
		avoiding_work: {
			text: 'Language punting effort (`straightforward`, `revisit later`).',
			slug: 'cat-avoiding-work'
		},
		faking_understanding: {
			text: 'Language claiming convention without evidence (`consistent with`).',
			slug: 'cat-faking-understanding'
		},
		overselling: {
			text: 'Language overstating quality (`robust`, `production-ready`).',
			slug: 'cat-overselling'
		}
	};

	interface Props {
		diagnostics: Diagnostic[];
		requestId?: number | null;
	}

	let { diagnostics, requestId = null }: Props = $props();

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);

	const openKey = $derived(`request:${requestId}:diagnostics-open`);

	let expandedLocal = $state(false);

	const expanded = $derived(
		collapseStore && requestId !== null ? collapseStore.isExpanded(openKey) : expandedLocal
	);
	let rootEl = $state<HTMLElement | null>(null);

	onMount(() => {
		if (typeof window === 'undefined') return;
		if (window.location.hash !== '#diagnostics') return;
		if (diagnostics.length === 0) return;
		if (collapseStore && requestId !== null) {
			if (!collapseStore.isExpanded(openKey)) collapseStore.toggle(openKey);
		} else {
			expandedLocal = true;
		}
		void tick().then(() => {
			rootEl?.scrollIntoView({ block: 'start', behavior: 'smooth' });
		});
	});

	const count = $derived(diagnostics.length);
	const countLabel = $derived(
		count === 0 ? 'No findings' : `${count} ${count === 1 ? 'finding' : 'findings'}`
	);

	function toggle(): void {
		if (collapseStore && requestId !== null) collapseStore.toggle(openKey);
		else expandedLocal = !expandedLocal;
	}

	function handleKey(event: KeyboardEvent): void {
		if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			toggle();
		}
	}
</script>

<article id="diagnostics" bind:this={rootEl} data-slot="diagnostics-card" aria-label="Diagnostics">
	<SectionCard title="Diagnostics">
		{#if count === 0}
			<div class="flex items-center justify-between">
				<span class="flex items-center gap-1.5 text-sm text-[var(--text-strong)]">
					<span>{countLabel}</span>
				</span>
			</div>
		{:else}
			<div class="flex items-center justify-between">
				<div
					role="button"
					tabindex="0"
					aria-expanded={expanded}
					aria-label={expanded ? 'Collapse diagnostics list' : 'Expand diagnostics list'}
					class="flex flex-1 cursor-pointer items-center gap-1.5 rounded-[2px] outline-none focus-visible:ring-1 focus-visible:ring-[var(--interactive-focus-ring)]"
					onclick={toggle}
					onkeydown={handleKey}
				>
					<span class="text-sm text-[var(--text-strong)]">{countLabel}</span>
					<span class="ml-auto inline-flex items-center">
						<DisclosureMarker open={expanded} size={12} />
					</span>
				</div>
			</div>
		{/if}

		{#if expanded && count > 0}
			<ul class="mt-4 flex flex-col gap-3">
				{#each diagnostics as diagnostic, idx (idx)}
					{@const meta = CATEGORY_META[diagnostic.category]}
					{@const help = CATEGORY_HELP[diagnostic.category]}
					<li class="rounded-[2px] border border-[var(--border-subtle)] bg-[var(--surface-bg)] p-3">
						<div class="flex flex-wrap items-center gap-2">
							<Tooltip text={help.text} slug={help.slug}>
								<span class="{meta.badgeClass} text-label" data-slot="diagnostics-category-badge">
									{meta.label}
								</span>
							</Tooltip>
							<Tooltip text="The check that flagged this finding.">
								<span
									class="font-mono text-label text-[var(--text-3)]"
									data-slot="diagnostics-analyzer-name"
								>
									{checkLabel(diagnostic.analyzer)}
								</span>
							</Tooltip>
							{#if diagnostic.other_hits > 0}
								<Tooltip text="Additional matches dedup'd into this finding.">
									<span
										class="inline-block rounded-[2px] border border-[var(--border-subtle)] bg-[var(--surface-2)] px-2 py-0.5 text-caption font-medium text-[var(--text-2)]"
									>
										+{diagnostic.other_hits} more
									</span>
								</Tooltip>
							{/if}
						</div>
						<Tooltip
							text="Operator-supplied guidance for this category — configurable under Settings → Flagging."
						>
							<p class="mt-2 text-xs italic text-[var(--text-2)]" data-slot="diagnostics-advice">
								{diagnostic.advice}
							</p>
						</Tooltip>
						<code
							class="mt-2 block whitespace-pre-wrap break-all rounded-[2px] bg-[var(--surface-bg)] px-2 py-1.5 font-mono text-label text-[var(--text-strong)]"
							>{diagnostic.matched_text}</code
						>
					</li>
				{/each}
			</ul>
			<div class="mt-4 border-t border-[var(--border-subtle)] pt-3 text-right">
				<a
					href={resolve('/analysis')}
					class="inline-flex items-center gap-1 text-xs text-[var(--status-info-fg)] hover:underline"
				>
					View flagged responses
					<ArrowRight class="size-3 text-[var(--status-info-fg)]" aria-hidden="true" />
				</a>
			</div>
		{/if}
	</SectionCard>
</article>
