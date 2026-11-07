<script lang="ts">
	import { resolve } from '$app/paths';
	import { Button } from '$lib/components/ui/button';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import TestRunCard from '$lib/components/judges/TestRunCard.svelte';
	import {
		LABELLING_QUERY,
		QUESTION_NAME,
		gateFor,
		currentRunFor,
		questionScore,
		runName,
		runRole,
		runWhen,
		sessionScore
	} from '$lib/components/judges/judges';
	import type { QuestionKey } from '$lib/components/judges/judges';
	import type { TestRunPageData } from './+page';

	let { data }: { data: TestRunPageData } = $props();

	const run = $derived(data.run);
	const role = $derived(run ? runRole(run) : 'turn_judge');
	const unit = $derived(role === 'session_judge' ? 'sessions' : 'turns');
	const inUse = $derived(run !== null && run.bundle_id === data.liveId);
	const inUseRun = $derived(
		run && !inUse && data.liveId > 0 ? currentRunFor(data.runs, data.liveId) : null
	);
	const keys: QuestionKey[] = ['reaction', 'delivery'];

	function runHref(id: number): string {
		return resolve(
			`sessions/labelling/accuracy/${id}` as `sessions/labelling/accuracy/${string}`
		);
	}

	const useHref = $derived(
		run
			? `${resolve('/sessions/labelling')}${LABELLING_QUERY[role] || '?'}${LABELLING_QUERY[role] ? '&' : ''}edit=${run.bundle_id}`
			: null
	);

	function score(r: NonNullable<typeof run>, key: QuestionKey): string {
		return questionScore(r, key)?.percent ?? 'not answered';
	}
</script>

<svelte:head
	><title>{run ? runName(run) : 'Labelling accuracy'} — Sessions — mike</title></svelte:head
>

<div class="tr-scroll">
	<div class="tr-page">
		<header class="tr-head">
			<div class="tr-title-row">
				<h1 class="tr-h1">
					{run ? `Labelling accuracy, ${runWhen(run)}` : 'Labelling accuracy'}
				</h1>
				<span class="tr-all"
					><Button variant="outline" href={resolve('/sessions/labelling/accuracy')}
						>All accuracy runs</Button
					></span
				>
			</div>
			<p class="tr-lede">
				This page says what this accuracy run measured and how often the judge gave the right label.
				In an accuracy run the judge labels {unit} whose right labels are already known, and every answer
				is compared with that label.
			</p>
			{#if role === 'turn_judge'}
				<p class="tr-lede">
					Where the judge's answer and the label differ, one of them is wrong: Review labels opens
					those turns one at a time, so you can say which label is right.
				</p>
			{/if}
		</header>

		{#if !run}
			<LoadErrorBanner
				title="This accuracy run could not be loaded"
				detail={data.error ?? 'not found'}
			/>
		{:else}
			<TestRunCard
				{run}
				bundle={data.bundles.find((b) => b.id === run.bundle_id) ?? null}
				liveBundleId={data.liveId}
				gate={gateFor(data.gates, run.id)}
				counts={data.counts}
				gateOpen
				others={data.runs}
			/>

			<section class="tr-card edge mech-group" aria-label="Compared with the configuration in use">
				<div class="band-hd band-hd--static">
					<h2 class="machine tr-label">Compared with the configuration in use</h2>
				</div>
				<div class="band-body tr-body" data-testid="comparison">
					{#if inUse}
						<p class="tr-text">
							Configuration {run.bundle_id}, which this run measured, is the one in use.
						</p>
					{:else if data.liveId === 0}
						<p class="tr-text">No configuration is in use for this judge.</p>
					{:else if !inUseRun && role === 'turn_judge'}
						<p class="tr-text">
							Configuration {data.liveId} is in use and has no finished accuracy run to compare with.
						</p>
					{:else if role === 'session_judge'}
						<p class="tr-text">
							Configuration {data.liveId} is in use. This run's configuration {run.bundle_id} agreed on
							{sessionScore(run)?.percent ?? 'no score'} of sessions.
						</p>
					{:else if inUseRun}
						<table class="tr-table">
							<thead>
								<tr>
									<th class="machine" scope="col">Question</th>
									<th class="machine" scope="col">This run: configuration {run.bundle_id}</th>
									<th class="machine" scope="col"
										>In use: configuration {data.liveId}, measured
										<a class="tr-link" href={runHref(inUseRun.id)}>{runWhen(inUseRun)}</a></th
									>
								</tr>
							</thead>
							<tbody>
								{#each keys as key (key)}
									<tr>
										<th class="machine" scope="row">{QUESTION_NAME[key]}</th>
										<td>{score(run, key)}</td>
										<td>{score(inUseRun, key)}</td>
									</tr>
								{/each}
							</tbody>
						</table>
					{/if}
					{#if !inUse && run.status === 'complete' && useHref}
						<div class="tr-use">
							<Button variant="outline" href={useHref}>Use this run's configuration</Button>
							<span class="tr-hint"
								>Opens the change step on the Labelling page, where you confirm it.</span
							>
						</div>
					{/if}
				</div>
			</section>
		{/if}
	</div>
</div>

<style>
	.tr-scroll {
		height: 100%;
		min-height: 0;
		overflow-y: auto;
		background: var(--surface-bg);
		container-type: inline-size;
	}
	.tr-page {
		padding: var(--space-5) var(--space-6) var(--space-8);
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		max-width: 1200px;
		margin: 0 auto;
	}
	.tr-head {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.tr-title-row {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-3);
	}
	.tr-h1 {
		margin: 0;
		font-family: var(--font-display);
		font-size: var(--text-2xl);
		font-weight: var(--weight-display);
		line-height: var(--leading-tight);
		letter-spacing: var(--track-display);
		color: var(--text-strong);
	}
	.tr-all {
		margin-left: auto;
	}
	.tr-lede {
		margin: 0;
		max-width: 80ch;
		font-size: var(--text-md);
		line-height: var(--leading-body);
		color: var(--text-2);
	}
	.tr-lede:first-of-type {
		color: var(--text-1);
	}
	.tr-card {
		--edge: var(--edge-sm);
	}
	.tr-label {
		margin: 0;
		font-size: var(--text-xs);
		font-weight: var(--weight-body);
		color: var(--text-1);
	}
	.tr-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}
	.tr-text {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	.tr-table {
		border-collapse: collapse;
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	.tr-table th,
	.tr-table td {
		padding: var(--space-2) var(--space-5) var(--space-2) 0;
		text-align: left;
		vertical-align: baseline;
	}
	.tr-table th {
		font-size: var(--text-xs);
		font-weight: var(--weight-body);
		color: var(--text-2);
	}
	.tr-table tbody tr {
		border-top: var(--hairline) solid var(--border-subtle);
	}
	.tr-table td {
		font-family: var(--font-display);
		font-size: var(--text-lg);
		font-weight: var(--weight-display);
	}
	.tr-link {
		color: var(--interactive-active-fg);
		text-decoration: underline;
	}
	.tr-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.tr-use {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2) var(--space-3);
	}
	.tr-hint {
		font-size: var(--text-sm);
		color: var(--text-2);
	}
</style>
