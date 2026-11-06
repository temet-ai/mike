<!--
@component
Lessons mike placed as a rule or an agent statement, per project. mike never writes these into a
project on its own; here the user reads each lesson, sees the file it belongs in and the text it
would add, and accepts or declines it.
-->
<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { resolve } from '$app/paths';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import EmptyStateOnboarding from '$lib/components/shared/EmptyStateOnboarding.svelte';
	import * as Tabs from '$lib/components/ui/tabs';
	import ProposalRow from '$lib/components/proposals/ProposalRow.svelte';
	import { decideProposal } from '$lib/api/proposals';
	import type { ProposalsPageData } from './+page';

	const { data }: { data: ProposalsPageData } = $props();

	const project = $derived(data.projects?.find((p) => p.project_uuid === data.selected) ?? null);

	let busy = $state<number | null>(null);
	let failed = $state<{ id: number; message: string } | null>(null);

	function choose(uuid: string): void {
		void goto(`${resolve('/proposals')}?project=${encodeURIComponent(uuid)}`, {
			keepFocus: true,
			noScroll: true
		});
	}

	async function decide(id: number, decision: 'accept' | 'decline' | 'clear'): Promise<void> {
		busy = id;
		failed = null;
		try {
			await decideProposal(id, decision);
			await invalidateAll();
		} catch (e) {
			failed = { id, message: e instanceof Error ? e.message : String(e) };
		} finally {
			busy = null;
		}
	}

	function projectMeta(proposals: number, undecided: number): string {
		const total = `${proposals} ${proposals === 1 ? 'proposal' : 'proposals'}`;
		return undecided > 0 ? `${total}, ${undecided} waiting on you` : total;
	}
</script>

<svelte:head>
	<title>Proposals — mike</title>
</svelte:head>

<section class="pp-scroll" aria-label="Proposals">
	<div class="pp">
		<PageIntro
			title="Proposals"
			level="h1"
			description="Some lessons are short standing rules for certain files, or say how one of your agents should work. mike never writes those into your project; it lists them here for you to accept or decline."
		/>

		{#if data.projectsError}
			<LoadErrorBanner
				title="Could not load the projects"
				detail={data.projectsError}
				onRetry={() => void invalidateAll()}
			/>
		{:else if !data.projects || data.projects.length === 0}
			<EmptyStateOnboarding
				headline="No proposals yet"
				body="A proposal appears when mike decides a lesson belongs in a rule or an agent definition rather than in a skill or memory."
				command={null}
				confirmation="Nothing is added to your project until you accept it."
			/>
		{:else}
			<Tabs.Root
				bind:value={() => data.selected ?? '', choose}
				orientation="vertical"
				class="pp-work"
			>
				<Tabs.List aria-label="Projects" class="pp-list">
					{#each data.projects as p (p.project_uuid)}
						<Tabs.Trigger value={p.project_uuid} class="pp-proj">
							<span>{p.display_name}</span>
							<span>{projectMeta(p.proposals, p.undecided)}</span>
						</Tabs.Trigger>
					{/each}
				</Tabs.List>
				{#each data.projects as p (p.project_uuid)}
					<Tabs.Content value={p.project_uuid} class="pp-main">
						{#if project && p.project_uuid === project.project_uuid}
							<PageIntro
								title={project.display_name}
								level="h2"
								description={project.project_path}
							/>
							{#if data.proposalsError}
								<LoadErrorBanner
									title="Could not load this project's proposals"
									detail={data.proposalsError}
									onRetry={() => void invalidateAll()}
								/>
							{:else if data.proposals}
								<div class="pp-rows">
									{#each data.proposals.proposals as proposal (proposal.lesson_id)}
										<ProposalRow
											{proposal}
											busy={busy === proposal.lesson_id}
											error={failed?.id === proposal.lesson_id ? failed.message : null}
											onDecide={(d) => void decide(proposal.lesson_id, d)}
										/>
									{/each}
								</div>
							{/if}
						{/if}
					</Tabs.Content>
				{/each}
			</Tabs.Root>
		{/if}
	</div>
</section>

<style>
	.pp-scroll {
		height: 100%;
		min-height: 0;
		overflow-y: auto;
	}
	.pp {
		display: flex;
		flex-direction: column;
		gap: var(--space-5);
		max-width: 1600px;
		padding: var(--space-5) var(--space-6) var(--space-8);
	}
	:global(.pp-work) {
		display: grid;
		grid-template-columns: 20rem minmax(0, 1fr);
		gap: var(--space-5);
		align-items: start;
	}
	:global(.pp-list.pp-list) {
		position: sticky;
		top: var(--space-4);
		display: flex;
		flex-direction: column;
		align-items: stretch;
		gap: var(--space-1);
		width: auto;
		height: auto;
	}
	:global(.pp-proj.pp-proj) {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-1);
		height: auto;
	}
	:global(.pp-main) {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		min-width: 0;
	}
	.pp-rows {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}
</style>
