<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';

	const CAPTURED_CWD =
		'/Users/vimes/projects/work/ankh-morpork/night-watch/repositories/am-ledger-clerk/ledger-clerk-ui/apps/operator-console';
	const SESSION_ID = '85c12912-9196-4ddf-9a4b-71f8c2f14ee8';
	const MANUAL_VALUE = '__manual_project_path__';
	const MANUAL_PROJECT_PATH = '/Users/vimes/projects/temet/temet-proxy';

	const CANONICAL_PROJECTS = [
		{
			path: '/Users/vimes/projects/work/ankh-morpork/night-watch',
			label: 'Supplier charges'
		},
		{
			path: '/Users/vimes/projects/temet/radar',
			label: 'radar'
		}
	] as const;

	const { Story } = defineMeta({
		title: 'Projects/ProjectAssignmentControl',
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	import { AlertCircle, CheckCircle2, FolderGit2 } from '@lucide/svelte';
	import { Button } from '$lib/components/ui/button';

	let selectedProject = $state<string>(CANONICAL_PROJECTS[0].path);
	let manualProjectPath = $state('');
	let assignmentError = $state<string | null>(null);
	let assignedProject = $state<string | null>(null);
	let assigning = $state(false);

	const usingManualPath = $derived(selectedProject === MANUAL_VALUE);

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
		await Promise.resolve();
		assignedProject = projectPath;
		assigning = false;
	}
</script>

<Story
	name="Needs project"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(canvas.getByText(CAPTURED_CWD, { exact: true })).toBeInTheDocument();
		await expect(canvas.getByText(SESSION_ID, { exact: true })).toBeInTheDocument();

		const projectSelect = canvas.getByLabelText('Canonical project') as HTMLSelectElement;
		await expect(projectSelect.options.length).toBe(CANONICAL_PROJECTS.length + 1);
		await expect(projectSelect.options.item(0)?.textContent).toContain(CANONICAL_PROJECTS[0].path);
		await userEvent.click(canvas.getByRole('button', { name: 'Assign project' }));
		await waitFor(() =>
			expect(canvas.getByRole('status')).toHaveTextContent(CANONICAL_PROJECTS[0].path)
		);

		await userEvent.selectOptions(projectSelect, MANUAL_VALUE);
		const manualInput = canvas.getByLabelText('Full repository root');
		await userEvent.click(canvas.getByRole('button', { name: 'Assign project' }));
		await expect(canvas.getByRole('alert')).toHaveTextContent(
			'Enter the full repository root before assigning this session.'
		);
		await expect(manualInput).toHaveAttribute('aria-invalid', 'true');

		await userEvent.type(manualInput, MANUAL_PROJECT_PATH);
		await userEvent.click(canvas.getByRole('button', { name: 'Assign project' }));
		await waitFor(() => expect(canvas.queryByRole('alert')).not.toBeInTheDocument());
		await expect(canvas.getByRole('status')).toHaveTextContent(MANUAL_PROJECT_PATH);
	}}
>
	{#snippet template()}
		<div class="min-h-screen bg-[var(--surface-bg)] p-6 text-[var(--text-1)]">
			<div class="mx-auto flex max-w-4xl flex-col gap-4" data-slot="session-detail-context">
				<header class="flex flex-col gap-1 border-b border-[var(--border-subtle)] pb-4">
					<div class="flex flex-wrap items-center gap-2">
						<h1 class="font-display text-xl font-semibold text-[var(--text-strong)]">
							Investigate unresolved project capture
						</h1>
						<span
							class="rounded border border-[var(--status-warning-border)] bg-[var(--status-warning-bg)] px-1.5 py-0.5 text-label font-medium uppercase tracking-wider text-[var(--status-warning-fg)]"
						>
							Needs project
						</span>
					</div>
					<code class="break-all text-xs text-[var(--text-3)]">{SESSION_ID}</code>
				</header>

				<section
					class="card-surface flex flex-col gap-4 p-4"
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
								No repository root was resolved at session start. Choose a known canonical root or
								enter the correct root now.
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
						<code class="mt-1 block break-all text-xs leading-relaxed text-[var(--text-1)]">
							{CAPTURED_CWD}
						</code>
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
								class="min-h-9 w-full rounded-lg border border-[var(--border-default)] bg-[var(--surface-bg)] px-3 py-2 font-mono text-xs text-[var(--text-1)] outline-none focus-visible:ring-2 focus-visible:ring-[var(--interactive-focus-ring)]"
							>
								{#each CANONICAL_PROJECTS as project (project.path)}
									<option value={project.path}>{project.label} — {project.path}</option>
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
									class="min-h-9 w-full rounded-lg border bg-[var(--surface-bg)] px-3 py-2 font-mono text-xs text-[var(--text-1)] outline-none focus-visible:ring-2 focus-visible:ring-[var(--interactive-focus-ring)]"
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
									Session assigned to <code class="text-[var(--text-strong)]"
										>{assignedProject}</code
									>.
								</span>
							</div>
						{/if}

						<div class="flex justify-end border-t border-[var(--border-subtle)] pt-3">
							<Button type="submit" disabled={assigning}>
								{assigning ? 'Assigning' : 'Assign project'}
							</Button>
						</div>
					</form>
				</section>
			</div>
		</div>
	{/snippet}
</Story>
