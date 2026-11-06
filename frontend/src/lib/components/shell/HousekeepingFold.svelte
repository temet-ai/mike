<script lang="ts">
	import { Bot, Gauge, Activity, Hash, Globe, Lightbulb, Circle } from '@lucide/svelte';
	import type { Component } from 'svelte';
	import type { HousekeepingItemView } from './timelineTurnView';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';

	interface Props {
		anchorId: number;
		items: HousekeepingItemView[];
		open: boolean;
		onToggle: () => void;
		onOpenRequest: (id: number) => void;
	}

	let { anchorId, items, open, onToggle, onOpenRequest }: Props = $props();

	const LABEL_ICON: Record<string, Component> = {
		'quota probe': Gauge,
		'title sidecar': Bot,
		advisor: Lightbulb,
		monitor: Activity,
		'count tokens': Hash,
		'web fetch': Globe
	};
	function iconFor(item: HousekeepingItemView): Component {
		return LABEL_ICON[item.label] ?? (Circle as unknown as Component);
	}

	const distinctLabels = $derived.by(() => {
		const counts = new Map<string, number>();
		for (const it of items) counts.set(it.label, (counts.get(it.label) ?? 0) + 1);
		const parts: string[] = [];
		for (const [label, n] of counts) parts.push(n > 1 ? `${label} ×${n}` : label);
		return parts.join(', ');
	});

	const range = $derived.by(() => {
		const first = items[0];
		const last = items[items.length - 1];
		if (!first || !last) return '';
		const a = first.requestNumber ?? first.id;
		const b = last.requestNumber ?? last.id;
		return a === b ? `#${a}` : `#${a}–#${b}`;
	});
</script>

<div class="my-1.5" data-slot="housekeeping-fold" data-band-id={anchorId}>
	<button
		type="button"
		class="flex w-full items-center gap-2.5 border-y border-dashed border-[var(--border-subtle)] px-2.5 py-1.5 text-left text-[var(--text-faint)] hover:text-[var(--text-2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--interactive-focus-ring)]"
		aria-expanded={open}
		aria-label={`${open ? 'Collapse' : 'Expand'} ${items.length} housekeeping requests`}
		onclick={onToggle}
	>
		<DisclosureMarker {open} size={12} />
		<span class="text-label"
			>{items.length} housekeeping request{items.length === 1 ? '' : 's'} — {distinctLabels}</span
		>
		<span class="flex-1"></span>
		<span class="font-mono text-micro tabular-nums text-[var(--text-faint)]">{range}</span>
	</button>
	{#if open}
		<div class="flex flex-col gap-1 py-1.5 pl-8">
			{#each items as item (item.id)}
				{@const ItemIcon = iconFor(item)}
				<button
					type="button"
					class="flex w-fit items-center gap-2 text-left text-label text-[var(--text-3)] hover:text-[var(--text-strong)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--interactive-focus-ring)]"
					onclick={() => onOpenRequest(item.id)}
				>
					<span class="font-mono tabular-nums text-[var(--text-faint)]" style="width: 40px;"
						>#{item.requestNumber ?? item.id}</span
					>
					<ItemIcon size={11} class="text-[var(--text-faint)]" aria-hidden="true" />
					<span>{item.label}</span>
				</button>
			{/each}
		</div>
	{/if}
</div>
