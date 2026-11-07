<!--
@component
The body of the Judge page for one judge (the turn judge or the session judge): a summary strip
under the caller's header (on or off, the configuration in use, how often it agrees with the labels,
open alarms, what judging costs), a closed "How the judge works" explanation, then its on/off switch, what it runs with now (with the Edit that changes the configuration), how often it
agrees with the labels, the alarms that concern it, a re-judge in progress, its configurations and
its settings. Everything about operating this judge is here and nowhere else.
-->
<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import type { Snippet } from 'svelte';
	import { resolve } from '$app/paths';
	import { ApiError } from '$lib/api';
	import { getJudgeCost } from '$lib/api/improvement';
	import type { JudgeCost } from '$lib/api/improvement';
	import { formatUsd } from '$lib/format';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import { disclose } from '$lib/components/ui/motion';
	import { acknowledgeEvalAlarm, getEvalAlarms } from '$lib/api/evalMetrics';
	import type { EvalAlarmDto } from '$lib/api/evalMetrics';
	import { getFeatures, getSettings, patchFeature, patchSetting } from '$lib/api/settings';
	import type { FeatureDto, SettingMeta, SettingValue } from '$lib/api/settings';
	import { getOpenRouterUsage } from '$lib/api/usage';
	import WorkerCard from '$lib/components/settings/WorkerCard.svelte';
	import EvalAlarmList from '$lib/components/settings/EvalAlarmList.svelte';
	import LabelDefinitions from '$lib/components/eval/LabelDefinitions.svelte';
	import { formatDate } from '$lib/components/eval/evalReview';
	import JudgeInUsePanel from './JudgeInUsePanel.svelte';
	import JudgeAgreementPanel from './JudgeAgreementPanel.svelte';
	import JudgeConfigurations from './JudgeConfigurations.svelte';
	import JudgeConfigurationEdit from './JudgeConfigurationEdit.svelte';
	import JudgeSettings from './JudgeSettings.svelte';
	import {
		SESSION_JUDGE_SETTING_KEYS,
		TURN_JUDGE_SETTING_KEYS,
		QUESTION_NAME,
		alarmsFor,
		currentRunFor,
		modelOf,
		questionScore,
		sessionScore
	} from './judges';
	import type { QuestionCounts, QuestionKey } from './judges';
	import { LIVE_KEY, disagreementCounts, liveIdFrom } from './judgePageData';
	import type { JudgePageData } from './judgePageData';

	let { data, head }: { data: JudgePageData; head?: Snippet } = $props();

	const role = untrack(() => data.role);
	const isTurn = role === 'turn_judge';
	const featureKey = isTurn ? 'judge_worker' : 'session_judge';

	let features = $state<FeatureDto[]>(untrack(() => data.features ?? []));
	let settings = $state<SettingMeta[]>(untrack(() => data.settings ?? []));
	let alarms = $state<EvalAlarmDto[]>(untrack(() => data.alarms?.alarms ?? []));
	let toggleError = $state<string | null>(null);
	let rowLoading = $state<Record<string, boolean>>({});
	let rowError = $state<Record<string, string | null>>({});
	let keyConfigured = $state<boolean | null>(null);
	let keyError = $state<string | null>(null);
	let editOpen = $state(false);
	let howOpen = $state(false);
	let cost = $state<JudgeCost | null>(null);
	let costError = $state<string | null>(null);
	let counts = $state<Partial<Record<QuestionKey, QuestionCounts>> | null>(
		untrack(() => data.counts)
	);
	let countsFor = $state<number | null>(
		untrack(() =>
			data.counts ? (currentRunFor(data.runs, liveIdFrom(data.settings, role))?.id ?? null) : null
		)
	);

	const bundles = $derived(data.bundles.filter((b) => b.role === role));
	const liveId = $derived(liveIdFrom(settings, role));
	const liveBundle = $derived(bundles.find((b) => b.id === liveId) ?? null);
	const measured = $derived(currentRunFor(data.runs, liveId));
	const mine = $derived(alarmsFor(alarms, data.bundles, isTurn ? 'turn' : 'session'));
	const rejudge = $derived(
		data.rejudgeRuns.find((r) => r.status === 'running' || r.status === 'queued') ?? null
	);
	const enabled = $derived(features.find((f) => f.key === featureKey)?.enabled ?? true);
	const applies = $derived(features.find((f) => f.key === featureKey)?.applies ?? null);
	const byKey = $derived(new Map(settings.map((s) => [s.key, s])));

	const liveModel = $derived(modelOf(liveBundle));
	const agreement = $derived.by((): { label: string; percent: string; of: string }[] => {
		if (isTurn) {
			return (['reaction', 'delivery'] as const).map((key) => {
				const score = measured ? questionScore(measured, key) : null;
				return {
					label: `Agrees on ${QUESTION_NAME[key].toLowerCase()}`,
					percent: score?.percent ?? '—',
					of: score ? `${score.agreed} of ${score.n} turns` : 'not measured yet'
				};
			});
		}
		const score = measured ? sessionScore(measured) : null;
		return [
			{
				label: 'Agrees on how the session ended',
				percent: score?.percent ?? '—',
				of: score ? `${score.agreed} of ${score.n} sessions` : 'not measured yet'
			}
		];
	});
	const runHref = $derived(
		measured
			? resolve(
					`sessions/labelling/accuracy/${measured.id}` as `sessions/labelling/accuracy/${string}`
				)
			: null
	);
	const weekSub = $derived.by((): string => {
		if (costError) return 'could not be loaded';
		if (cost === null) return '\u00a0';
		return `${cost.weekCalls.toLocaleString('en-US')} call${cost.weekCalls === 1 ? '' : 's'}`;
	});
	const monthSub = $derived.by((): string => {
		if (costError) return 'could not be loaded';
		if (cost === null) return '\u00a0';
		return cost.ceilingUsd === null
			? 'no ceiling set'
			: `of the ${formatUsd(cost.ceilingUsd)} ceiling`;
	});

	function num(key: string): number | null {
		const v = byKey.get(key)?.value;
		return typeof v === 'number' ? v : null;
	}

	const when = $derived.by(() => {
		if (isTurn) {
			if (byKey.get('turn_judge.immediate')?.value === true && liveId > 0) {
				const ms = num('turn_judge.immediate_delay_ms');
				return ms !== null ? `${ms / 1000} s after you reply` : 'Soon after you reply';
			}
			const every = num('judge_worker.interval_secs');
			const batch = num('judge_worker.batch_size');
			return every !== null && batch !== null ? `Every ${every} s, ${batch} turns at a time` : null;
		}
		const every = num('session_judge.poll_interval_secs');
		const batch = num('session_judge.batch_size');
		const quiet = num('session_judge.min_session_age_secs');
		if (every === null || batch === null) return null;
		return `Every ${every} s, ${batch} sessions at a time${quiet !== null ? `, once a session has been quiet for ${quiet} s` : ''}`;
	});

	const rejudgeWords = $derived.by(() => {
		if (!rejudge) return null;
		const unit = isTurn ? 'turns' : 'sessions';
		const head =
			rejudge.status === 'queued'
				? `Waiting to start with configuration ${rejudge.bundle_id}`
				: `With configuration ${rejudge.bundle_id}`;
		const parts = [
			`${head}: ${rejudge.judged.toLocaleString('en-US')} of ${rejudge.eligible_total.toLocaleString('en-US')} ${unit} done.`,
			`${rejudge.calls_today} of today's ${rejudge.daily_call_cap} calls used.`,
			`$${rejudge.cost_usd_so_far.toFixed(2)} spent so far${rejudge.estimated_usd !== null ? ` of about $${rejudge.estimated_usd.toFixed(2)}` : ''}.`
		];
		const finish = formatDate(rejudge.projected_finish_date);
		if (finish) parts.push(`Expected to finish ${finish}.`);
		return parts.join(' ');
	});

	$effect(() => {
		const run = measured;
		if (!isTurn || !run || run.id === countsFor) return;
		untrack(() => {
			countsFor = run.id;
			counts = null;
			disagreementCounts(run.id)
				.then((c) => {
					if (countsFor === run.id) counts = c;
				})
				.catch(() => undefined);
		});
	});

	onMount(() => {
		getOpenRouterUsage()
			.then((u) => (keyConfigured = u.configured))
			.catch((e) => (keyError = e instanceof ApiError ? e.message : String(e)));
		if (data.editId !== null) editOpen = true;
		getJudgeCost()
			.then((c) => (cost = c))
			.catch((e) => (costError = message(e)));
	});

	function message(e: unknown): string {
		return e instanceof ApiError ? e.message : e instanceof Error ? e.message : String(e);
	}

	async function toggle(next: boolean) {
		const previous = enabled;
		features = features.map((f) => (f.key === featureKey ? { ...f, enabled: next } : f));
		toggleError = null;
		try {
			const saved = await patchFeature(featureKey, next);
			features = features.map((f) => (f.key === featureKey ? { ...f, ...saved } : f));
		} catch (e) {
			features = features.map((f) => (f.key === featureKey ? { ...f, enabled: previous } : f));
			toggleError = message(e);
		}
	}

	async function save(key: string, value: SettingValue) {
		const previous = byKey.get(key);
		if (!previous) return;
		if (previous.kind === 'bool')
			settings = settings.map((s) => (s.key === key ? { ...s, value } : s));
		rowLoading = { ...rowLoading, [key]: true };
		rowError = { ...rowError, [key]: null };
		try {
			const saved = await patchSetting(key, value);
			settings = settings.map((s) => (s.key === key ? { ...s, ...saved } : s));
		} catch (e) {
			if (previous.kind === 'bool')
				settings = settings.map((s) => (s.key === key ? { ...s, value: previous.value } : s));
			rowError = { ...rowError, [key]: message(e) };
		} finally {
			rowLoading = { ...rowLoading, [key]: false };
		}
	}

	async function useConfiguration(id: number) {
		const saved = await patchSetting(LIVE_KEY[role], id);
		settings = settings.map((s) => (s.key === saved.key ? { ...s, ...saved } : s));
		try {
			settings = (await getSettings()).settings;
			features = await getFeatures();
		} catch {
			return;
		}
	}

	async function acknowledge(id: number, note: string) {
		await acknowledgeEvalAlarm(id, note);
		alarms = (await getEvalAlarms({ state: 'open' })).alarms;
	}
</script>

<div class="jh-scroll">
	<div class="jh" data-testid="judge-home">
		<div class="jh-head">
			{@render head?.()}
		</div>

		<div class="page-strip" data-slot="judge-strip" data-testid="judge-strip">
			<div class="page-strip__group">
				<p class="machine page-strip__cap" id="jh-strip-judge">This judge</p>
				<dl class="page-strip__facts" aria-labelledby="jh-strip-judge">
					<div class="page-strip__fact jh-fact-onoff">
						<dt>Labelling</dt>
						<dd class="page-strip__num" data-testid="strip-on">{enabled ? 'On' : 'Off'}</dd>
					</div>
					<div class="page-strip__fact">
						<dt>Configuration in use</dt>
						<dd class="page-strip__num" data-testid="strip-config">
							{liveId > 0 ? liveId : 'None'}
						</dd>
						<dd class="page-strip__sub">
							{liveId > 0 ? (liveModel ?? 'model not recorded') : 'uses the settings below'}
						</dd>
					</div>
					{#each agreement as fact (fact.label)}
						<div class="page-strip__fact" data-testid="strip-agreement">
							<dt>{fact.label}</dt>
							<dd class="page-strip__num">{fact.percent}</dd>
							<dd class="page-strip__sub">
								{fact.of}{#if runHref}{' · '}<a href={runHref}>latest accuracy run</a>{/if}
							</dd>
						</div>
					{/each}
					<div class="page-strip__fact">
						<dt>Needs attention</dt>
						<dd
							class="page-strip__num"
							class:page-strip__num--live={mine.length > 0}
							data-testid="strip-alarms"
						>
							{mine.length}
						</dd>
						<dd class="page-strip__sub">
							{#if mine.length > 0}<a href="#jh-alarms">See the alarms</a>{:else}nothing open{/if}
						</dd>
					</div>
				</dl>
			</div>
			<div class="page-strip__group">
				<p class="machine page-strip__cap" id="jh-strip-cost">What judging costs</p>
				<dl class="page-strip__facts" aria-labelledby="jh-strip-cost">
					<div class="page-strip__fact jh-fact-cost">
						<dt>Last 7 days</dt>
						<dd class="page-strip__num" data-testid="strip-cost-week">
							{cost ? formatUsd(cost.weekUsd) : '—'}
						</dd>
						<dd class="page-strip__sub">{weekSub}</dd>
					</div>
					<div class="page-strip__fact jh-fact-cost">
						<dt>Last 30 days</dt>
						<dd class="page-strip__num" data-testid="strip-cost-month">
							{cost ? formatUsd(cost.monthUsd) : '—'}
						</dd>
						<dd class="page-strip__sub">{monthSub}</dd>
					</div>
				</dl>
			</div>
		</div>

		<div class="jh-how">
			<button
				type="button"
				class="jh-how-toggle"
				aria-expanded={howOpen}
				aria-controls="jh-how-body"
				onclick={() => (howOpen = !howOpen)}
			>
				<DisclosureMarker open={howOpen} size={10} /><span>How the judge works</span>
			</button>
			{#if howOpen}
				<div class="jh-how-body" id="jh-how-body" transition:disclose>
					<div class="jh-how-inner">
						{#if isTurn}
							<p class="jh-lede">
								Turns are labelled by the turn judge, an AI model that reads every finished turn and
								labels two things: what the assistant delivered, and how you reacted in your next
								message. It never changes a running session.
							</p>
							<p class="jh-lede">
								Its accuracy is measured on a fixed set of turns that already have a label. That
								label was made beforehand, mostly by AI too: two AI models labelled each turn
								separately and a third settled where they differed, and some labels are ones you
								chose. The judge's answer is what the judge says about the same turn. Where the two
								differ, you decide which is right.
							</p>
						{:else}
							<p class="jh-lede">
								Sessions are labelled by the session judge, an AI model that reads each session once
								it has gone quiet and labels how it ended. It also writes the two-paragraph summary
								shown at the top of the session page. It never changes a running session.
							</p>
						{/if}
					</div>
				</div>
			{/if}
		</div>

		{#if data.errors.length > 0}
			<p class="jh-error" role="alert">
				Some of this page could not be loaded: {data.errors.join('; ')}
			</p>
		{/if}

		<h2 class="sr-only">Switch, configuration and settings</h2>

		<WorkerCard
			name={featureKey}
			label={isTurn ? 'Label new turns' : 'Label finished sessions'}
			description={keyConfigured === false
				? 'Set up your OpenRouter key in Integrations to switch it on.'
				: undefined}
			{applies}
			{enabled}
			disabled={keyConfigured === false}
			onToggle={(v) => void toggle(v)}
			error={toggleError}
			accent="teal"
		/>

		<div class="jh-panels">
			<JudgeInUsePanel
				{role}
				bundle={liveBundle}
				{liveId}
				runs={data.runs}
				gates={data.gates}
				{when}
				{keyConfigured}
				{keyError}
				canEdit={bundles.length > 0}
				onEdit={() => (editOpen = true)}
			/>
			<JudgeAgreementPanel {role} run={measured} {counts} />
		</div>

		{#if isTurn}
			<LabelDefinitions vocabulary={data.vocabulary} />
		{/if}
		{#if mine.length > 0}
			<div class="jh-anchor" id="jh-alarms">
				<EvalAlarmList
					alarms={mine}
					home={isTurn ? 'turn' : 'session'}
					onAcknowledge={acknowledge}
				/>
			</div>
		{/if}

		{#if rejudgeWords}
			<section class="jh-card edge mech-group" aria-label="Labelling older turns again">
				<div class="band-hd band-hd--static">
					<h2 class="machine jh-label">Labelling older {isTurn ? 'turns' : 'sessions'} again</h2>
				</div>
				<p class="band-body jh-text" data-testid="rejudge">{rejudgeWords}</p>
			</section>
		{/if}

		{#if bundles.length > 0}
			<JudgeConfigurations {bundles} {liveId} runs={data.runs} />
		{/if}

		<JudgeSettings
			keys={isTurn ? TURN_JUDGE_SETTING_KEYS : SESSION_JUDGE_SETTING_KEYS}
			modelKey={isTurn ? 'judge_worker.model' : 'session_judge.model'}
			promptKey={isTurn ? 'judge_worker.system_prompt' : 'session_judge.system_prompt'}
			{settings}
			{liveId}
			loading={rowLoading}
			errors={rowError}
			inheritLabel={isTurn ? "the steerer's model" : null}
			onSave={(k, v) => void save(k, v)}
		/>
	</div>
</div>

<JudgeConfigurationEdit
	{role}
	{bundles}
	{liveId}
	runs={data.runs}
	gates={data.gates}
	bind:open={editOpen}
	initial={data.editId}
	onConfirm={useConfiguration}
/>

<style>
	.jh-scroll {
		height: 100%;
		min-height: 0;
		overflow-y: auto;
		background: var(--surface-bg);
		container-type: inline-size;
	}
	.jh {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		padding: var(--space-5) var(--space-6) var(--space-8);
		max-width: 1600px;
	}
	.jh-head {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.jh-fact-onoff {
		min-inline-size: 5rem;
	}
	.jh-fact-cost {
		min-inline-size: 8rem;
	}
	.jh-how {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-width: 0;
	}
	.jh-how-toggle {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		align-self: flex-start;
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		font-size: var(--text-sm);
		color: var(--text-2);
		text-align: left;
		cursor: pointer;
	}
	.jh-how-toggle:hover {
		color: var(--text-1);
	}
	.jh-how-toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.jh-how-body {
		display: grid;
	}
	.jh-how-inner {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-height: 0;
		overflow: hidden;
	}
	.jh-lede {
		margin: 0;
		max-width: 80ch;
		font-size: var(--text-sm);
		line-height: var(--leading-body);
		color: var(--text-1);
	}
	.jh-anchor {
		scroll-margin-top: var(--space-4);
	}
	.jh-error {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--status-danger-fg);
	}
	.jh-panels {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: var(--space-4);
		align-items: start;
	}
	.jh-card {
		--edge: var(--edge-sm);
	}
	.jh-label {
		margin: 0;
		font-size: var(--text-xs);
		font-weight: var(--weight-body);
		color: var(--text-1);
	}
	.jh-text {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	@container (max-width: 820px) {
		.jh-panels {
			grid-template-columns: minmax(0, 1fr);
		}
	}
	@container (max-width: 719.98px) {
		.jh {
			padding: var(--space-4) var(--space-3) var(--space-6);
		}
	}
</style>
