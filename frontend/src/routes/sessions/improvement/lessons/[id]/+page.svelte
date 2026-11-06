<!--
@component
One lesson candidate or approved lesson: the rule text (editable while still raw), the turn it
came from, similar and reinforcing corrections, its injection/effect once live, and the decision.
See design-spec.md §6.
-->
<script lang="ts">
	import { ArrowLeft } from '@lucide/svelte';
	import { resolve } from '$app/paths';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import EvalQuote from '$lib/components/eval/EvalQuote.svelte';
	import { formatDay } from '$lib/components/eval/evalReview';
	import { Button } from '$lib/components/ui/button';
	import { disclose } from '$lib/components/ui/motion';
	import EffectVerdict from '$lib/components/improvement/EffectVerdict.svelte';
	import { approveLessonCandidate, rejectLessonCandidate } from '$lib/api/improvement';
	import type { EvidenceCorrection, LessonKind, RejectReason } from '$lib/api/improvement';
	import type { LessonDetailPageData } from './+page';

	const { data }: { data: LessonDetailPageData } = $props();

	const SOURCE_TEXT: Record<'judge' | 'directive', string> = {
		judge: 'the judge',
		directive: 'a directive you approved'
	};

	const KIND_WORD: Record<LessonKind, string> = {
		preference: 'Preference',
		correction: 'Correction',
		warning: 'Warning',
		convention: 'Convention'
	};

	const REASONS: { reason: RejectReason; text: string }[] = [
		{ reason: 'not_a_correction', text: "This wasn't something I was correcting" },
		{ reason: 'too_specific', text: 'Too specific to be a general rule' },
		{ reason: 'wrong', text: 'Just wrong' },
		{ reason: 'other', text: 'Something else' }
	];

	type OpenKey = 'rationale' | 'similar' | 'reinforcing';

	// editedText is user-editable scratch state seeded once from the loaded lesson text;
	// re-deriving from `data` would overwrite the operator's edit.
	// svelte-ignore state_referenced_locally
	let editedText = $state(data.lesson?.text ?? '');
	let open = $state<Record<OpenKey, boolean>>({ rationale: false, similar: true, reinforcing: true });
	let rejecting = $state(false);
	let showOtherNote = $state(false);
	let otherNote = $state('');
	let saving = $state<'approve' | RejectReason | null>(null);
	let decisionError = $state<string | null>(null);
	let decided = $state<'approved' | 'rejected' | null>(null);

	const textChanged = $derived((data.lesson?.text ?? '') !== editedText);
	const isEditable = $derived(data.lesson?.stage === 'raw' && decided === null);
	const title = $derived.by(() => {
		const stage = data.lesson?.stage;
		if (decided === 'approved') return 'Approved lesson';
		if (decided === 'rejected') return 'Rejected lesson';
		if (stage === 'raw') return 'Lesson waiting on you';
		if (stage === 'daily' || stage === 'long_term') return 'Approved lesson';
		if (stage === 'archived') return 'Rejected lesson';
		return 'Lesson';
	});
	const shownRule = $derived(decided === 'approved' ? editedText : (data.lesson?.text ?? ''));

	function resetText(): void {
		editedText = data.lesson?.text ?? '';
	}

	async function approve(): Promise<void> {
		if (!data.lesson) return;
		saving = 'approve';
		decisionError = null;
		try {
			await approveLessonCandidate(data.lesson.id, textChanged ? editedText : undefined);
			decided = 'approved';
			rejecting = false;
		} catch (e) {
			decisionError = e instanceof Error ? e.message : String(e);
		} finally {
			saving = null;
		}
	}

	function toggleReject(): void {
		rejecting = !rejecting;
		if (!rejecting) showOtherNote = false;
	}

	function toggleOpen(key: OpenKey): void {
		open[key] = !open[key];
	}

	function quoteHeading(ev: EvidenceCorrection): string {
		const day = formatDay(ev.createdAt);
		return day ? `You wrote, ${day}` : 'You wrote';
	}

	async function reject(reason: RejectReason): Promise<void> {
		if (!data.lesson) return;
		if (reason === 'other' && !showOtherNote) {
			showOtherNote = true;
			return;
		}
		saving = reason;
		decisionError = null;
		try {
			await rejectLessonCandidate(data.lesson.id, reason, reason === 'other' ? otherNote : undefined);
			decided = 'rejected';
			rejecting = false;
			showOtherNote = false;
		} catch (e) {
			decisionError = e instanceof Error ? e.message : String(e);
		} finally {
			saving = null;
		}
	}

	function evidenceKey(e: EvidenceCorrection): string {
		return e.turnHref + e.createdAt;
	}
</script>


<svelte:head><title>{title} — Sessions — mike</title></svelte:head>

{#snippet evidenceBand(key: 'similar' | 'reinforcing', label: string, items: EvidenceCorrection[])}
	{@const bodyId = `les-body-${key}`}
	<section aria-label={label}>
		<div class="band-hd-wrap">
			<button
				type="button"
				class="band-hd"
				aria-expanded={open[key]}
				aria-controls={bodyId}
				onclick={() => toggleOpen(key)}
			>
				<DisclosureMarker open={open[key]} size={10} />
				<span class="machine les-hd">{label}</span>
				<span class="band-hd__meta">{items.length}</span>
			</button>
		</div>
		{#if open[key]}
			<div class="les-disc" id={bodyId} transition:disclose>
				<div class="les-disc__in">
					{#each items as ev (evidenceKey(ev))}
						<div class="band-body les-ev">
							<a class="les-link les-ev-session" href={ev.turnHref}>{ev.sessionTitle}</a>
							<EvalQuote who="you" heading={quoteHeading(ev)}>
								<p class="les-quote">{ev.correctingQuote}</p>
							</EvalQuote>
							{#if ev.existingLessonText}
								<p class="les-note">
									Already a lesson:
									<a class="les-link" href={ev.existingLessonHref ?? '#'}>{ev.existingLessonText}</a>
								</p>
							{/if}
						</div>
					{/each}
				</div>
			</div>
		{/if}
	</section>
{/snippet}

<div class="old-scroll">
	<div class="old">
		<div class="old-head">
			{#if !data.error}
				<a class="les-back" href={resolve('/sessions/improvement')}
					><ArrowLeft size={12} aria-hidden="true" />What the judge did</a
				>
			{/if}
			<PageIntro
				level="h1"
				{title}
				description="Read where it came from, then approve it (edit the wording first if you want) or reject it."
			/>
		</div>

		{#if data.error}
			<LoadErrorBanner
				title="Could not load this lesson."
				detail={data.error}
				backHref="/sessions/improvement"
				backLabel="Back to what the judge did"
			/>
		{:else if data.lesson}
			{@const lesson = data.lesson}
			{@const proposedDay = formatDay(lesson.createdAt)}

			<div class="les-work">
				<div class="les-spine edge mech-group">
					<section aria-labelledby="les-from-hd">
						<div class="band-hd band-hd--static">
							<h2 class="machine les-hd" id="les-from-hd">Where it came from</h2>
							<span class="band-hd__meta les-meta"
								>{KIND_WORD[lesson.kind]} · proposed by {SOURCE_TEXT[lesson.source]}{proposedDay
									? ` · ${proposedDay}`
									: ''}</span
							>
						</div>
						<div class="band-body les-quotes">
							<EvalQuote who="you" heading="You asked">
								<p class="les-quote">{lesson.askText}</p>
							</EvalQuote>
							<EvalQuote who="claude" scroll heading="How the turn ended">
								<p class="les-quote">{lesson.assistantEndingText}</p>
							</EvalQuote>
							<EvalQuote who="you" key heading="Your correction">
								{#snippet meta()}<a class="les-link" href={lesson.sessionHref}>{lesson.sessionTitle}</a
									>{/snippet}
								<p class="les-quote">{lesson.correctingQuote}</p>
							</EvalQuote>
						</div>
					</section>

					<section aria-label="Why the judge called this a correction">
						<div class="band-hd-wrap">
							<button
								type="button"
								class="band-hd"
								aria-expanded={open.rationale}
								aria-controls="les-body-rationale"
								onclick={() => toggleOpen('rationale')}
							>
								<DisclosureMarker open={open.rationale} size={10} />
								<span class="machine les-hd">Why the judge called this a correction</span>
							</button>
						</div>
						{#if open.rationale}
							<div class="les-disc" id="les-body-rationale" transition:disclose>
								<div class="les-disc__in">
									<div class="band-body">
										<p class="les-rationale">{lesson.judgeRationale}</p>
									</div>
								</div>
							</div>
						{/if}
					</section>

					{#if lesson.similarCorrections.length > 0}
						{@render evidenceBand('similar', 'Similar earlier corrections', lesson.similarCorrections)}
					{/if}

					{#if lesson.evidenceCount > 1 && lesson.reinforcingCorrections.length > 0}
						{@render evidenceBand(
							'reinforcing',
							'Corrections that back this one',
							lesson.reinforcingCorrections
						)}
					{/if}

					{#if lesson.stage === 'daily' || lesson.stage === 'long_term'}
						<section aria-labelledby="les-live-hd">
							<div class="band-hd band-hd--static">
								<h2 class="machine les-hd" id="les-live-hd">Since it went live</h2>
							</div>
							<div class="band-body les-live">
								<p class="les-note les-note--strong">
									Sent to {lesson.injectionCount ?? 0} session{lesson.injectionCount === 1 ? '' : 's'}
									since it went live on {lesson.wentLiveAt ?? 'an unrecorded date'}.
								</p>
								{#if lesson.effect}
									<EffectVerdict effect={lesson.effect} />
								{/if}
							</div>
						</section>
					{/if}
				</div>

				<aside class="les-rail" aria-label="The decision">
					<div class="edge mech-group">
						<section aria-labelledby="les-rule-hd">
							<div class="band-hd band-hd--static">
								<h2 class="machine les-hd" id="les-rule-hd">The rule</h2>
							</div>
							<div class="band-body les-rule-body">
								{#if isEditable}
									<textarea
										class="les-textarea"
										aria-labelledby="les-rule-hd"
										bind:value={editedText}
									></textarea>
									<button
										type="button"
										class="les-reset"
										class:les-reset--off={!textChanged}
										onclick={resetText}>Reset to the judge's wording</button
									>
								{:else}
									<p class="les-rule-static">{shownRule}</p>
								{/if}
							</div>
						</section>

						<section aria-labelledby="les-decide-hd">
							<div class="band-hd band-hd--static">
								<h2 class="machine les-hd" id="les-decide-hd">Decide</h2>
							</div>
							<div class="band-body les-decide">
								{#if decided !== null}
									<p class="les-note">Saved. You can leave this page.</p>
								{:else if isEditable}
									<p class="les-note" data-testid="lesson-action-effect">
										Approve sends this rule to later sessions of its project, starting with their next
										message. Reject keeps it from ever being sent, and saves the reason you pick.
									</p>
									<div class="les-actions">
										<Button
											variant="default"
											disabled={saving !== null}
											aria-busy={saving === 'approve'}
											onclick={() => void approve()}>Approve</Button
										>
										<Button
											variant="outline"
											disabled={saving !== null}
											aria-expanded={rejecting}
											aria-controls="les-reasons"
											onclick={toggleReject}>Reject</Button
										>
									</div>
									{#if rejecting}
										<fieldset class="les-reasons" id="les-reasons">
											<legend class="les-legend">Why reject it?</legend>
											<div class="les-reason-row">
												{#each REASONS as r (r.reason)}
													<Button
														size="sm"
														variant="outline"
														disabled={saving !== null}
														aria-busy={saving === r.reason}
														aria-expanded={r.reason === 'other' ? showOtherNote : undefined}
														onclick={() => void reject(r.reason)}>{r.text}</Button
													>
												{/each}
											</div>
											{#if showOtherNote}
												<div class="les-reason-row">
													<input
														class="les-note-input"
														type="text"
														placeholder="Say why (optional)"
														aria-label="Why you're rejecting this lesson"
														bind:value={otherNote}
													/>
													<Button
														size="sm"
														variant="default"
														disabled={saving !== null}
														aria-busy={saving === 'other'}
														onclick={() => void reject('other')}>Reject</Button
													>
												</div>
											{/if}
										</fieldset>
									{/if}
									{#if decisionError}
										<p class="les-error" role="alert">{decisionError}</p>
									{/if}
								{:else}
									<p class="les-note">This lesson already has a decision recorded.</p>
								{/if}
							</div>
						</section>
					</div>
				</aside>
			</div>
		{/if}
	</div>
</div>

<style>
	.old-scroll {
		height: 100%;
		min-height: 0;
		overflow-y: auto;
		container-type: inline-size;
		background: var(--surface-bg);
	}
	.old {
		display: flex;
		flex-direction: column;
		gap: var(--space-5);
		max-width: 1600px;
		padding: var(--space-5) var(--space-6) var(--space-8);
	}
	.old-head {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-3);
	}
	.les-back {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		font-size: var(--text-sm);
		color: var(--interactive-active-fg);
		text-underline-offset: var(--space-1);
	}
	.les-back:hover {
		text-decoration: underline;
	}
	.les-back:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.les-work {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 24rem;
		gap: var(--space-5);
		align-items: start;
	}
	.les-spine {
		min-width: 0;
	}
	.les-rail {
		position: sticky;
		top: var(--space-4);
		min-width: 0;
		max-block-size: calc(100dvh - 44px - 2 * var(--space-4));
		overflow-y: auto;
	}
	.les-hd {
		margin: 0;
		font-size: var(--text-xs);
		font-weight: var(--weight-body);
		color: var(--text-1);
	}
	.les-meta {
		white-space: normal;
		text-align: right;
		overflow-wrap: anywhere;
	}
	.les-quotes {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}
	.les-quote {
		margin: 0;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.les-disc {
		display: grid;
	}
	.les-disc__in {
		min-height: 0;
		overflow: hidden;
	}
	.les-ev {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.les-ev + .les-ev {
		padding-top: var(--space-3);
		border-top: var(--hairline) solid var(--border-subtle);
	}
	.les-ev-session {
		font-size: var(--text-sm);
		overflow-wrap: anywhere;
	}
	.les-rationale {
		margin: 0;
		font-size: var(--text-sm);
		line-height: var(--leading-body);
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.les-live {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.les-note {
		margin: 0;
		font-size: var(--text-sm);
		line-height: var(--leading-body);
		color: var(--text-2);
		overflow-wrap: anywhere;
	}
	.les-note--strong {
		color: var(--text-1);
	}
	.les-link {
		color: var(--interactive-active-fg);
		text-underline-offset: var(--space-1);
		overflow-wrap: anywhere;
	}
	.les-link:hover {
		text-decoration: underline;
	}
	.les-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.les-rule-body,
	.les-decide {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}
	.les-textarea {
		width: 100%;
		box-sizing: border-box;
		min-block-size: 6lh;
		field-sizing: content;
		padding: var(--space-2) var(--space-3);
		border: var(--hairline) solid var(--border-default);
		background: var(--surface-bg);
		color: var(--text-1);
		font: inherit;
		font-size: var(--text-md);
		line-height: var(--leading-body);
		resize: vertical;
	}
	.les-textarea:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.les-reset {
		align-self: flex-start;
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		font-size: var(--text-xs);
		color: var(--interactive-active-fg);
		text-decoration: underline;
		cursor: pointer;
	}
	.les-reset--off {
		visibility: hidden;
	}
	.les-reset:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.les-rule-static {
		margin: 0;
		font-size: var(--text-md);
		font-weight: var(--weight-medium);
		line-height: var(--leading-body);
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.les-actions,
	.les-reason-row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
	}
	.les-reasons {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-width: 0;
		margin: 0;
		padding: 0;
		border: 0;
	}
	.les-legend {
		padding: 0;
		margin-bottom: var(--space-2);
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.les-note-input {
		flex: 1 1 12rem;
		min-width: 0;
		padding: var(--space-1) var(--space-2);
		border: var(--hairline) solid var(--border-default);
		background: var(--surface-bg);
		color: var(--text-1);
		font: inherit;
		font-size: var(--text-xs);
	}
	.les-note-input:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.les-error {
		margin: 0;
		font-size: var(--text-xs);
		color: var(--status-danger-fg);
	}
	@container (max-width: 1199.98px) {
		.les-work {
			grid-template-columns: minmax(0, 1fr);
		}
		.les-rail {
			order: -1;
			position: static;
			max-block-size: none;
			overflow-y: visible;
		}
	}
	@container (max-width: 719.98px) {
		.old {
			padding: var(--space-4) var(--space-3) var(--space-6);
		}
	}
</style>
