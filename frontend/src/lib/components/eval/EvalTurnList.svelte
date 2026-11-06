<!--
@component
Every turn of one tab of the Review labels page, in turn-id order: the session it belongs to,
its position in that session, what the label and the judge say, and what you decided. Rendered in
the page's "All turns" sheet; picking a row opens that turn. Session titles come with the list;
each row's position in its session loads as the row scrolls into view, and each row reserves its
lines so loading moves nothing. The list is
fixed: deciding a turn repaints its row, never moves or removes it.
-->
<script lang="ts" module>
	import type { TurnPosition } from './evalReview';

	export type RowInfo = { title: string; position: TurnPosition | null } | { error: string };
</script>

<script lang="ts">
	import { Check } from '@lucide/svelte';
	import type { EvalRunItemDto } from '$lib/api/evalRuns';
	import { ROW_LABEL, disputedAxes, goldOn, judgeOn } from './evalReview';
	import type { Axis, ReviewMode } from './evalReview';
	import { wordFor } from './labelControl';

	interface Props {
		items: EvalRunItemDto[];
		mode: ReviewMode;
		selected: number | null;
		info: ReadonlyMap<number, RowInfo>;
		onVisible: (turn: number) => void;
		onSelect: (turn: number) => void;
	}

	let { items, mode, selected, info, onVisible, onSelect }: Props = $props();

	function current(item: EvalRunItemDto, axis: Axis): string | null {
		return (axis === 'reaction' ? item.current_reaction : item.current_delivery) ?? null;
	}

	function pair(item: EvalRunItemDto, axis: Axis): string {
		const label = current(item, axis) ?? goldOn(item, axis);
		const judge = judgeOn(item, axis);
		const said = `the label says ${label ? wordFor(label) : '—'}`;
		return judge
			? `${said} · the judge says ${wordFor(judge)}`
			: `${said} · the judge did not label it`;
	}

	function decided(item: EvalRunItemDto): string | null {
		if (!item.reviewed_at) return null;
		const now = current(item, mode);
		return now ? wordFor(now) : null;
	}

	function position(turn: number): string {
		const found = info.get(turn);
		return found && 'position' in found && found.position
			? `Turn ${found.position.index} of ${found.position.total}`
			: 'Turn';
	}

	function title(item: EvalRunItemDto): string {
		const found = info.get(item.operator_turn_id);
		if (found && !('error' in found)) return found.title;
		if (item.session_uuid) return item.session_title || 'Untitled session';
		if (!found) return 'Loading session';
		return 'The session could not be loaded';
	}

	function visible(node: HTMLElement, turn: number) {
		if (typeof IntersectionObserver === 'undefined') {
			onVisible(turn);
			return;
		}
		const observer = new IntersectionObserver((entries) => {
			if (entries.some((e) => e.isIntersecting)) {
				onVisible(turn);
				observer.disconnect();
			}
		});
		observer.observe(node);
		return { destroy: () => observer.disconnect() };
	}
</script>

<ol class="etl" aria-label="Turns">
	{#each items as item (item.operator_turn_id)}
		{@const other = disputedAxes(item, mode).find((axis) => axis !== mode)}
		{@const word = decided(item)}
		<li use:visible={item.operator_turn_id}>
			<button
				type="button"
				class="etl-row"
				data-turn={item.operator_turn_id}
				aria-current={item.operator_turn_id === selected ? 'true' : undefined}
				onclick={() => onSelect(item.operator_turn_id)}
			>
				<span class="etl-title">{title(item)}</span>
				<span class="etl-done" data-on={Boolean(item.reviewed_at)}
					><Check size={12} aria-hidden="true" />{#if item.reviewed_at}<span
							>{word ?? 'decided'}</span
						>{/if}</span
				>
				<span class="etl-pair">{position(item.operator_turn_id)} · {pair(item, mode)}</span>
				{#if other}<span class="etl-other">{ROW_LABEL[other]}: {pair(item, other)}</span>{/if}
			</button>
		</li>
	{/each}
</ol>

<style>
	.etl {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
	}
	.etl li + li {
		border-top: var(--hairline) solid var(--border-subtle);
	}
	.etl-row {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		align-items: baseline;
		gap: var(--space-1) var(--space-3);
		inline-size: 100%;
		padding: var(--space-2) var(--space-3);
		border: 0;
		background: none;
		color: var(--text-2);
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		text-align: left;
		cursor: pointer;
	}
	.etl-row:hover {
		background: var(--surface-2);
	}
	.etl-row[aria-current='true'] {
		background: var(--interactive-active-bg);
	}
	.etl-row:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}
	.etl-title {
		color: var(--text-1);
		font-size: var(--text-sm);
		overflow-wrap: anywhere;
	}
	.etl-row[aria-current='true'] .etl-title {
		color: var(--interactive-active-fg);
	}
	.etl-pair,
	.etl-other {
		grid-column: 1 / 3;
		overflow-wrap: anywhere;
	}
	.etl-done {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		color: var(--status-success-fg);
		visibility: hidden;
	}
	.etl-done[data-on='true'] {
		visibility: visible;
	}
</style>
