<script lang="ts">
	import { AlertCircle, CheckCircle2, FolderGit2 } from '@lucide/svelte';
	import { Button } from '$lib/components/ui/button';
	import { assignSessionProject } from '$lib/api';
	import type { FleetProjectSummary } from '$lib/fleet/types';
	import type { SessionProjectAssignmentResponse } from '$lib/types';

	interface Props {
		sessionId: string;
		cwd: string | null;
		projects: readonly FleetProjectSummary[];
		projectsLoading?: boolean;
		onAssigned?: (assignment: SessionProjectAssignmentResponse) => void | Promise<void>;
	}

	const MANUAL_VALUE = '__manual_project_path__';

	let { sessionId, cwd, projects, projectsLoading = false, onAssigned }: Props = $props();

	let selectedProject = $state(MANUAL_VALUE);
	let manualProjectPath = $state('');
	let assignmentError = $state<string | null>(null);
	let assignedProject = $state<string | null>(null);
	let assigning = $state(false);
	let initialized = $state(false);

	const usingManualPath = $derived(selectedProject === MANUAL_VALUE);

	$effect(() => {
		if (initialized || projectsLoading) return;
		selectedProject = projects[0]?.project_path ?? MANUAL_VALUE;
		initialized = true;
	});

	function clearOutcome(): void {
		assignmentError = null;
		assignedProject = null;
	}

	function onProjectChoice(): void {
		clearOutcome();
	}

	function onManualPathInput(): void {
		clearOutcome();
	}

	async function assignProject(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		clearOutcome();

		const projectPath = usingManualPath ? manualProjectPath.trim() : selectedProject;
		if (projectPath.length === 0) {
			assignmentError = 'Enter the full repository root before assigning this session.';
			return;
		}
		if (!projectPath.startsWith('/')) {
			assignmentError = 'Repository root must be a full absolute path.';
			return;
		}

		assigning = true;
		try {
			const assignment = await assignSessionProject(sessionId, projectPath);
			assignedProject = assignment.project_path;
			await onAssigned?.(assignment);
		} catch (error) {
			assignmentError = error instanceof Error ? error.message : String(error);
		} finally {
			assigning = false;
		}
	}
</script>

<section
	class="card-surface mx-6 my-4 flex flex-col gap-4 p-4"
	aria-labelledby="project-assignment-title"
	data-slot="project-assignment-control"
>
	<div class="flex items-start gap-3">
		<div
			class="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded border border-[var(--status-warning-border)] bg-[var(--status-warning-bg)] text-[var(--status-warning-fg)]"
		>
			<FolderGit2 size={16} aria-hidden="true" />
		</div>
		<div class="min-w-0 flex-1">
			<h2
				id="project-assignment-title"
				class="font-display text-base font-semibold text-[var(--text-strong)]"
			>
				Assign this session to a project
			</h2>
			<p class="mt-1 text-xs leading-relaxed text-[var(--text-2)]">
				No repository root was resolved at session start. Choose a known canonical root or enter the
				correct root now.
			</p>
		</div>
	</div>

	<div
		class="rounded-lg border border-[var(--border-subtle)] bg-[var(--surface-2)] p-3"
		data-slot="project-assignment-cwd"
	>
		<div class="text-label font-medium uppercase tracking-wider text-[var(--text-3)]">
			Captured working directory
		</div>
		{#if cwd}
			<code class="mt-1 block break-all text-xs leading-relaxed text-[var(--text-1)]">{cwd}</code>
		{:else}
			<p class="mt-1 text-xs text-[var(--text-3)]">No working directory was captured.</p>
		{/if}
	</div>

	<form class="flex flex-col gap-3" onsubmit={assignProject}>
		<div class="flex flex-col gap-1.5">
			<label
				for="project-assignment-select"
				class="text-label font-medium uppercase tracking-wider text-[var(--text-2)]"
			>
				Canonical project
			</label>
			<select
				id="project-assignment-select"
				bind:value={selectedProject}
				onchange={onProjectChoice}
				disabled={projectsLoading || assigning}
				class="min-h-9 w-full rounded-lg border border-[var(--border-default)] bg-[var(--surface-bg)] px-3 py-2 font-mono text-xs text-[var(--text-1)] outline-none focus-visible:ring-2 focus-visible:ring-[var(--interactive-focus-ring)] disabled:opacity-50"
			>
				{#each projects as project (project.project_path)}
					<option value={project.project_path}>
						{project.custom_name ? `${project.custom_name} — ` : ''}{project.project_path}
					</option>
				{/each}
				<option value={MANUAL_VALUE}>Enter another repository root</option>
			</select>
		</div>

		{#if usingManualPath}
			<div class="flex flex-col gap-1.5" data-slot="project-assignment-manual-path">
				<label
					for="project-assignment-path"
					class="text-label font-medium uppercase tracking-wider text-[var(--text-2)]"
				>
					Full repository root
				</label>
				<input
					id="project-assignment-path"
					type="text"
					bind:value={manualProjectPath}
					oninput={onManualPathInput}
					placeholder="/Users/name/projects/repository"
					aria-invalid={assignmentError ? 'true' : undefined}
					aria-describedby={assignmentError ? 'project-assignment-error' : undefined}
					disabled={assigning}
					class="min-h-9 w-full rounded-lg border bg-[var(--surface-bg)] px-3 py-2 font-mono text-xs text-[var(--text-1)] outline-none focus-visible:ring-2 focus-visible:ring-[var(--interactive-focus-ring)] disabled:opacity-50"
					class:border-[var(--status-danger-border)]={assignmentError !== null}
					class:border-[var(--border-default)]={assignmentError === null}
				/>
			</div>
		{/if}

		{#if assignmentError}
			<div
				id="project-assignment-error"
				class="flex items-start gap-2 rounded-lg border border-[var(--status-danger-border)] bg-[var(--status-danger-bg)] px-3 py-2 text-xs text-[var(--status-danger-fg)]"
				role="alert"
			>
				<AlertCircle class="mt-0.5 shrink-0" size={14} aria-hidden="true" />
				<span>{assignmentError}</span>
			</div>
		{/if}

		{#if assignedProject}
			<div
				class="flex items-start gap-2 rounded-lg border border-[var(--status-success-border)] bg-[var(--status-success-bg)] px-3 py-2 text-xs text-[var(--status-success-fg)]"
				role="status"
				data-slot="project-assignment-success"
			>
				<CheckCircle2 class="mt-0.5 shrink-0" size={14} aria-hidden="true" />
				<span class="min-w-0 break-all">
					Session assigned to <code class="text-[var(--text-strong)]">{assignedProject}</code>.
				</span>
			</div>
		{/if}

		<div class="flex justify-end border-t border-[var(--border-subtle)] pt-3">
			<Button type="submit" disabled={assigning || projectsLoading}>
				{assigning ? 'Assigning' : 'Assign project'}
			</Button>
		</div>
	</form>
</section>
