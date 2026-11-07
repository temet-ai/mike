<script lang="ts">
	import type { OperatorTurnDto } from '$lib/types';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import Badge from '$lib/components/shared/Badge.svelte';
	import {
		verdictSource,
		sourceTip,
		reactionTip,
		deliveryTip,
		verdictHistoryHref
	} from './turnVerdictChips';

	let { turn }: { turn: OperatorTurnDto } = $props();

	const source = $derived(verdictSource(turn));
</script>

<span class="inline-flex items-center gap-1" data-verdict-source={source}>
	{#if turn.reaction}
		<Tooltip text={reactionTip(turn.reaction)}>
			<Badge variant="chip-muted" data-verdict-axis="reaction"
				>{turn.reaction.replaceAll('_', ' ')}</Badge
			>
		</Tooltip>
	{/if}
	{#if turn.delivery}
		<Tooltip text={deliveryTip(turn.delivery)}>
			<Badge variant="chip-muted" data-verdict-axis="delivery"
				>{turn.delivery.replaceAll('_', ' ')}</Badge
			>
		</Tooltip>
	{/if}
	<Tooltip text={sourceTip(source)}>
		<Badge variant={source === 'operator' ? 'chip-active' : 'chip-muted'} data-verdict-axis="source"
			>{source}</Badge
		>
	</Tooltip>
	<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- an inspect API URL opened as JSON, not an app route -->
	<a
		class="text-micro text-[var(--text-faint)] hover:text-[var(--text-strong)]"
		href={verdictHistoryHref(turn.turn_id)}
		target="_blank"
		rel="noopener noreferrer">verdict history</a
	>
</span>
