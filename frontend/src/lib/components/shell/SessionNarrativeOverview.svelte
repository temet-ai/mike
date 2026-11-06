<script lang="ts">
	import { NotebookText } from '@lucide/svelte';

	interface Props {
		overview?: string;
	}

	let { overview = '' }: Props = $props();

	const hasOverview = $derived(overview.trim().length > 0);
</script>

<div class="sno-card edge edge-green">
	<div class="sno-card-hd panel-hd">
		<NotebookText size={12} aria-hidden="true" />
		Session overview
	</div>
	<div class="sno-body">
		{#if hasOverview}
			<p class="sno-text">{overview}</p>
		{:else}
			<p class="sno-empty">No overview recorded for this session yet.</p>
			<div class="sno-ghost" aria-hidden="true">
				<span class="sno-ghost-line" style="width: 96%;"></span>
				<span class="sno-ghost-line" style="width: 88%;"></span>
				<span class="sno-ghost-line" style="width: 92%;"></span>
				<span class="sno-ghost-line" style="width: 40%;"></span>
			</div>
			<div class="sno-ghost" aria-hidden="true">
				<span class="sno-ghost-line" style="width: 90%;"></span>
				<span class="sno-ghost-line" style="width: 94%;"></span>
				<span class="sno-ghost-line" style="width: 58%;"></span>
			</div>
			<p class="sno-caption">
				Reserved: a two-paragraph narrative of what this session set out to do and where it landed.
			</p>
		{/if}
	</div>
</div>

<style>
	.sno-card {
		--edge: 10px;
		display: flex;
		flex-direction: column;
		min-width: 0;
		height: 100%;
	}

	.sno-card-hd {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 10px 12px;
		border-bottom: 1px solid var(--border-subtle);
	}

	.sno-body {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 12px;
	}

	.sno-text {
		margin: 0;
		max-width: 92ch;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		line-height: 1.55;
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.sno-empty {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		font-style: italic;
		color: var(--text-3);
	}

	.sno-ghost {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.sno-ghost-line {
		display: block;
		height: 9px;
		border-radius: 2px;
		background: var(--surface-2);
	}

	.sno-caption {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
	}
</style>
