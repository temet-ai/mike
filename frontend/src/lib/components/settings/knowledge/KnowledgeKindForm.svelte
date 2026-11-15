<script lang="ts">
	import { onMount } from 'svelte';
	import { CircleX } from '@lucide/svelte';
	import { Button } from '$lib/components/ui/button';
	import Field from '$lib/components/ui/field/field.svelte';
	import StatusMark from '$lib/components/ui/status-mark/status-mark.svelte';
	import * as RadioGroup from '$lib/components/ui/radio-group';
	import {
		KnowledgeKindError,
		createKnowledgeKind,
		getKnowledgeTables,
		previewKnowledgeKind,
		type KnowledgeKindDraft,
		type KnowledgeKindField,
		type KnowledgeKindPreviewRow,
		type KnowledgeTable
	} from '$lib/api/knowledgeKinds';
	import KnowledgeKindPreview, { type PreviewState } from './KnowledgeKindPreview.svelte';
	import { joinList, splitList } from './list';

	interface Props {
		onAdded: (kind: string) => void;
	}

	const { onAdded }: Props = $props();

	type Mode = 'table' | 'view';
	/** The fields each mode shows; a refusal naming another field goes to the form-level line. */
	const TABLE_FIELDS: KnowledgeKindField[] = [
		'kind',
		'label',
		'description',
		'source_table',
		'key_column',
		'title_column',
		'body_columns',
		'project_column',
		'watch_columns',
		'settle_secs'
	];
	const VIEW_FIELDS: KnowledgeKindField[] = [
		'kind',
		'label',
		'description',
		'view',
		'source_table',
		'key_column',
		'watch_columns',
		'settle_secs'
	];

	let name = $state('');
	let label = $state('');
	let description = $state('');
	let mode = $state<Mode>('table');
	let table = $state('');
	let keyColumn = $state('');
	let titleColumn = $state('');
	let projectColumn = $state('');
	let bodyColumns = $state('');
	let watchColumns = $state('');
	let settle = $state('');
	let view = $state('');

	let tables = $state<KnowledgeTable[]>([]);
	let previewing = $state(false);
	let adding = $state(false);
	/** Outcomes are tied to the draft they answered; editing any field makes them stale. */
	let previewOutcome = $state<{
		key: string;
		row: KnowledgeKindPreviewRow | null;
		error: { field: KnowledgeKindField | null; message: string } | null;
	} | null>(null);
	let addRefusal = $state<{
		key: string;
		field: KnowledgeKindField | null;
		message: string;
	} | null>(null);

	onMount(() => {
		// Suggestions are optional: without the listing the fields are plain text.
		void getKnowledgeTables()
			.then((listed) => (tables = listed))
			.catch(() => undefined);
	});

	function settleSecs(): number | null {
		const text = String(settle ?? '').trim();
		return text === '' ? null : Number(text);
	}

	const draft = $derived<KnowledgeKindDraft>({
		kind: name.trim(),
		label: label.trim(),
		description: description.trim(),
		source_table: table.trim(),
		key_column: keyColumn.trim(),
		title_column: mode === 'table' ? titleColumn.trim() : null,
		body_columns: mode === 'table' ? splitList(bodyColumns) : [],
		project_column: mode === 'table' && projectColumn.trim() !== '' ? projectColumn.trim() : null,
		watch_columns: splitList(watchColumns),
		settle_secs: settleSecs(),
		view: mode === 'view' ? view.trim() : null
	});
	const draftKey = $derived(JSON.stringify(draft));

	const shownFields = $derived(mode === 'table' ? TABLE_FIELDS : VIEW_FIELDS);
	const currentPreview = $derived(
		previewOutcome !== null && previewOutcome.key === draftKey ? previewOutcome : null
	);
	const currentAdd = $derived(
		addRefusal !== null && addRefusal.key === draftKey ? addRefusal : null
	);
	const refusal = $derived(currentAdd ?? currentPreview?.error ?? null);
	const previewed = $derived(currentPreview !== null && currentPreview.row !== null);

	function errorFor(field: KnowledgeKindField): string | null {
		return refusal !== null && refusal.field === field && shownFields.includes(field)
			? refusal.message
			: null;
	}
	const besideField = $derived(
		refusal !== null && refusal.field !== null && shownFields.includes(refusal.field)
	);
	const formLine = $derived(currentAdd !== null && !besideField ? currentAdd.message : null);

	const previewState = $derived.by((): PreviewState => {
		if (previewing) return { status: 'loading' };
		if (currentPreview === null) return { status: 'idle' };
		if (currentPreview.error !== null) {
			return { status: 'refused', message: besideField ? null : currentPreview.error.message };
		}
		if (currentPreview.row === null) return { status: 'empty' };
		return { status: 'ready', row: currentPreview.row };
	});

	const known = $derived(tables.find((t) => t.name === table.trim()) ?? null);
	const columnNames = $derived(known?.columns.map((c) => c.name) ?? []);

	function toRefusal(err: unknown): { field: KnowledgeKindField | null; message: string } {
		return {
			field: err instanceof KnowledgeKindError ? err.field : null,
			message: err instanceof Error ? err.message : String(err)
		};
	}

	async function runPreview(): Promise<void> {
		const key = draftKey;
		previewing = true;
		previewOutcome = null;
		addRefusal = null;
		try {
			const row = await previewKnowledgeKind(draft);
			previewOutcome = { key, row, error: null };
		} catch (err) {
			previewOutcome = { key, row: null, error: toRefusal(err) };
		} finally {
			previewing = false;
		}
	}

	function reset(): void {
		name = label = description = table = keyColumn = titleColumn = projectColumn = '';
		bodyColumns = watchColumns = settle = view = '';
		mode = 'table';
		previewOutcome = null;
		addRefusal = null;
	}

	async function add(): Promise<void> {
		const key = draftKey;
		adding = true;
		addRefusal = null;
		try {
			const created = await createKnowledgeKind(draft);
			reset();
			onAdded(created);
		} catch (err) {
			addRefusal = { key, ...toRefusal(err) };
		} finally {
			adding = false;
		}
	}
</script>

<form class="kk-form" aria-label="Add a knowledge source" onsubmit={(e) => e.preventDefault()}>
	<datalist id="kk-tables">
		{#each tables as t (t.name)}<option value={t.name}></option>{/each}
	</datalist>
	<datalist id="kk-columns">
		{#each columnNames as c (c)}<option value={c}></option>{/each}
	</datalist>

	<div class="kk-grid">
		<Field label="Name" hint="lowercase, underscores" bind:value={name} error={errorFor('kind')} />
		<Field label="Label" bind:value={label} error={errorFor('label')} />
		<Field
			class="col-span-full"
			label="Description"
			hint="optional"
			bind:value={description}
			error={errorFor('description')}
		/>

		<div class="col-span-full">
			<span id="kk-source-label" class="machine kk-source-label">Source</span>
			<div class="edge kk-radios">
				<RadioGroup.Root
					name="kk-source"
					bind:value={mode}
					aria-labelledby="kk-source-label"
					class="mech-group"
				>
					<RadioGroup.Item value="table" label="A table" />
					<RadioGroup.Item value="view" label="A view" />
				</RadioGroup.Root>
			</div>
		</div>

		{#if mode === 'view'}
			<Field
				class="col-span-full"
				label="View name"
				hint="must return source_id, project_id, reference, label, name, body"
				bind:value={view}
				error={errorFor('view')}
			/>
		{/if}
		<Field
			label={mode === 'view' ? 'Watched table' : 'Table'}
			placeholder="schema.table"
			list="kk-tables"
			bind:value={table}
			error={errorFor('source_table')}
		/>
		<Field
			label="Key column"
			list="kk-columns"
			bind:value={keyColumn}
			error={errorFor('key_column')}
		/>
		{#if mode === 'table'}
			<Field
				label="Title column"
				list="kk-columns"
				bind:value={titleColumn}
				error={errorFor('title_column')}
			/>
			<Field
				label="Project column"
				hint="optional"
				list="kk-columns"
				bind:value={projectColumn}
				error={errorFor('project_column')}
			/>
			<div class="col-span-full">
				<Field
					label="Body columns"
					hint="comma-separated"
					bind:value={bodyColumns}
					error={errorFor('body_columns')}
				/>
				{@render columnsHint()}
			</div>
		{/if}
		<div class="col-span-full">
			<Field
				label="Re-embed when these change"
				hint="optional, comma-separated; empty means title, body and project columns"
				bind:value={watchColumns}
				error={errorFor('watch_columns')}
			/>
			{@render columnsHint()}
		</div>
		<Field
			label="Settle time"
			hint="seconds, optional"
			type="number"
			bind:value={settle}
			error={errorFor('settle_secs')}
		/>
	</div>

	<KnowledgeKindPreview state={previewState} />

	<p class="kk-line" role="alert">
		{#if formLine !== null}<StatusMark icon={CircleX} tone="danger" label={formLine} />{/if}
	</p>

	<div class="kk-actions">
		<Button variant="outline" disabled={previewing || adding} onclick={() => void runPreview()}
			>Preview</Button
		>
		<Button disabled={!previewed || previewing || adding} onclick={() => void add()}>
			{adding ? 'Adding' : 'Add source'}
		</Button>
	</div>
</form>

{#snippet columnsHint()}
	{#if tables.length > 0}
		<p class="kk-cols">{columnNames.length > 0 ? `Columns: ${joinList(columnNames)}` : ''}</p>
	{/if}
{/snippet}

<style>
	.kk-form {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	.kk-grid {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: var(--space-4);
	}
	@media (max-width: 640px) {
		.kk-grid {
			grid-template-columns: minmax(0, 1fr);
		}
	}
	.kk-source-label {
		display: block;
		margin-block-end: 6px;
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.kk-radios {
		--edge: var(--edge-sm);
		--edge-pad: 0px;
		--edge-line: var(--border-default);
		--edge-surface: var(--surface-bg);
	}
	.kk-cols {
		margin: 0;
		min-block-size: 1.4em;
		font-size: var(--text-xs);
		color: var(--text-3);
		overflow-wrap: anywhere;
	}
	.kk-line {
		margin: 0;
		min-block-size: 1.55em;
	}
	.kk-actions {
		display: flex;
		gap: var(--space-2);
	}
</style>
