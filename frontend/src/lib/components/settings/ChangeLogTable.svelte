<script lang="ts">
	import type { ChangeLogRow } from '$lib/api/configHistory';
	import { VIA_WORDS } from '$lib/api/configHistory';

	let { rows }: { rows: ChangeLogRow[] } = $props();

	function whole(value: unknown): string {
		if (value === null || value === undefined) return '—';
		return JSON.stringify(value, null, 2);
	}

	function steererName(row: ChangeLogRow): string | null {
		if (row.subject_kind !== 'steerer') return null;
		const source = (row.new_value ?? row.old_value) as { name?: unknown } | null;
		return typeof source?.name === 'string' ? source.name : null;
	}

	function when(iso: string): string {
		return new Date(iso).toLocaleString();
	}
</script>

<ol class="change-log" aria-label="Configuration changes">
	{#each rows as row (row.id)}
		<li class="change" data-change-id={row.id}>
			<div class="change-head">
				<time datetime={row.changed_at}>{when(row.changed_at)}</time>
				<span class="chip chip-kind">{row.subject_kind}</span>
				<span class="key">
					{row.subject_key}{#if steererName(row)}<span class="steerer-name">
							· {steererName(row)}</span
						>{/if}
				</span>
				<span class="op">{row.op}</span>
				<span class="chip chip-via" title={VIA_WORDS[row.via]}>{row.via}</span>
				{#if row.actor}<span class="actor">{row.actor}</span>{/if}
				{#if row.build_sha}<span class="sha">{row.build_sha.slice(0, 8)}</span>{/if}
			</div>
			<p class="via-words">{VIA_WORDS[row.via]}</p>
			{#if row.redacted}
				<p class="redacted">value withheld: secret-shaped key</p>
			{:else}
				<div class="values">
					<div class="value">
						<span class="value-label">before</span>
						<pre>{whole(row.old_value)}</pre>
					</div>
					<div class="value">
						<span class="value-label">after</span>
						<pre>{whole(row.new_value)}</pre>
					</div>
				</div>
			{/if}
		</li>
	{/each}
</ol>

<style>
	.change-log {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}
	.change {
		background: var(--surface-card);
		border: 1px solid var(--border-subtle);
		padding: var(--space-3);
	}
	.change-head {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-2);
		font-family: var(--font-machine);
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.key {
		color: var(--text-strong);
		overflow-wrap: anywhere;
	}
	.steerer-name {
		color: var(--text-2);
	}
	.chip {
		padding: 0 var(--space-2);
		border: 1px solid var(--border-default);
		background: var(--surface-2);
	}
	.via-words,
	.redacted {
		margin: var(--space-2) 0 0;
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.values {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr));
		gap: var(--space-2);
		margin-top: var(--space-2);
	}
	.value-label {
		font-family: var(--font-machine);
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	pre {
		margin: 0;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		font-family: var(--font-machine);
		font-size: var(--text-xs);
		color: var(--text-strong);
		background: var(--surface-bg);
		padding: var(--space-2);
	}
</style>
