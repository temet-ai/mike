<script lang="ts">
	import { untrack } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import Field from '$lib/components/ui/field/field.svelte';
	import {
		KnowledgeKindError,
		kindSource,
		patchKnowledgeKind,
		type KnowledgeKind,
		type KnowledgeKindField,
		type KnowledgeKindPatch
	} from '$lib/api/knowledgeKinds';
	import { joinList, splitList } from './list';

	interface Props {
		kind: KnowledgeKind;
		onClose: () => void;
		onSaved: (row: KnowledgeKind) => void;
	}

	const { kind, onClose, onSaved }: Props = $props();

	// The drafts start from the row as it was when the dialog opened; a poll never overwrites them.
	const start = untrack(() => kind);
	// Watch columns are editable when the kind has one watch; several watches are shown read-only.
	const editableWatch = start.watches.length === 1;
	let label = $state(start.label);
	let description = $state(start.description);
	let settle = $state(String(start.settle_secs));
	let watchColumns = $state(joinList(start.watches[0]?.watch_columns ?? []));

	const SHOWN: KnowledgeKindField[] = ['label', 'description', 'settle_secs', 'watch_columns'];

	let busy = $state(false);
	let refusal = $state<{ field: KnowledgeKindField | null; message: string } | null>(null);

	const patch = $derived.by((): KnowledgeKindPatch => {
		const p: KnowledgeKindPatch = {};
		if (label !== start.label) p.label = label;
		if (description !== start.description) p.description = description;
		if (String(settle).trim() !== String(start.settle_secs))
			p.settle_secs = Number(String(settle).trim());
		if (
			editableWatch &&
			splitList(watchColumns).join(',') !== start.watches[0].watch_columns.join(',')
		) {
			p.watch_columns = splitList(watchColumns);
		}
		return p;
	});
	const changed = $derived(Object.keys(patch).length > 0);

	function errorFor(field: KnowledgeKindField): string | null {
		return refusal?.field === field ? refusal.message : null;
	}
	const generalError = $derived(
		refusal !== null && (refusal.field === null || !SHOWN.includes(refusal.field))
			? refusal.message
			: null
	);

	async function save(): Promise<void> {
		busy = true;
		refusal = null;
		try {
			onSaved(await patchKnowledgeKind(start.kind, patch));
		} catch (err) {
			refusal = {
				field: err instanceof KnowledgeKindError ? err.field : null,
				message: err instanceof Error ? err.message : String(err)
			};
		} finally {
			busy = false;
		}
	}
</script>

<Dialog.Root
	open
	onOpenChange={(open) => {
		if (!open && !busy) onClose();
	}}
>
	<Dialog.Content class="sm:max-w-xl" showCloseButton={false}>
		<Dialog.Header>
			<Dialog.Title>Tune {start.label}</Dialog.Title>
			<Dialog.Description>A change applies to the next write.</Dialog.Description>
		</Dialog.Header>

		<dl class="kk-def">
			<dt>Source</dt>
			<dd>{kindSource(start)}</dd>
			{#if start.definition}
				<dt>Key</dt>
				<dd>{start.definition.key_column}</dd>
				<dt>Title</dt>
				<dd>{start.definition.title_column}</dd>
				<dt>Body</dt>
				<dd>{joinList(start.definition.body_columns)}</dd>
				{#if start.definition.project_column}
					<dt>Project</dt>
					<dd>{start.definition.project_column}</dd>
				{/if}
			{/if}
			{#each start.watches as watch (watch.table)}
				<dt>Watch</dt>
				<dd>
					{watch.table} · key {watch.key_column}{editableWatch
						? ''
						: ` · ${joinList(watch.watch_columns)}`}
				</dd>
			{/each}
		</dl>

		<Field label="Label" bind:value={label} error={errorFor('label')} disabled={busy} />
		<Field
			label="Description"
			multiline
			rows={2}
			bind:value={description}
			error={errorFor('description')}
			disabled={busy}
		/>
		<Field
			label="Settle time"
			hint="seconds"
			type="number"
			bind:value={settle}
			error={errorFor('settle_secs')}
			disabled={busy}
		/>
		{#if editableWatch}
			<Field
				label="Re-embed when these change"
				hint="comma-separated"
				bind:value={watchColumns}
				error={errorFor('watch_columns')}
				disabled={busy}
			/>
		{/if}

		<p class="kk-alert" role="alert">{generalError ?? ''}</p>
		<Dialog.Footer>
			<Button variant="ghost" disabled={busy} onclick={onClose}>Cancel</Button>
			<Button disabled={busy || !changed} onclick={() => void save()}>Save</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>

<style>
	.kk-def {
		display: grid;
		grid-template-columns: max-content minmax(0, 1fr);
		gap: var(--space-1) var(--space-3);
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
	}
	.kk-def dt {
		color: var(--text-3);
	}
	.kk-def dd {
		margin: 0;
		color: var(--text-1);
		overflow-wrap: anywhere;
	}
	.kk-alert {
		margin: 0;
		min-block-size: 1.4em;
		font-size: var(--text-sm);
		color: var(--status-danger-fg);
		overflow-wrap: anywhere;
	}
</style>
