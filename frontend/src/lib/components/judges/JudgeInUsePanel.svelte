<!--
@component
What a judge runs with right now, read-only: the configuration in use, its model and rules, when it
judges, whether it passed the accuracy bar set for being put in use, and whether the OpenRouter key it needs is set up. Changing the
configuration happens only through the panel's Edit button, which opens the change-and-confirm step.
-->
<script lang="ts">
	import { resolve } from '$app/paths';
	import { Pencil } from '@lucide/svelte';
	import type { BundleDto, EvalGateDto, EvalRunDto } from '$lib/api/evalRuns';
	import { Button } from '$lib/components/ui/button';
	import { formatWhen } from '$lib/components/eval/evalReview';
	import { gateForBundle, inUseBarWords, modelOf, rubricDate, thinkingOf } from './judges';
	import type { JudgeRole } from './judges';

	interface Props {
		role: JudgeRole;
		bundle: BundleDto | null;
		liveId: number;
		runs: EvalRunDto[];
		gates: EvalGateDto[];
		when: string | null;
		keyConfigured: boolean | null;
		keyError: string | null;
		canEdit: boolean;
		onEdit: () => void;
	}

	let { bundle, liveId, runs, gates, when, keyConfigured, keyError, canEdit, onEdit }: Props =
		$props();

	const model = $derived(
		[modelOf(bundle), thinkingOf(bundle)].filter((x): x is string => x !== null).join(', ')
	);
	const rules = $derived(rubricDate(bundle?.components?.rubric_version));
	const since = $derived(formatWhen(bundle?.promoted_at ?? null));
	const gate = $derived(liveId > 0 ? gateForBundle(gates, runs, liveId) : null);
	const early = $derived(Boolean(bundle?.override_reason));
</script>

<section class="jiu edge mech-group" aria-label="In use now" data-testid="in-use">
	<div class="band-hd band-hd--static">
		<h2 class="machine jiu-title">In use now</h2>
		<span class="jiu-edit">
			<Button variant="outline" size="sm" disabled={!canEdit} onclick={onEdit}
				><Pencil aria-hidden="true" />Edit</Button
			>
		</span>
	</div>
	<div class="band-body">
		{#if liveId === 0}
			<p class="jiu-empty">
				No configuration is in use; the judge uses the model and instructions set below.
			</p>
		{/if}
		<dl class="jiu-facts">
			{#if liveId > 0}
				<dt class="machine">Configuration</dt>
				<dd data-testid="in-use-config">
					{liveId}{#if since && !early}, in use since {since}{/if}
				</dd>
				{#if model}
					<dt class="machine">Model</dt>
					<dd>{model}</dd>
				{/if}
				{#if rules}
					<dt class="machine">Rules</dt>
					<dd>Dated {rules}</dd>
				{/if}
			{/if}
			{#if when}
				<dt class="machine">When it judges</dt>
				<dd>{when}</dd>
			{/if}
			{#if liveId > 0}
				<dt class="machine">Accuracy bar</dt>
				<dd data-testid="in-use-bar">{inUseBarWords(bundle, gate)}</dd>
			{/if}
			<dt class="machine">OpenRouter key</dt>
			<dd>
				{#if keyError}could not check ({keyError}){:else if keyConfigured === null}checking{:else if keyConfigured}set
					up{:else}not set up{/if} ·
				<a class="jiu-link" href={resolve('/settings/integrations')}>Integrations</a>
			</dd>
		</dl>
	</div>
</section>

<style>
	.jiu {
		--edge: var(--edge-sm);
		min-width: 0;
	}
	.jiu-title {
		margin: 0;
		font-size: var(--text-xs);
		font-weight: var(--weight-body);
		color: var(--text-1);
	}
	.jiu-edit {
		margin-left: auto;
	}
	.jiu-empty,
	.jiu-facts {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr);
		gap: var(--space-2) var(--space-4);
		margin: 0;
		font-size: var(--text-sm);
	}
	.jiu-facts dt {
		font-size: var(--text-xs);
		color: var(--text-2);
		align-self: baseline;
	}
	.jiu-facts dd {
		margin: 0;
		color: var(--text-1);
		overflow-wrap: anywhere;
	}
	.jiu-link {
		color: var(--interactive-active-fg);
		text-decoration: underline;
		text-underline-offset: 2px;
	}
	.jiu-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
</style>
