<!--
@component
One learned skill draft: the files as Claude reads them beside the lessons they came from, the
reviewer's evidence, loads since it was written, and the user's decision.
-->
<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { ArrowLeft } from '@lucide/svelte';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import { Button } from '$lib/components/ui/button';
	import SkillStateBadge from '$lib/components/skills/SkillStateBadge.svelte';
	import SkillFilesViewer from '$lib/components/skills/SkillFilesViewer.svelte';
	import SkillLessonList from '$lib/components/skills/SkillLessonList.svelte';
	import SkillEvidence from '$lib/components/skills/SkillEvidence.svelte';
	import SkillDecisionBar, {
		type SkillDecision
	} from '$lib/components/skills/SkillDecisionBar.svelte';
	import SkillRevisionList from '$lib/components/skills/SkillRevisionList.svelte';
	import SkillPlacementAdvice from '$lib/components/skills/SkillPlacementAdvice.svelte';
	import { skillOutcome } from '$lib/components/skills/skillOutcome';
	import {
		liveUseSentence,
		placementAdvice,
		retirementSentence
	} from '$lib/components/skills/skillEvidenceText';
	import {
		editSkillDraft,
		restoreSkill,
		retireSkill,
		reviewSkillDraft
	} from '$lib/api/skills';
	import type { SkillDraftPageData } from './+page';

	const { data }: { data: SkillDraftPageData } = $props();

	const detail = $derived(data.detail);
	const draft = $derived(detail?.draft ?? null);
	const retired = $derived(detail?.folder?.retired_by_user_at != null);

	let editing = $state(false);
	let draftText = $state('');
	let busy = $state<SkillDecision | null>(null);
	let error = $state<string | null>(null);

	// The advice is about the folder's newest draft only; an older revision shows none, because
	// "Ship it anyway" would then approve a draft that is no longer the newest.
	const adviceReview = $derived.by(() => {
		const review = draft?.eval?.review;
		if (!draft || !review || draft.id !== detail?.folder?.draft_id) return null;
		return placementAdvice(review) === null ? null : review;
	});

	// The skill page of the folder the reviewer named, when mike keeps a draft for that folder.
	const existingHref = $derived.by(() => {
		const name = adviceReview?.overlap;
		const hit = name
			? data.skills?.skills.find((s) => s.folder === name && s.draft_id !== null)
			: undefined;
		return hit?.draft_id == null
			? null
			: resolve(`skills/${hit.draft_id}` as `skills/${string}`);
	});

	const outcome = $derived(
		draft && detail ? skillOutcome(detail.mode, draft.status, draft.review, retired) : ''
	);

	const backHref = $derived(
		draft
			? `${resolve('/skills')}?project=${encodeURIComponent(draft.project_uuid)}`
			: resolve('/skills')
	);

	async function act(decision: SkillDecision, run: () => Promise<unknown>): Promise<void> {
		busy = decision;
		error = null;
		try {
			await run();
			await invalidateAll();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			busy = null;
		}
	}

	function decide(decision: SkillDecision): void {
		if (!draft) return;
		const d = draft;
		if (decision === 'edit') {
			draftText = d.skill_md;
			editing = true;
			return;
		}
		if (decision === 'retire') void act(decision, () => retireSkill(d.project_uuid, d.folder));
		else if (decision === 'restore') void act(decision, () => restoreSkill(d.project_uuid, d.folder));
		else void act(decision, () => reviewSkillDraft(d.id, decision));
	}

	async function saveEdit(): Promise<void> {
		if (!draft) return;
		const id = draft.id;
		await act('edit', () => editSkillDraft(id, draftText));
		if (error === null) editing = false;
	}
</script>

<svelte:head>
	<title>{draft ? `${draft.folder} — Skills` : 'Skill'} — mike</title>
</svelte:head>

<section class="sd-scroll" aria-label="Skill">
	<div class="sd">
		<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- backHref is built from resolve('/skills') plus a query string -->
		<a class="sd-back" href={backHref}><ArrowLeft class="size-4" aria-hidden="true" /> Skills</a>

		{#if data.error || !detail || !draft}
			<LoadErrorBanner title="Could not load this skill" detail={data.error ?? undefined} />
		{:else}
			<PageIntro
				title={draft.folder}
				level="h1"
				description={`Revision ${draft.revision} of this skill for ${draft.project_path}, written ${new Date(draft.created_at).toLocaleString()} from ${draft.lesson_ids.length} lessons.`}
			>
				{#snippet actions()}
					{#if detail.folder}<SkillStateBadge state={detail.folder.state} />{/if}
				{/snippet}
			</PageIntro>

			<h2 class="sr-only">The skill, its lessons and its evidence</h2>

			{#if adviceReview}
				<SkillPlacementAdvice
					review={adviceReview}
					variant="page"
					draftReview={draft.review}
					{retired}
					{editing}
					{busy}
					{existingHref}
					lessons={detail.lessons}
					onDecide={decide}
				/>
			{/if}

			<SkillDecisionBar
				review={draft.review}
				{retired}
				{editing}
				{busy}
				{outcome}
				{error}
				onDecide={decide}
			/>

			<div class="sd-pair">
				<SectionCard
					title="What Claude reads"
					description={draft.edited_at
						? 'You edited SKILL.md. The reviewer judged the text before your edit.'
						: 'The description below decides when Claude loads the skill; the files are what it reads then.'}
				>
					<p class="sd-description">{draft.description}</p>
					<SkillFilesViewer
						skillMd={draft.skill_md}
						references={draft.reference_files}
						{editing}
						bind:draftText
					/>
					{#if editing}
						<div class="sd-edit-actions">
							<Button variant="default" disabled={busy !== null} onclick={() => void saveEdit()}
								>Save SKILL.md</Button
							>
							<Button variant="ghost" disabled={busy !== null} onclick={() => (editing = false)}
								>Cancel</Button
							>
						</div>
					{/if}
				</SectionCard>
				<SectionCard title="The lessons behind it">
					<SkillLessonList lessons={detail.lessons} />
				</SectionCard>
			</div>

			<SectionCard title="What the reviewer found">
				<SkillEvidence {draft} />
			</SectionCard>

			{#if detail.folder}
				{@const use = detail.folder.live_use}
				{@const retired = retirementSentence(use)}
				<SectionCard title="Since it was written">
					<p class="sd-description">
						{#if detail.folder.first_written === null}
							This skill has not been written into any checkout.
						{:else if use === null}
							First written {detail.folder.first_written}. The promoter has not measured its use yet.
						{:else}
							First written {detail.folder.first_written}. {liveUseSentence(use)}
						{/if}
					</p>
					{#if retired}
						<p class="sd-description">{retired}</p>
					{/if}
				</SectionCard>
			{/if}

			<SectionCard title="Every revision of this skill">
				<SkillRevisionList revisions={detail.revisions} current={draft.id} />
			</SectionCard>
		{/if}
	</div>
</section>

<style>
	.sd-scroll {
		height: 100%;
		min-height: 0;
		overflow-y: auto;
	}
	.sd {
		display: flex;
		flex-direction: column;
		gap: var(--space-5);
		max-width: 1600px;
		padding: var(--space-5) var(--space-6) var(--space-8);
	}
	.sd-back {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		width: fit-content;
	}
	.sd-pair {
		display: grid;
		grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
		gap: var(--space-5);
		align-items: start;
	}
	.sd-edit-actions {
		display: flex;
		gap: var(--space-2);
	}
</style>
