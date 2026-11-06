<!--
@component
Every skill mike has written or would write into each project, its state and evidence, and the
project's choice of whether mike may ship skills on its own. See
artifacts/2026-10-14/learned-skills-ui/design.md.
-->
<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { resolve } from '$app/paths';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import EmptyStateOnboarding from '$lib/components/shared/EmptyStateOnboarding.svelte';
	import * as Tabs from '$lib/components/ui/tabs';
	import SkillModeControl from '$lib/components/skills/SkillModeControl.svelte';
	import SkillFolderRow from '$lib/components/skills/SkillFolderRow.svelte';
	import { setSkillMode, type SkillMode } from '$lib/api/skills';
	import type { SkillsPageData } from './+page';

	const { data }: { data: SkillsPageData } = $props();

	const project = $derived(data.projects?.find((p) => p.project_uuid === data.selected) ?? null);

	function choose(uuid: string): void {
		void goto(`${resolve('/skills')}?project=${encodeURIComponent(uuid)}`, {
			keepFocus: true,
			noScroll: true
		});
	}

	async function saveMode(mode: SkillMode): Promise<void> {
		if (!data.selected) return;
		await setSkillMode(data.selected, mode);
		await invalidateAll();
	}

	function modeWord(mode: SkillMode): string {
		return mode === 'ask' ? 'asks first' : 'ships when reviewed';
	}
</script>

<svelte:head>
	<title>Skills — mike</title>
</svelte:head>

<section class="sk-scroll" aria-label="Skills">
	<div class="sk">
		<PageIntro
			title="Skills"
			level="h1"
			description="mike turns the lessons it learns into Claude Code skills and writes them into your project folders under .claude/skills. In a project set to ask first, nothing is written until you approve it here."
		/>

		{#if data.projectsError}
			<LoadErrorBanner
				title="Could not load the projects"
				detail={data.projectsError}
				onRetry={() => void invalidateAll()}
			/>
		{:else if !data.projects || data.projects.length === 0}
			<EmptyStateOnboarding
				headline="No learned skills yet"
				body="A skill is drafted once a project has two or more accepted lessons on the same topic."
				command={null}
				confirmation="Drafts appear here as soon as they are written, before anything reaches your project."
			/>
		{:else}
			<Tabs.Root
				bind:value={() => data.selected ?? '', choose}
				orientation="vertical"
				class="sk-work"
			>
				<Tabs.List aria-label="Projects" class="sk-list">
					{#each data.projects as p (p.project_uuid)}
						<Tabs.Trigger value={p.project_uuid} class="sk-proj">
							<span class="sk-proj-name">{p.display_name}</span>
							<span class="sk-proj-meta"
								>{p.shipped} of {p.skill_cap} shipped, {modeWord(p.mode)}{p.waiting > 0
									? `, ${p.waiting} waiting on you`
									: ''}</span
							>
						</Tabs.Trigger>
					{/each}
				</Tabs.List>
				{#each data.projects as p (p.project_uuid)}
					<Tabs.Content value={p.project_uuid} class="sk-main">
						{#if project && p.project_uuid === project.project_uuid}
							<PageIntro
								title={project.display_name}
								level="h2"
								description={project.project_path}
							/>
							{#key project.project_uuid}
								<SkillModeControl
									mode={data.skills?.mode ?? project.mode}
									projectName={project.display_name}
									onChange={saveMode}
								/>
							{/key}
							{#if data.skillsError}
								<LoadErrorBanner
									title="Could not load this project's skills"
									detail={data.skillsError}
									onRetry={() => void invalidateAll()}
								/>
							{:else if data.skills}
								<div class="sk-rows">
									{#each data.skills.skills as skill (skill.folder)}
										<SkillFolderRow {skill} />
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
	.sk-scroll {
		height: 100%;
		min-height: 0;
		overflow-y: auto;
	}
	.sk {
		display: flex;
		flex-direction: column;
		gap: var(--space-5);
		max-width: 1600px;
		padding: var(--space-5) var(--space-6) var(--space-8);
	}
	:global(.sk-work) {
		display: grid;
		grid-template-columns: 20rem minmax(0, 1fr);
		gap: var(--space-5);
		align-items: start;
	}
	:global(.sk-list.sk-list) {
		position: sticky;
		top: var(--space-4);
		display: flex;
		flex-direction: column;
		align-items: stretch;
		gap: var(--space-1);
		width: auto;
		height: auto;
	}
	:global(.sk-proj.sk-proj) {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-1);
		height: auto;
	}
	:global(.sk-main) {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		min-width: 0;
	}
	.sk-rows {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}
</style>
