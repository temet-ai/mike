<script lang="ts">
	import SettingsPage from '$lib/components/settings/SettingsPage.svelte';
	import { Button } from '$lib/components/ui/button';
	import Field from '$lib/components/ui/field/field.svelte';
	import ChangeLogTable from '$lib/components/settings/ChangeLogTable.svelte';
	import { listConfigChanges } from '$lib/api/configHistory';
	import type { ChangeLogRow, ChangeSubjectKind, ChangeVia } from '$lib/api/configHistory';
	import type { HistoryPageData } from './+page';

	const { data }: { data: HistoryPageData } = $props();

	const PAGE = 100;
	const KINDS: ChangeSubjectKind[] = ['setting', 'toggle', 'steerer', 'build', 'promotion'];
	const VIAS: ChangeVia[] = ['spa', 'api', 'startup', 'migration', 'promotion', 'mike', 'sql'];

	let rows = $state<ChangeLogRow[]>([]);
	let nextBeforeId = $state<number | null>(null);
	let loadError = $state<string | null>(null);
	let loading = $state(false);
	let kind = $state<ChangeSubjectKind | ''>('');
	let via = $state<ChangeVia | ''>('');
	let prefix = $state('');

	$effect(() => {
		rows = data.page?.changes ?? [];
		nextBeforeId = data.page?.next_before_id ?? null;
		loadError = data.loadError;
	});

	function query(beforeId?: number) {
		return {
			subject_kind: kind || undefined,
			via: via || undefined,
			subject_key_prefix: prefix.trim() || undefined,
			before_id: beforeId,
			limit: PAGE
		};
	}

	async function applyFilters(event: SubmitEvent) {
		event.preventDefault();
		loading = true;
		try {
			const page = await listConfigChanges(query());
			rows = page.changes;
			nextBeforeId = page.next_before_id;
			loadError = null;
		} catch (e) {
			loadError = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	async function loadOlder() {
		if (nextBeforeId === null) return;
		loading = true;
		try {
			const page = await listConfigChanges(query(nextBeforeId));
			rows = [...rows, ...page.changes];
			nextBeforeId = page.next_before_id;
		} catch (e) {
			loadError = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}
</script>

<svelte:head><title>Configuration history — mike</title></svelte:head>

<SettingsPage
	title="Configuration history"
	current="/settings/history"
	wide
	loadErrors={[loadError]}
>
	{#snippet lede()}
		Every change to settings, feature toggles and steerers, newest first, with the value before and
		after.
	{/snippet}

	<form class="hist-filters" onsubmit={applyFilters}>
		<label>
			<span class="machine">Kind</span>
			<select bind:value={kind}>
				<option value="">all</option>
				{#each KINDS as k (k)}<option value={k}>{k}</option>{/each}
			</select>
		</label>
		<Field label="Key starts with" bind:value={prefix} placeholder="judge_worker." />
		<label>
			<span class="machine">Via</span>
			<select bind:value={via}>
				<option value="">all</option>
				{#each VIAS as v (v)}<option value={v}>{v}</option>{/each}
			</select>
		</label>
		<Button type="submit" variant="outline" disabled={loading}>Apply</Button>
	</form>

	<ChangeLogTable {rows} />

	{#if nextBeforeId !== null}
		<Button variant="outline" class="self-start" disabled={loading} onclick={() => void loadOlder()}
			>Load older</Button
		>
	{:else if rows.length > 0}
		<p class="hist-end">This is the start of the change log.</p>
	{/if}
</SettingsPage>

<style>
	.hist-end {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.hist-filters {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-3);
		align-items: end;
		color: var(--text-2);
	}
	.hist-filters label {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.hist-filters label .machine {
		font-size: var(--text-xs);
	}
	.hist-filters select {
		block-size: var(--control-h);
	}
</style>
