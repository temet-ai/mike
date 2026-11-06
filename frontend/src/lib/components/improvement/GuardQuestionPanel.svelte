<!--
@component
The rail card for one turn-guard question on `/sessions/improvement/guard`: whether it is live and
what that means, three readouts (fired in the last 24 hours, held back to compare, delivered as
usual) with the holdback's progress toward the read point, the honest holdback-vs-treated effect
verdict, and what marking a nudge does. One `edge mech-group`. The question's nudges are NOT here:
the page lists them beside this card, so no frame nests inside this one.

Built against the real landed API (`GET /api/steering/guard/effect` for the effect estimate,
`GET /api/steering/guard` for mode and the 24-hour fire count). The question's own labelled
precision and relapse-evidence progress have no confirmed data source yet and are left out rather
than fabricated. See artifacts/2026-09-29/judge-pages-redesign/SPEC.md §2.
-->
<script lang="ts">
	import InstrumentReadout from '$lib/components/shared/InstrumentReadout.svelte';
	import { guardQuestionLabel } from '$lib/api/guardEffect';
	import type { GuardEffectEstimate } from '$lib/api/guardEffect';
	import type { GuardMode } from '$lib/api/guard';

	interface Props {
		questionKey: string;
		mode: GuardMode;
		counts24h: { rows: number; wouldFire: number } | null;
		effect: GuardEffectEstimate;
		readPointHoldback: number;
	}

	let { questionKey, mode, counts24h, effect, readPointHoldback }: Props = $props();

	// Wording per ui-designer (design-spec.md §4.7): the model gets the new turn or the added
	// note, not the reader. `append`/`wake` are the real mode values (GuardMode in
	// $lib/api/guard.ts); a question's mode decides which single channel it uses.
	const MODE_HEADING: Record<GuardMode, string> = {
		off: 'Off',
		shadow: 'Tried quietly',
		append: 'Live — adds a note to the reply',
		wake: 'Live — starts a new turn'
	};
	const MODE_TEXT: Record<GuardMode, string> = {
		off: 'This question is off.',
		shadow:
			"This question is being tried quietly — it doesn't yet start a turn for the model or change what you see.",
		append: "This question is live — it can add a note to the model's reply.",
		wake: 'This question is live — it can start a new turn for the model.'
	};

	function pct(p: number | null): string {
		return p === null ? '—' : `${Math.round(p * 100)}`;
	}

	// Arm-form wording only — never "before"/"after" — per the coordinator's ruling: a guard
	// question's effect is the holdback-vs-treated comparison, never a before/after share.
	function effectText(): { text: string; tone: 'success' | 'neutral' } {
		const { primary, status } = effect;
		if (status === 'monitoring') {
			return {
				text: `Not enough data yet (${primary.holdback.n} of ${readPointHoldback} holdback events collected so far).`,
				tone: 'neutral'
			};
		}
		const { holdback, treated, rd, ci } = primary;
		const fewerCorrections = rd !== null && rd > 0 && ci !== null && ci[0] > 0;
		const ciText = ci ? ` (${pct(ci[0])}–${pct(ci[1])} percentage points)` : '';
		if (fewerCorrections) {
			return {
				text:
					`Fewer corrections in the treated arm: ${pct(treated.pBad)}% of ${treated.n} fires ` +
					`delivered as usual, against ${pct(holdback.pBad)}% of ${holdback.n} held back — both ` +
					`past the ${readPointHoldback}-event read point this needed. The difference is unlikely ` +
					`to be chance${ciText}.`,
				tone: 'success'
			};
		}
		return {
			text:
				`No change we can see: ${pct(treated.pBad)}% delivered as usual, ${pct(holdback.pBad)}% held ` +
				`back (${treated.n} and ${holdback.n} fires, both past the ${readPointHoldback}-event read ` +
				`point this needed). The difference could be chance${ciText}.`,
			tone: 'neutral'
		};
	}

	const rendered = $derived(effectText());
	const label = $derived(guardQuestionLabel(questionKey));
	const holdbackPercent = $derived(
		readPointHoldback > 0 ? Math.min(100, (effect.primary.holdback.n / readPointHoldback) * 100) : 0
	);
</script>

<section class="gqp edge mech-group" aria-label={label}>
	<div class="band-hd band-hd--static">
		<h2 class="machine gqp-title">{MODE_HEADING[mode]}</h2>
	</div>

	<div class="band-body gqp-body">
		<p class="gqp-line">{MODE_TEXT[mode]}</p>

		<div class="gqp-readouts">
			<InstrumentReadout label="Fired, last 24 hours" value={counts24h?.wouldFire ?? '—'} />
			<InstrumentReadout
				label="Held back to compare"
				value={`${effect.primary.holdback.n} / ${readPointHoldback}`}
			/>
			<InstrumentReadout label="Delivered as usual" value={effect.primary.treated.n} />
		</div>

		<span class="gqp-track" aria-hidden="true"
			><span class="gqp-fill" style:width={`${holdbackPercent}%`}></span></span
		>

		<p class="gqp-effect" class:gqp-effect--success={rendered.tone === 'success'}>
			{rendered.text}
		</p>
	</div>

	<div class="band-body">
		<p class="gqp-marks" data-testid="guard-marks-effect">
			Marking a nudge right or wrong tells its question how often it is right. If too many are
			wrong, the question goes back to being tried quietly.
		</p>
	</div>
</section>

<style>
	.gqp {
		min-width: 0;
	}
	.gqp-title {
		margin: 0;
		font-size: var(--text-sm);
		font-weight: var(--weight-body);
		color: var(--text-1);
	}
	.gqp-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}
	.gqp-line {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	.gqp-readouts {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: var(--space-3);
	}
	.gqp-track {
		display: block;
		height: 4px;
		background: var(--surface-2);
	}
	.gqp-fill {
		display: block;
		height: 100%;
		background: var(--status-success-fg);
	}
	.gqp-effect {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	.gqp-effect--success {
		color: var(--status-success-fg);
	}
	.gqp-marks {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
</style>
