<script lang="ts">
	import { AlertCircle, SearchX, TriangleAlert } from '@lucide/svelte';
	import { onMount } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import * as Field from '$lib/components/ui/field';
	import * as RadioGroup from '$lib/components/ui/radio-group';
	import { projectDisplayName } from '$lib/components/filter/WorkbenchProjectPicker.svelte';
	import type { FleetProjectWithCounts } from '$lib/fleet/types';
	import { formatCount } from '$lib/time';

	interface Props {
		project: FleetProjectWithCounts;
		projects: readonly FleetProjectWithCounts[];
		onRenameProject?: (projectPath: string, customName: string | null) => Promise<void>;
		onMergeProject: (projectPath: string, intoProjectUuid: string) => Promise<void>;
		onCreateProject: (projectPath: string, name: string) => Promise<void>;
		onClose: () => void;
	}

	const KEEP = 'keep';
	const NEW = 'new';

	const { project, projects, onRenameProject, onMergeProject, onCreateProject, onClose }: Props =
		$props();

	let name = $state('');
	let search = $state('');
	let picked = $state(KEEP);
	let newName = $state('');
	let saving = $state(false);
	let error = $state<string | null>(null);
	let newNameInput = $state<HTMLInputElement | HTMLTextAreaElement | null>(null);

	const slug = $derived(project.project_path.replace(/[^a-zA-Z0-9]+/g, '-'));

	const candidates = $derived(
		projects
			.filter((p) => p.project_path !== project.project_path && p.project_uuid)
			.toSorted((a, b) => projectDisplayName(a).localeCompare(projectDisplayName(b)))
	);

	const suggested = $derived(
		candidates
			.filter((p) => project.project_path.startsWith(p.project_path.replace(/\/+$/, '')))
			.toSorted((a, b) => b.project_path.length - a.project_path.length)[0]
	);

	const shown = $derived(
		candidates
			.filter((p) => {
				const needle = search.trim().toLowerCase();
				return (
					projectDisplayName(p).toLowerCase().includes(needle) ||
					p.project_path.toLowerCase().includes(needle)
				);
			})
			.toSorted((a, b) => Number(b === suggested) - Number(a === suggested))
	);

	const target = $derived(candidates.find((p) => p.project_uuid === picked));
	const moving = $derived(picked === NEW || target !== undefined);
	const canSave = $derived(!saving && (picked !== NEW || newName.trim().length > 0));
	const sessionWord = $derived(project.session_count === 1 ? 'session' : 'sessions');

	onMount(() => {
		name = project.custom_name ?? '';
	});

	$effect(() => {
		if (picked === NEW) newNameInput?.focus();
	});

	function onOpenChange(next: boolean): void {
		if (!next) onClose();
	}

	async function save(): Promise<void> {
		saving = true;
		error = null;
		try {
			const trimmed = name.trim();
			if (onRenameProject && trimmed !== (project.custom_name ?? '')) {
				await onRenameProject(project.project_path, trimmed.length > 0 ? trimmed : null);
			}
			if (target?.project_uuid) {
				await onMergeProject(project.project_path, target.project_uuid);
			} else if (picked === NEW) {
				await onCreateProject(project.project_path, newName.trim());
			}
			onClose();
		} catch (err) {
			error = err instanceof Error ? err.message : String(err);
		} finally {
			saving = false;
		}
	}
</script>

<Dialog.Root open={true} {onOpenChange}>
	<Dialog.Content class="psd" bodyClass="!flex flex-col" data-testid="project-settings-dialog">
		<Dialog.Header>
			<Dialog.Title>Edit project</Dialog.Title>
			<Dialog.Description class="font-mono text-[var(--text-2)]">
				{project.project_path}
			</Dialog.Description>
		</Dialog.Header>

		<Field.Root
			label="Name"
			bind:value={name}
			placeholder="Name shown in the project list"
			disabled={saving}
			data-slot="project-settings-name"
		/>

		<section class="psd-belongs" aria-labelledby="psd-belongs-{slug}">
			<span id="psd-belongs-{slug}" class="machine psd-label">Belongs to</span>
			<p class="psd-help">
				If this folder is a worktree or another copy of a project, put it in that project. Its
				sessions go with it and new sessions from this folder land there.
			</p>
			<Field.Root
				type="search"
				bind:value={search}
				placeholder="Find a project"
				aria-label="Find a project"
				disabled={saving}
				data-slot="project-settings-search"
			/>
			<div class="edge psd-options">
				<RadioGroup.Root
					class="mech-group psd-options__group"
					name="psd-belongs-{slug}"
					bind:value={picked}
					disabled={saving}
					aria-labelledby="psd-belongs-{slug}"
					data-slot="project-settings-options"
				>
					<RadioGroup.Item value={KEEP} label="Its own project" description="Leave it as it is" />
					<div class="mech-group psd-options__scroll">
						{#each shown as candidate (candidate.project_path)}
							<RadioGroup.Item
								value={candidate.project_uuid ?? ''}
								description={candidate.project_path}
							>
								{#snippet label()}
									{projectDisplayName(candidate)}
									{#if candidate === suggested}
										<span class="badge-nx badge-analyzed">Suggested</span>
									{/if}
								{/snippet}
							</RadioGroup.Item>
						{:else}
							<p class="psd-empty">
								<SearchX size={14} aria-hidden="true" />
								<span>No project matches “{search}”.</span>
							</p>
						{/each}
					</div>
					<RadioGroup.Item
						value={NEW}
						label="Create a new project"
						description="None of these fit"
					/>
				</RadioGroup.Root>
			</div>

			{#if picked === NEW}
				<Field.Root
					label="New project name"
					bind:ref={newNameInput}
					bind:value={newName}
					placeholder="Name of the new project"
					disabled={saving}
					data-slot="project-settings-new-name"
				/>
			{/if}

			<p class="psd-warning" role="status" data-slot="project-settings-warning">
				{#if moving}
					<TriangleAlert size={14} aria-hidden="true" />
					<span>
						{formatCount(project.session_count)}
						{sessionWord} will move and this folder will stop showing in the list. This cannot be undone
						from here.
					</span>
				{/if}
			</p>
		</section>

		{#if error}
			<p class="psd-error" role="alert">
				<AlertCircle size={14} aria-hidden="true" />
				<span>Not saved: {error}</span>
			</p>
		{/if}

		<Dialog.Footer>
			<Button variant="ghost" disabled={saving} onclick={() => onOpenChange(false)}>Cancel</Button>
			<Button disabled={!canSave} onclick={save} data-slot="project-settings-save">
				{saving ? 'Saving' : 'Save'}
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>

<style>
	:global([data-slot='dialog-content'].psd) {
		max-inline-size: min(44rem, calc(100vw - 2 * var(--space-4)));
	}

	.psd-label {
		font-size: var(--text-xs);
		color: var(--text-2);
	}

	.psd-belongs {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		flex: 1 1 auto;
		min-block-size: 0;
		min-inline-size: 0;
	}

	.psd-help,
	.psd-warning,
	.psd-error {
		margin: 0;
		font-size: var(--text-sm);
	}

	.psd-help {
		color: var(--text-2);
	}

	.psd-warning,
	.psd-error {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
	}

	.psd-warning {
		color: var(--status-warning-fg);
	}

	.psd-error {
		color: var(--status-danger-fg);
	}

	.psd-warning :global(svg),
	.psd-error :global(svg) {
		flex: 0 0 auto;
		margin-block-start: 2px;
	}

	.psd-warning:empty {
		display: none;
	}

	.psd-options {
		--edge: var(--edge-sm);
		--edge-pad: 0px;
		--edge-line: var(--border-default);
		--edge-surface: var(--surface-bg);
		display: flex;
		flex-direction: column;
		flex: 1 1 auto;
		min-block-size: 0;
	}

	.psd-options :global(.psd-options__group) {
		flex: 1 1 auto;
		min-block-size: 0;
	}

	.psd-options__scroll {
		flex: 1 1 auto;
		min-block-size: calc(3 * var(--control-h));
		overflow-y: auto;
		overscroll-behavior: contain;
	}

	.psd-empty {
		margin: 0;
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-3);
		font-size: var(--text-xs);
		color: var(--text-3);
	}
</style>
