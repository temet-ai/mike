<!--
@component
What is waiting on you, as three tabs with counts: Lessons, Nudges and Directives. The tabs are
always rendered, even when every count is 0; each panel carries its own empty line. Pure layout
over three already-resolved panels passed as snippets. The caller owns which tab is open (it keeps
it in the URL).
-->
<script module lang="ts">
	export type WaitingTab = 'lessons' | 'nudges' | 'directives';

	export const WAITING_TABS: readonly WaitingTab[] = ['lessons', 'nudges', 'directives'];

	export function isWaitingTab(value: string | null): value is WaitingTab {
		return value !== null && (WAITING_TABS as readonly string[]).includes(value);
	}
</script>

<script lang="ts">
	import type { Snippet } from 'svelte';
	import * as Tabs from '$lib/components/ui/tabs';

	interface Props {
		counts: { lessons: number; nudges: number; directives: number };
		tab: WaitingTab;
		lessons: Snippet;
		nudges: Snippet;
		directives: Snippet;
	}

	let { counts, tab = $bindable(), lessons, nudges, directives }: Props = $props();
</script>

<div class="wfy" data-slot="waiting-for-you">
	<Tabs.Root
		bind:value={
			() => tab,
			(v: string) => {
				if (isWaitingTab(v)) tab = v;
			}
		}
	>
		<div class="wfy-bar">
			<Tabs.List aria-label="What is waiting for you">
				<Tabs.Trigger value="lessons"
					>Lessons<span class="wfy-count">{counts.lessons}</span></Tabs.Trigger
				>
				<Tabs.Trigger value="nudges"
					>Nudges<span class="wfy-count">{counts.nudges}</span></Tabs.Trigger
				>
				<Tabs.Trigger value="directives"
					>Directives<span class="wfy-count">{counts.directives}</span></Tabs.Trigger
				>
			</Tabs.List>
		</div>
		<Tabs.Content value="lessons">{@render lessons()}</Tabs.Content>
		<Tabs.Content value="nudges">{@render nudges()}</Tabs.Content>
		<Tabs.Content value="directives">{@render directives()}</Tabs.Content>
	</Tabs.Root>
</div>

<style>
	.wfy {
		min-width: 0;
	}
	.wfy-bar {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-end;
		gap: var(--space-2) var(--space-4);
		border-bottom: var(--hairline) solid var(--border-subtle);
	}
	.wfy-bar :global([data-slot='tabs-list']) {
		border-bottom: 0;
	}
	.wfy-count {
		display: inline-block;
		min-width: 2ch;
		margin-left: var(--space-2);
		color: var(--text-2);
		text-align: right;
		font-variant-numeric: tabular-nums;
	}
</style>
