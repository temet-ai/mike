<!--
@component
The reviewer's advice that a draft's lessons do not need a new skill: where they belong instead,
what mike can and cannot do about it, and on the skill page the two decisions that follow the
advice. On a folder's row it is read-only and links to the skill page. Every slot exists for the
life of the block and a button that cannot work is disabled, so nothing moves.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { ArrowRight, ClipboardCopy, Signpost } from '@lucide/svelte';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import { Button } from '$lib/components/ui/button';
	import type { DraftReview, ReviewerVerdict, SkillLesson } from '$lib/api/skills';
	import type { SkillDecision } from './SkillDecisionBar.svelte';
	import { lessonsAsBullets, placementAdvice } from './skillEvidenceText';

	let {
		review,
		variant,
		decideHref = '',
		draftReview = null,
		retired = false,
		editing = false,
		busy = null,
		existingHref = null,
		lessons = [],
		onDecide
	}: {
		review: ReviewerVerdict;
		variant: 'row' | 'page';
		/** Row: the skill page where the decision is made. */
		decideHref?: string;
		draftReview?: DraftReview | null;
		retired?: boolean;
		editing?: boolean;
		busy?: SkillDecision | null;
		/** Page: the skill page of the folder the reviewer named, or null when mike keeps none. */
		existingHref?: string | null;
		lessons?: SkillLesson[];
		onDecide?: (decision: SkillDecision) => void;
	} = $props();

	const advice = $derived(placementAdvice(review));
	const name = $derived(review.overlap ? review.overlap : null);
	const toExisting = $derived(review.decision === 'wrong_place' && review.belongs_in === 'existing_skill');
	const toRule = $derived(review.decision === 'wrong_place' && review.belongs_in === 'rule');

	const locked = $derived(busy !== null || editing || retired);
	const bullets = $derived(lessonsAsBullets(lessons));

	let clipboardSupported = $state(false);
	let copyStatus = $state('');

	onMount(() => {
		clipboardSupported = typeof navigator !== 'undefined' && !!navigator.clipboard?.writeText;
	});

	const notFound = $derived(
		toExisting && name !== null && existingHref === null
			? `${name} is not a skill folder mike keeps in this project, so there is nothing to open.`
			: ''
	);
	const status = $derived(copyStatus || notFound);

	async function copyLessons(): Promise<void> {
		try {
			await navigator.clipboard.writeText(bullets.text);
			copyStatus = `Copied ${bullets.count} ${bullets.count === 1 ? 'lesson' : 'lessons'}. Paste them into CLAUDE.md or a file in .claude/rules.`;
		} catch {
			copyStatus = 'Could not copy. Select the lessons in the list below instead.';
		}
	}
</script>

{#snippet body()}
	{#if advice}
		<p class="spa-sentence">{advice.sentence}</p>
		{#if review.reason}<p class="spa-reason">{review.reason}</p>{/if}
		{#if advice.followUp}<p class="spa-follow">{advice.followUp}</p>{/if}
		{#if variant === 'row'}
			<a class="spa-link" href={decideHref}>
				Decide on the skill page <ArrowRight class="size-3.5" aria-hidden="true" />
			</a>
		{:else}
			<div class="spa-buttons" role="group" aria-label="What to do with the reviewer's advice">
				{#if toExisting}
					<Button
						variant="outline"
						href={existingHref ?? '#'}
						disabled={existingHref === null || name === null}
					>
						{name ? `Open ${name}` : 'Open the existing skill'}
						<ArrowRight aria-hidden="true" />
					</Button>
				{:else if toRule}
					<Button
						variant="outline"
						disabled={bullets.count === 0 || !clipboardSupported}
						onclick={() => void copyLessons()}
					>
						<ClipboardCopy aria-hidden="true" />
						Copy the lessons as a rule
					</Button>
				{/if}
				<Button
					variant="outline"
					disabled={locked || draftReview === 'approved'}
					onclick={() => onDecide?.('approve')}>Ship it anyway</Button
				>
				<Button
					variant="default"
					disabled={locked || draftReview === 'rejected'}
					onclick={() => onDecide?.('reject')}>Do not ship it</Button
				>
			</div>
			<p class="spa-status" aria-live="polite">{status}</p>
		{/if}
	{/if}
{/snippet}

{#if advice}
	{#if variant === 'page'}
		<div data-slot="skill-placement-advice" data-variant="page">
			<SectionCard title="The reviewer's advice" titleIcon={Signpost} accent="info">
				<div class="spa-body">{@render body()}</div>
			</SectionCard>
		</div>
	{:else}
		<div class="spa spa-row" data-slot="skill-placement-advice" data-variant="row">
			<h4 class="spa-head">The reviewer's advice</h4>
			{@render body()}
		</div>
	{/if}
{/if}

<style>
	.spa-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.spa-row {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-3);
		background: var(--status-info-bg);
		border: 1px solid var(--status-info-border);
		border-left: 2px solid var(--status-info-fg);
	}
	.spa-head {
		margin: 0;
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--status-info-fg);
	}
	.spa-sentence,
	.spa-reason,
	.spa-follow,
	.spa-status {
		margin: 0;
	}
	.spa-sentence {
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	.spa-reason {
		font-size: var(--text-sm);
		color: var(--text-2);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.spa-follow,
	.spa-status {
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.spa-status {
		min-height: 1lh;
	}
	.spa-buttons {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}
	.spa-link {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		width: fit-content;
		font-size: var(--text-sm);
	}
</style>
