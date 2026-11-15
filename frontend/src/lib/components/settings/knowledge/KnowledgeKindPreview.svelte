<script lang="ts" module>
	import type { KnowledgeKindPreviewRow } from '$lib/api/knowledgeKinds';

	export type PreviewState =
		| { status: 'idle' }
		| { status: 'loading' }
		| { status: 'ready'; row: KnowledgeKindPreviewRow }
		| { status: 'empty' }
		/** `message` is null when the refusal sits beside the field it names. */
		| { status: 'refused'; message: string | null };
</script>

<script lang="ts">
	import { CircleCheck, CircleX, LoaderCircle, TriangleAlert } from '@lucide/svelte';
	import StatusMark from '$lib/components/ui/status-mark/status-mark.svelte';

	interface Props {
		state: PreviewState;
	}

	const { state: preview }: Props = $props();
</script>

<div class="edge kk-preview" data-slot="knowledge-kind-preview" aria-live="polite">
	{#if preview.status === 'idle'}
		<p class="kk-quiet">A real row, as it would be embedded. Press Preview.</p>
	{:else if preview.status === 'loading'}
		<StatusMark icon={LoaderCircle} tone="info" label="Reading a row" spin />
	{:else if preview.status === 'ready'}
		<StatusMark icon={CircleCheck} tone="success" label="Row {preview.row.reference}" />
		<dl class="kk-dl">
			<dt>Reference</dt>
			<dd>{preview.row.reference}</dd>
			<dt>Header</dt>
			<dd>{preview.row.header}</dd>
			<dt>Body</dt>
			<dd class="kk-body">{preview.row.body}</dd>
		</dl>
	{:else if preview.status === 'empty'}
		<StatusMark icon={TriangleAlert} tone="warning" label="The table has no rows to preview yet" />
	{:else if preview.message === null}
		<p class="kk-quiet">Not previewed</p>
	{:else}
		<StatusMark icon={CircleX} tone="danger" label={preview.message} />
	{/if}
</div>

<style>
	.kk-preview {
		--edge: var(--edge-sm);
		--edge-pad: var(--space-3);
		--edge-line: var(--border-default);
		--edge-surface: var(--surface-2);
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-block-size: 12rem;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
	}
	.kk-quiet {
		margin: 0;
		color: var(--text-3);
	}
	.kk-dl {
		display: grid;
		grid-template-columns: max-content minmax(0, 1fr);
		gap: var(--space-1) var(--space-3);
		margin: 0;
	}
	.kk-dl dt {
		color: var(--text-3);
	}
	.kk-dl dd {
		margin: 0;
		color: var(--text-1);
		overflow-wrap: anywhere;
	}
	.kk-body {
		max-block-size: 24rem;
		overflow: auto;
		white-space: pre-wrap;
	}
</style>
