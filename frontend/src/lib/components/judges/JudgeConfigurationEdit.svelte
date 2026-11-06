<!--
@component
The one place a judge's configuration is changed. It lists every configuration with its newest
labelling accuracy and whether it passed the accuracy bar set for being put in use, asks to confirm the change in words, and after saving shows how to judge the
older turns again with the new configuration. The re-judge is started from the command line: the
inspect API has no endpoint that starts one, so the step names the command instead of a button that
cannot work.
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import { resolve } from '$app/paths';
	import type { BundleDto, EvalGateDto, EvalRunDto } from '$lib/api/evalRuns';
	import * as Dialog from '$lib/components/ui/dialog';
	import * as RadioGroup from '$lib/components/ui/radio-group';
	import { Button } from '$lib/components/ui/button';
	import {
		JUDGE_NAME,
		QUESTION_NAME,
		configurationLine,
		gateForBundle,
		gateWords,
		currentRunFor,
		questionScore,
		runWhen,
		sessionScore
	} from './judges';
	import type { JudgeRole } from './judges';

	interface Props {
		role: JudgeRole;
		bundles: BundleDto[];
		liveId: number;
		runs: EvalRunDto[];
		gates: EvalGateDto[];
		open: boolean;
		initial: number | null;
		onConfirm: (id: number) => Promise<void>;
	}

	let {
		role,
		bundles,
		liveId,
		runs,
		gates,
		open = $bindable(false),
		initial,
		onConfirm
	}: Props = $props();

	let choice = $state<number | null>(null);
	let phase = $state<'choose' | 'saving' | 'done'>('choose');
	let error = $state<string | null>(null);

	const unit = $derived(role === 'session_judge' ? 'sessions' : 'turns');
	const judge = $derived(JUDGE_NAME[role].toLowerCase());
	const rejudge = $derived(
		role === 'session_judge'
			? 'mike eval rejudge --role session_judge --execute'
			: 'mike eval rejudge --execute'
	);
	const ordered = $derived([...bundles].sort((a, b) => b.id - a.id));
	const changed = $derived(choice !== null && choice !== liveId);

	$effect(() => {
		if (!open) return;
		untrack(() => {
			choice = initial ?? (liveId > 0 ? liveId : (ordered[0]?.id ?? null));
			phase = 'choose';
			error = null;
		});
	});

	function scoreWords(run: EvalRunDto): string {
		if (role === 'session_judge') {
			const s = sessionScore(run);
			return s ? `agreed on ${s.agreed} of ${s.n}` : 'no score';
		}
		return (['reaction', 'delivery'] as const)
			.map((k) => `${QUESTION_NAME[k]} ${questionScore(run, k)?.percent ?? 'not answered'}`)
			.join(', ');
	}

	async function confirm() {
		if (choice === null || !changed) return;
		phase = 'saving';
		error = null;
		try {
			await onConfirm(choice);
			phase = 'done';
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
			phase = 'choose';
		}
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="jce">
		<Dialog.Header>
			<Dialog.Title>Change the {judge}'s configuration</Dialog.Title>
			<Dialog.Description>
				{#if phase === 'done'}
					Configuration {choice} now labels new {unit}.
				{:else}
					Choose the configuration that labels new {unit}. Each one shows its newest labelling
					accuracy and whether it passed the accuracy bar set for being put in use.
				{/if}
			</Dialog.Description>
		</Dialog.Header>

		{#if phase === 'done'}
			<div class="jce-done" data-testid="rejudge-next">
				<p class="jce-text">
					{unit === 'turns' ? 'Turns' : 'Sessions'} judged before keep their labels until they are judged
					again. To judge them again with configuration {choice}, run this on the command line:
				</p>
				<code class="jce-cmd">{rejudge}</code>
				<p class="jce-text">Its progress then shows on this page.</p>
			</div>
			<Dialog.Footer>
				<Button variant="outline" onclick={() => (open = false)}>Close</Button>
			</Dialog.Footer>
		{:else}
			<div class="jce-list">
				<span id="jce-legend" class="machine jce-legend">Configurations</span>
				<RadioGroup.Root
					class="mech-group jce-options"
					name="configuration"
					value={choice === null ? '' : String(choice)}
					onValueChange={(next) => (choice = Number(next))}
					aria-labelledby="jce-legend"
				>
					{#each ordered as bundle (bundle.id)}
						{@const run = currentRunFor(runs, bundle.id)}
						<RadioGroup.Item class="jce-option" data-config={bundle.id} value={String(bundle.id)}>
							{#snippet label()}
								Configuration {bundle.id}{#if bundle.id === liveId}<span class="jce-in-use">
										· in use</span
									>{/if}
							{/snippet}
							<p class="jce-line">{configurationLine(bundle)}</p>
							<p class="jce-line">
								{#if run}Newest accuracy run, <a
										class="jce-link"
										href={resolve(
											`/sessions/labelling/accuracy/${run.id}` as `/sessions/labelling/accuracy/${string}`
										)}>{runWhen(run)}</a
									>: {scoreWords(run)}.{:else}Accuracy not measured yet.{/if}
							</p>
							<p class="jce-line">{gateWords(gateForBundle(gates, runs, bundle.id))}</p>
						</RadioGroup.Item>
					{/each}
				</RadioGroup.Root>
			</div>
			<p class="jce-confirm" data-testid="confirm-words">
				{#if changed}
					Makes configuration {choice} the one that labels new {unit}, from the next sweep. After
					the change, this step shows how to judge the older {unit} again with it.
				{:else}
					Configuration {liveId} is already in use.
				{/if}
			</p>
			{#if error}<p class="jce-error" role="alert">Not changed: {error}</p>{/if}
			<Dialog.Footer>
				<Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
				<Button variant="default" disabled={!changed || phase === 'saving'} onclick={confirm}
					>{phase === 'saving' ? 'Saving' : `Use configuration ${choice ?? ''}`}</Button
				>
			</Dialog.Footer>
		{/if}
	</Dialog.Content>
</Dialog.Root>

<style>
	:global([data-slot='dialog-content'].jce) {
		max-inline-size: min(44rem, calc(100vw - 2 * var(--space-4)));
	}
	.jce-list {
		display: flex;
		flex-direction: column;
		min-block-size: 0;
	}
	.jce-list :global(.jce-options) {
		max-block-size: 50vh;
		overflow-y: auto;
	}
	.jce-legend {
		margin-bottom: var(--space-2);
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.jce-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.jce-in-use {
		color: var(--status-success-fg);
	}
	.jce-line {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
		overflow-wrap: anywhere;
	}
	.jce-link {
		color: var(--interactive-active-fg);
		text-decoration: underline;
	}
	.jce-confirm,
	.jce-text {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	.jce-error {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--status-danger-fg);
	}
	.jce-done {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.jce-cmd {
		padding: var(--space-2) var(--space-3);
		background: var(--surface-bg);
		border: var(--hairline) solid var(--border-subtle);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--text-1);
		overflow-wrap: anywhere;
	}
</style>
