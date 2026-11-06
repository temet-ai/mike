<script lang="ts">
	import SessionAnalysisBand from './SessionAnalysisBand.svelte';
	import ToolUsesPanel from './ToolUsesPanel.svelte';
	import type { Finding } from '$lib/types';

	interface Props {
		sessionId: string;
		now: number;
		findings: Finding[];
		findingsLoading: boolean;
		findingsError: string | null;
		analysisHref: string;
		grouped?: boolean;
	}

	let {
		sessionId,
		now,
		findings,
		findingsLoading,
		findingsError,
		analysisHref,
		grouped = false
	}: Props = $props();
</script>

{#if grouped}
	<SessionAnalysisBand {findings} loading={findingsLoading} error={findingsError} {analysisHref} />
	<ToolUsesPanel {sessionId} {now} embedded />
{:else}
	<div class="atc-card" data-augmented-ui="tl-clip br-clip border">
		<SessionAnalysisBand
			{findings}
			loading={findingsLoading}
			error={findingsError}
			{analysisHref}
		/>
		<ToolUsesPanel {sessionId} {now} embedded />
	</div>
{/if}

<style>
	.atc-card {
		--edge: 8px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		background: var(--surface-card);
		overflow: hidden;
	}

	.atc-card > :global(.dsc-elev) {
		border-top: var(--hairline, 1px) solid var(--border-subtle);
	}
</style>
