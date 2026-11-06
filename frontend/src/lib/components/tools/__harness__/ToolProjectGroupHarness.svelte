<script lang="ts">
	import ToolProjectGroup, { type ToolProjectGroupRow } from '../ToolProjectGroup.svelte';

	interface Props {
		group: ToolProjectGroupRow;
		now: number;
	}

	let { group, now }: Props = $props();

	let collapsed = $state(false);
	// eslint-disable-next-line svelte/prefer-svelte-reactivity -- cloned, mutated, then reassigned wholesale; reactivity comes from the reassignment, not in-place mutation
	let expandedKeys = $state<Set<string>>(new Set());
	let openedKeys = $state<Set<string>>(new Set());

	function isRowExpanded(key: string): boolean {
		return expandedKeys.has(key);
	}

	function onToggleRow(key: string): void {
		if (!openedKeys.has(key)) {
			const seen = new Set(openedKeys);
			seen.add(key);
			openedKeys = seen;
		}
		const next = new Set(expandedKeys);
		if (next.has(key)) next.delete(key);
		else next.add(key);
		expandedKeys = next;
	}

	function hasRowOpened(key: string): boolean {
		return openedKeys.has(key) || expandedKeys.has(key);
	}
</script>

<ToolProjectGroup
	{group}
	{now}
	{collapsed}
	onToggleCollapse={() => (collapsed = !collapsed)}
	{isRowExpanded}
	{onToggleRow}
	{hasRowOpened}
/>
