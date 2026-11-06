<script lang="ts">
	import { setContext } from 'svelte';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import { createCollapseStore } from '$lib/stores/collapseState.svelte';
	import type { OperatorTurnDto, ToolUseDto } from '$lib/types';
	import TurnCard from '../TurnCard.svelte';
	import SessionEventRow from '../SessionEventRow.svelte';

	interface Props {
		turn: OperatorTurnDto;
		index: number;
		isFirst: boolean;
		toolCalls: ToolUseDto[];
		ledgerUnavailable: boolean;
		live: boolean;
		now: number;
		defaultExpanded: boolean;
		query: string;
		toolFilter: string;
		forcedOpen: boolean | null;
		flash: boolean;
		jumpTargetRequestId: number | null;
		requestNumberFor: (id: number) => number | null;
		statusFor: (id: number) => string;
		onManualToggle: () => void;
		onOpenDrawer: (id: number, kind: 'request' | 'response') => void;
		onJumpSelf: () => void;
		eventTurn?: OperatorTurnDto | null;
	}

	let { eventTurn = null, ...rest }: Props = $props();

	setContext(COLLAPSE_STORE_KEY, createCollapseStore());
</script>

<TurnCard {...rest} />
{#if eventTurn}
	<SessionEventRow turn={eventTurn} query={rest.query} />
{/if}
