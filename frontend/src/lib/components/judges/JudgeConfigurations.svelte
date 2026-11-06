<!--
@component
Every configuration of one judge, read-only, closed by default: which one is in use, its model and
rules, and how often it gave the right label in its newest accuracy run (linked to that run). Changing which one is
in use happens through the "In use now" panel's Edit, never from this list.
-->
<script lang="ts">
	import { resolve } from '$app/paths';
	import type { BundleDto, EvalRunDto } from '$lib/api/evalRuns';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import {
		currentRunFor,
		modelOf,
		questionScore,
		rubricDate,
		runRole,
		runWhen,
		sessionScore,
		thinkingOf,
		twinNote
	} from './judges';

	interface Props {
		bundles: BundleDto[];
		liveId: number;
		runs: EvalRunDto[];
	}

	let { bundles, liveId, runs }: Props = $props();

	let open = $state(false);

	const ordered = $derived(
		[...bundles].sort((a, b) => (a.id === liveId ? -1 : b.id === liveId ? 1 : b.id - a.id))
	);

	function describe(bundle: BundleDto): string {
		const twin = twinNote(bundle, bundles, runs);
		if (twin) return twin;
		const rules = rubricDate(bundle.components?.rubric_version);
		return [modelOf(bundle), thinkingOf(bundle), rules ? `rules of ${rules}` : null]
			.filter((x): x is string => x !== null)
			.join(' · ');
	}

	function percent(run: EvalRunDto, key: 'reaction' | 'delivery'): string {
		return questionScore(run, key)?.percent ?? 'not answered';
	}
</script>

<section class="jcf edge mech-group" aria-label="Configurations" data-testid="configurations">
	<div class="band-hd-wrap">
		<button type="button" class="band-hd" aria-expanded={open} onclick={() => (open = !open)}>
			<DisclosureMarker {open} size={10} />
			<span class="machine jcf-title">Configurations · {bundles.length}</span>
		</button>
	</div>
	{#if open}
		<div class="band-body jcf-body">
			<table class="jcf-table">
				<thead>
					<tr>
						<th class="machine" scope="col">Configuration</th>
						<th class="machine" scope="col">Model and rules</th>
						<th class="machine" scope="col">Newest accuracy run</th>
						{#if bundles[0]?.role === 'session_judge'}
							<th class="machine" scope="col">How the session ended</th>
						{:else}
							<th class="machine" scope="col">How you reacted</th>
							<th class="machine" scope="col">What was delivered</th>
						{/if}
					</tr>
				</thead>
				<tbody>
					{#each ordered as bundle (bundle.id)}
						{@const run = currentRunFor(runs, bundle.id)}
						<tr data-config={bundle.id}>
							<td
								>{bundle.id}{#if bundle.id === liveId}<span class="jcf-in-use">
										· in use</span
									>{/if}</td
							>
							<td>{describe(bundle)}</td>
							{#if run}
								<td
									><a
										class="jcf-link"
										href={resolve(
											`/sessions/labelling/accuracy/${run.id}` as `/sessions/labelling/accuracy/${string}`
										)}>{runWhen(run)}</a
									>{#if run.split !== 'holdout' && run.label_count > 1}, on turns used while
										building the judge{/if}</td
								>
								{#if runRole(run) === 'session_judge'}
									<td>{sessionScore(run)?.percent ?? 'no score'}</td>
								{:else}
									<td>{percent(run, 'reaction')}</td>
									<td>{percent(run, 'delivery')}</td>
								{/if}
							{:else}
								<td colspan="3" class="jcf-none">not measured</td>
							{/if}
						</tr>
					{/each}
				</tbody>
			</table>
			<p class="jcf-note">
				Scores compare fairly only between runs on the same turns. A run on turns used while
				building the judge usually scores higher than the same configuration does on turns set aside
				for measuring.
			</p>
		</div>
	{/if}
</section>

<style>
	.jcf {
		--edge: var(--edge-sm);
		min-width: 0;
	}
	.jcf-title {
		font-size: var(--text-xs);
		color: var(--text-1);
	}
	.jcf-body {
		overflow-x: auto;
	}
	.jcf-table {
		inline-size: 100%;
		border-collapse: collapse;
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	.jcf-table th {
		padding: var(--space-1) var(--space-3) var(--space-2) 0;
		text-align: left;
		font-size: var(--text-xs);
		font-weight: var(--weight-body);
		color: var(--text-2);
		white-space: nowrap;
	}
	.jcf-table td {
		padding: var(--space-2) var(--space-3) var(--space-2) 0;
		border-top: var(--hairline) solid var(--border-subtle);
		vertical-align: baseline;
		overflow-wrap: anywhere;
	}
	.jcf-in-use {
		color: var(--status-success-fg);
	}
	.jcf-none {
		color: var(--text-2);
	}
	.jcf-note {
		margin: var(--space-2) 0 0;
		max-width: 80ch;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.jcf-link {
		color: var(--interactive-active-fg);
		text-decoration: underline;
		text-underline-offset: 2px;
	}
	.jcf-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
</style>
