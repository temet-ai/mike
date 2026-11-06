<script lang="ts">
	import Badge from '$lib/components/shared/Badge.svelte';
	import DeliveryRequestLink from '$lib/components/memory/DeliveryRequestLink.svelte';
	import type { InjectionStage, SessionDelivery } from '$lib/api/sessionDelivery';

	interface Props {
		sessionId: string;
		data: SessionDelivery;
	}

	let { sessionId, data }: Props = $props();

	const applied = $derived(data.injection_stages.filter((stage) => stage.outcome === 'applied'));
	const notApplied = $derived(data.injection_stages.filter((stage) => stage.outcome !== 'applied'));

	function reasonLabel(stage: InjectionStage): string {
		return stage.reason_code.replaceAll('_', ' ');
	}
</script>

<div class="sds" data-slot="session-delivery-stages">
	<section class="sds-section" aria-label="Changes applied to requests">
		<h4 class="sds-heading">Applied to requests ({applied.length})</h4>
		{#if applied.length === 0}
			<p class="sds-empty">No stage changed any request of this session.</p>
		{:else}
			<ul class="sds-list" role="list">
				{#each applied as stage (stage.stage + stage.reason_code)}
					<li class="sds-row" data-slot="session-delivery-stage" data-outcome="applied">
						<span class="sds-name">{stage.stage}</span>
						<Badge variant="chip-active">{reasonLabel(stage)}</Badge>
						<span class="sds-dim">{stage.request_count} requests</span>
						<DeliveryRequestLink {sessionId} requestId={stage.first_request_id} />
					</li>
				{/each}
			</ul>
		{/if}
	</section>

	<section class="sds-section" aria-label="Stages that left requests unchanged">
		<h4 class="sds-heading">Left requests unchanged ({notApplied.length})</h4>
		{#if notApplied.length === 0}
			<p class="sds-empty">Every recorded stage changed a request.</p>
		{:else}
			<ul class="sds-list" role="list">
				{#each notApplied as stage (stage.stage + stage.outcome + stage.reason_code)}
					<li class="sds-row" data-slot="session-delivery-stage" data-outcome={stage.outcome}>
						<span class="sds-name">{stage.stage}</span>
						<Badge variant={stage.outcome === 'failed_closed' ? 'chip-error' : 'chip-muted'}
							>{reasonLabel(stage)}</Badge
						>
						<span class="sds-dim">{stage.request_count} requests</span>
					</li>
				{/each}
			</ul>
		{/if}
	</section>

	<section class="sds-section" aria-label="What mike does not record">
		<h4 class="sds-heading">Not recorded ({data.evidence_gaps.length})</h4>
		<ul class="sds-list" role="list">
			{#each data.evidence_gaps as gap (gap.code)}
				<li class="sds-gap" data-slot="session-delivery-gap" data-code={gap.code}>
					{gap.detail}
				</li>
			{/each}
		</ul>
	</section>

	<p class="sds-note">
		Delivered means mike put the text into a request. It does not show that the model read it or
		followed it.
	</p>
</div>

<style>
	.sds {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	.sds-section {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.sds-heading {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-xs);
		font-weight: var(--weight-display);
		text-transform: uppercase;
		letter-spacing: var(--track-display);
		color: var(--text-2);
	}
	.sds-empty,
	.sds-note {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
		font-style: italic;
	}
	.sds-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.sds-row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
		font-size: var(--text-sm);
	}
	.sds-name {
		font-family: var(--font-mono, monospace);
		color: var(--text-1);
	}
	.sds-dim {
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.sds-gap {
		border: var(--hairline) solid var(--line);
		border-radius: var(--r-tol);
		padding: var(--space-2) var(--space-3);
		background: var(--surface-2);
		font-size: var(--text-sm);
		line-height: var(--leading-body);
		color: var(--text-1);
	}
</style>
