<script lang="ts" module>
	export interface LegendEntry {
		readonly key: string;
		readonly label: string;
		readonly kind: 'fill' | 'line' | 'ring';
		readonly colorToken: string;
		readonly pattern?: string;
	}
</script>

<script lang="ts">
	import { SvelteSet } from 'svelte/reactivity';
	import { cn } from '$lib/utils';

	interface Props {
		entries: readonly LegendEntry[];
		active: Set<string>;
		onChange?: (active: Set<string>) => void;
	}

	let { entries, active = $bindable(), onChange }: Props = $props();

	function toggle(key: string): void {
		const next = new SvelteSet(active);
		if (next.has(key)) {
			next.delete(key);
		} else {
			next.add(key);
		}
		active = next;
		onChange?.(next);
	}
</script>

<div class="legend-filter">
	{#each entries as entry (entry.key)}
		{@const on = active.has(entry.key)}
		<button
			type="button"
			class={cn('legend-filter__entry edge machine', !on && 'legend-filter__entry--off')}
			style="--edge: 4px; --edge-pad: 0px; --edge-line: var(--border-default); --edge-surface: var(--surface-2);"
			aria-pressed={on}
			onclick={() => toggle(entry.key)}
		>
			<span class="legend-filter__sample" aria-hidden="true">
				{#if entry.kind === 'fill'}
					<span class="legend-filter__swatch" style="background: var({entry.colorToken});"></span>
				{:else if entry.kind === 'line'}
					<svg width="20" height="10" viewBox="0 0 20 10">
						<line
							x1="0"
							y1="5"
							x2="20"
							y2="5"
							stroke="var({entry.colorToken})"
							stroke-width="1.5"
							stroke-dasharray={entry.pattern || undefined}
						/>
					</svg>
				{:else}
					<svg width="10" height="8" viewBox="0 0 10 8">
						<ellipse
							cx="5"
							cy="4"
							rx="4"
							ry="3"
							fill="none"
							stroke="var({entry.colorToken})"
							stroke-width="1.5"
						/>
					</svg>
				{/if}
			</span>
			<span class="legend-filter__label">{entry.label}</span>
		</button>
	{/each}
</div>

<style>
	.legend-filter {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2, 8px);
	}
	.legend-filter__entry {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 2px 7px;
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--text-2);
		cursor: pointer;
	}
	.legend-filter__entry--off {
		opacity: 0.32;
	}
	.legend-filter__sample {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		flex: 0 0 auto;
	}
	.legend-filter__swatch {
		width: 10px;
		height: 10px;
	}
</style>
