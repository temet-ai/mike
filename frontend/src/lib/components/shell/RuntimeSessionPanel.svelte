<script lang="ts">
	import { untrack } from 'svelte';
	import { getSession } from '$lib/api';
	import { invalidateApiCache } from '$lib/stores/cache/apiCache.svelte';
	import type { RuntimeItem, RuntimeItemPage, RuntimeSessionObservation } from '$lib/types';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import { Button } from '$lib/components/ui/button';

	interface Props {
		sessionId: string;
		observation: RuntimeSessionObservation;
		initialItems: RuntimeItemPage | null;
	}

	let { sessionId, observation, initialItems }: Props = $props();
	let loadedSession = $state<string | null>(null);
	let items = $state<RuntimeItemPage | null>(null);
	let refreshed = $state<RuntimeSessionObservation | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let requestedOffset = $state(0);
	const observed = $derived(refreshed ?? observation);

	$effect(() => {
		const id = sessionId;
		const firstPage = initialItems;
		untrack(() => {
			if (loadedSession === id) return;
			loadedSession = id;
			items = firstPage;
			refreshed = null;
			error = null;
			loading = false;
		});
	});

	function value(value: string | number | null | undefined): string {
		return value == null ? 'Unknown' : typeof value === 'number' ? value.toLocaleString() : value;
	}

	const facts = $derived<Array<[string, string]>>([
		['Runtime', observed.runtime],
		['Lifecycle', value(observed.lifecycle)],
		['Mail delivery', value(observed.control)],
		['Source', observed.source],
		['Source version', value(observed.source_version)],
		['Fidelity', observed.fidelity],
		['Coverage', observed.completeness],
		['Observed at', observed.observed_at],
		['Requested model', value(observed.requested_model)],
		['Observed model', value(observed.observed_model)],
		['Usage scope', value(observed.usage_scope)],
		['Usage basis', value(observed.usage_basis)],
		['Input tokens', value(observed.input_tokens)],
		['Cached input tokens', value(observed.cached_input_tokens)],
		['Output tokens', value(observed.output_tokens)],
		['Reasoning output tokens', value(observed.reasoning_output_tokens)],
		['Total tokens', value(observed.total_tokens)],
		['Context window', value(observed.context_window)],
		['Model calls', value(observed.model_call_count)],
		['Reported cost', 'Unknown']
	]);

	function itemFacts(item: RuntimeItem): Array<[string, string]> {
		return [
			['Item', item.item_id],
			['Turn', value(item.turn_id)],
			['Status', value(item.status)],
			['Success', item.success === null ? 'Unknown' : String(item.success)],
			['Exit code', value(item.exit_code)],
			['First seen', item.first_seen_at],
			['Observed at', item.observed_at],
			['Source', item.source],
			['Fidelity', item.fidelity],
			['Coverage', item.completeness],
			['Content', item.content_availability]
		];
	}

	async function loadItems(offset: number, refresh = false): Promise<void> {
		if (loading) return;
		const id = sessionId;
		requestedOffset = offset;
		loading = true;
		error = null;
		try {
			if (refresh) await invalidateApiCache(`/api/sessions/${encodeURIComponent(id)}`);
			const detail = await getSession(id, {
				offset,
				limit: Math.max(1, Math.min(items?.limit ?? 50, 100)),
				order: 'desc'
			});
			if (sessionId !== id) return;
			if (!detail.runtime_items) throw new Error('Runtime activity is unavailable.');
			items = detail.runtime_items;
			refreshed = detail.observation ?? null;
		} catch (cause) {
			if (sessionId === id) error = cause instanceof Error ? cause.message : String(cause);
		} finally {
			if (sessionId === id) loading = false;
		}
	}
</script>

{#snippet factRows(rows: Array<[string, string]>, label?: string)}
	<dl
		aria-label={label}
		class="grid grid-cols-1 gap-x-4 gap-y-3 text-xs sm:grid-cols-2 lg:grid-cols-3"
	>
		{#each rows as [name, fact] (name)}
			<div>
				<dt class="uppercase tracking-wider text-[var(--text-3)]">{name}</dt>
				<dd class="mt-0.5 break-words font-mono text-[var(--text-strong)]">{fact}</dd>
			</div>
		{/each}
	</dl>
{/snippet}

<div class="space-y-3" data-slot="runtime-session">
	<SectionCard
		title="Runtime observation"
		description="Activity metadata is independent of diagnostic wire capture; missing measurements remain unknown."
	>
		{#snippet headerActions()}
			<Button variant="outline" size="sm" disabled={loading} onclick={() => void loadItems(0, true)}
				>Refresh runtime activity</Button
			>
		{/snippet}
		{@render factRows(facts, 'Runtime observation facts')}
	</SectionCard>
	<SectionCard
		title="Runtime activity"
		description="Complete metadata rows for observed turns, tools and commands; content availability is shown for each item."
	>
		{#snippet headerActions()}
			{#if items}
				<span class="text-xs text-[var(--text-3)]" aria-live="polite"
					>Items {items.items.length ? items.offset + 1 : 0}–{items.offset +
						items.items.length}</span
				>
				<Button
					variant="outline"
					size="sm"
					disabled={loading || items.offset === 0}
					onclick={() => void loadItems(Math.max(0, items!.offset - items!.limit))}
					>Previous activity page</Button
				>
				<Button
					variant="outline"
					size="sm"
					disabled={loading || !items.has_more}
					onclick={() => void loadItems(items!.offset + items!.limit)}>Next activity page</Button
				>
			{/if}
		{/snippet}
		{#if error}
			<p role="alert" class="text-xs text-[var(--status-danger-fg)]">{error}</p>
			<Button
				variant="outline"
				size="sm"
				disabled={loading}
				onclick={() => void loadItems(requestedOffset)}>Retry runtime activity</Button
			>
		{/if}
		{#if items === null}
			<p class="text-xs text-[var(--text-3)]">Runtime activity is unavailable.</p>
		{:else if items.items.length === 0}
			<p class="text-xs text-[var(--text-3)]">No runtime items are retained for this page.</p>
		{:else}
			<ol class="space-y-3" aria-label="Runtime activity items" start={items.offset + 1}>
				{#each items.items as item (item.item_id)}
					<li class="border-t border-[var(--border-subtle)] pt-3">
						<h4 class="mb-2 break-words font-mono text-xs text-[var(--text-strong)]">
							{item.kind} · {item.name ?? item.item_id}
						</h4>
						{@render factRows(itemFacts(item))}
					</li>
				{/each}
			</ol>
		{/if}
	</SectionCard>
</div>
