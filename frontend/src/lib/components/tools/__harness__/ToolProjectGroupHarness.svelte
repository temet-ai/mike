<script lang="ts">
	import ToolProjectGroup, { type ToolProjectGroupRow } from '../ToolProjectGroup.svelte';

	interface Props {
		group: ToolProjectGroupRow;
		now: number;
	}

	let { group, now }: Props = $props();

	let collapsed = $state(false);
	let expandedKeys = $state<Set<string>>(new Set());
	let openedKeys = $state<Set<string>>(new Set());

	function isRowExpanded(key: string): boolean {
		return expandedKeys.has(key);
	}

	function onToggleRow(key: string): void {
		if (!openedKeys.has(key)) {
			// eslint-disable-next-line svelte/prefer-svelte-reactivity -- cloned, mutated, then reassigned wholesale; reactivity comes from the reassignment, not in-place mutation
			const seen = new Set(openedKeys);
			seen.add(key);
			openedKeys = seen;
		}
		expandedKeys = new Set(
			expandedKeys.has(key) ? [...expandedKeys].filter((v) => v !== key) : [...expandedKeys, key]
		);
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
