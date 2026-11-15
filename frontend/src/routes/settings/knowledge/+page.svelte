<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { Clock, TriangleAlert } from '@lucide/svelte';
	import SettingsPage from '$lib/components/settings/SettingsPage.svelte';
	import SettingsSection from '$lib/components/settings/SettingsSection.svelte';
	import EmptyStateOnboarding from '$lib/components/shared/EmptyStateOnboarding.svelte';
	import StatusMark from '$lib/components/ui/status-mark/status-mark.svelte';
	import KnowledgeKindsTable from '$lib/components/settings/knowledge/KnowledgeKindsTable.svelte';
	import KnowledgeStatusLine from '$lib/components/settings/knowledge/KnowledgeStatusLine.svelte';
	import KnowledgeKindForm from '$lib/components/settings/knowledge/KnowledgeKindForm.svelte';
	import KnowledgeKindTuneDialog from '$lib/components/settings/knowledge/KnowledgeKindTuneDialog.svelte';
	import KnowledgeKindDeleteDialog from '$lib/components/settings/knowledge/KnowledgeKindDeleteDialog.svelte';
	import { patchKnowledgeKind, type KnowledgeKind } from '$lib/api/knowledgeKinds';
	import { createKnowledgeKindsStore } from '$lib/state/knowledgeKinds.svelte';
	import type { KnowledgePageData } from './+page';

	const { data }: { data: KnowledgePageData } = $props();

	// The load result seeds the list; later polls replace it.
	const store = createKnowledgeKindsStore(untrack(() => data.kinds));

	/** The switch a row just moved, kept until its save settles so a poll cannot flip it back. */
	let pendingEnabled = $state<Record<string, boolean>>({});
	let status = $state<{ text: string; failed: boolean }>({ text: '', failed: false });
	let tuneKind = $state<string | null>(null);
	let deleteKind = $state<string | null>(null);

	const kinds = $derived<KnowledgeKind[]>(
		(store.snapshot ?? []).map((k) =>
			k.kind in pendingEnabled ? { ...k, enabled: pendingEnabled[k.kind] } : k
		)
	);
	const tuneRow = $derived(kinds.find((k) => k.kind === tuneKind) ?? null);
	const deleteRow = $derived(kinds.find((k) => k.kind === deleteKind) ?? null);
	const loadError = $derived(store.snapshot === null ? (store.error ?? data.loadError) : null);

	onMount(() => store.start());

	function clock(at: number | null): string {
		return at === null ? '—' : new Date(at).toLocaleTimeString('en-GB');
	}

	async function toggle(kind: string, enabled: boolean): Promise<void> {
		pendingEnabled = { ...pendingEnabled, [kind]: enabled };
		status = { text: `Saving ${kind}`, failed: false };
		try {
			store.apply(await patchKnowledgeKind(kind, { enabled }));
			status = { text: `Saved ${kind}`, failed: false };
		} catch (err) {
			const message = err instanceof Error ? err.message : String(err);
			status = { text: `${kind}: ${message}`, failed: true };
		} finally {
			pendingEnabled = Object.fromEntries(
				Object.entries(pendingEnabled).filter(([pending]) => pending !== kind)
			);
		}
	}
</script>

<svelte:head><title>Knowledge — Settings — mike</title></svelte:head>

<SettingsPage
	title="Knowledge sources"
	current="/settings/knowledge"
	wide
	loadErrors={[loadError === null ? null : `Knowledge sources: ${loadError}`]}
	onRetry={() => void store.refresh()}
>
	{#snippet lede()}
		Every table mike embeds so search and recall can find its rows by meaning. Add a table, or
		switch one off.
	{/snippet}

	{#if store.snapshot !== null}
		<SettingsSection title="Knowledge sources" id="knowledge-sources">
			{#snippet action()}
				<span class="kk-updated">
					{#if store.error === null}
						<StatusMark icon={Clock} tone="quiet" label="Updated {clock(store.lastFetchedAt)}" />
					{:else}
						<StatusMark
							icon={TriangleAlert}
							tone="warning"
							label="Update failed · showing {clock(store.lastFetchedAt)}"
						/>
					{/if}
				</span>
			{/snippet}

			{#if kinds.length === 0}
				<EmptyStateOnboarding
					headline="No knowledge sources"
					body="Add a table below and mike embeds its rows."
					command={null}
					confirmation=""
				/>
			{:else}
				<KnowledgeKindsTable
					{kinds}
					saving={Object.keys(pendingEnabled)}
					onToggle={(kind, enabled) => void toggle(kind, enabled)}
					onTune={(kind) => (tuneKind = kind)}
					onDelete={(kind) => (deleteKind = kind)}
				/>
			{/if}
			<KnowledgeStatusLine text={status.text} failed={status.failed} />

			<div class="kk-add">
				<h3 class="panel-subhd">Add a source</h3>
				<KnowledgeKindForm
					onAdded={(kind) => {
						status = { text: `Added ${kind}`, failed: false };
						void store.refresh();
					}}
				/>
			</div>
		</SettingsSection>
	{/if}
</SettingsPage>

{#if tuneRow}
	<KnowledgeKindTuneDialog
		kind={tuneRow}
		onClose={() => (tuneKind = null)}
		onSaved={(row) => {
			store.apply(row);
			status = { text: `Saved ${row.kind}`, failed: false };
			tuneKind = null;
		}}
	/>
{/if}
{#if deleteRow}
	<KnowledgeKindDeleteDialog
		kind={deleteRow}
		onClose={() => (deleteKind = null)}
		onDeleted={(kind) => {
			store.remove(kind);
			status = { text: `Deleted ${kind}`, failed: false };
			deleteKind = null;
		}}
	/>
{/if}

<style>
	.kk-updated {
		display: inline-flex;
		min-inline-size: 22ch;
		justify-content: flex-end;
	}
	.kk-add {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding: var(--space-4) calc(var(--edge-md) + var(--space-2));
		border-block-start: var(--hairline) solid var(--border-subtle);
	}
</style>
