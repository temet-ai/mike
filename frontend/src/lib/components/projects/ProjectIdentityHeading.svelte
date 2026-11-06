<script lang="ts">
	import type { FleetProjectSummary } from '$lib/fleet/types';
	import { Pencil } from '@lucide/svelte';

	interface Props {
		project: FleetProjectSummary;
		onRenameProject?: (projectPath: string, customName: string | null) => Promise<void>;
	}

	const { project, onRenameProject }: Props = $props();

	let editing = $state(false);
	let draftName = $state('');
	let saving = $state(false);
	let saveError = $state<string | null>(null);
	let nameInput = $state<HTMLInputElement | undefined>(undefined);

	$effect(() => {
		if (editing) nameInput?.focus();
	});

	function startEdit(): void {
		draftName = project.custom_name ?? '';
		saveError = null;
		editing = true;
	}

	function cancelEdit(): void {
		editing = false;
		saveError = null;
	}

	async function applyRename(nextName: string | null): Promise<void> {
		if (!onRenameProject) {
			editing = false;
			return;
		}
		saving = true;
		saveError = null;
		try {
			await onRenameProject(project.project_path, nextName);
			editing = false;
		} catch (err) {
			saveError = err instanceof Error ? err.message : String(err);
		} finally {
			saving = false;
		}
	}

	function commitEdit(): void {
		const trimmed = draftName.trim();
		void applyRename(trimmed.length > 0 ? trimmed : null);
	}

	function clearName(): void {
		void applyRename(null);
	}

	function onNameInputKeydown(event: KeyboardEvent): void {
		if (event.key === 'Enter') {
			event.preventDefault();
			commitEdit();
		} else if (event.key === 'Escape') {
			event.preventDefault();
			cancelEdit();
		}
	}
</script>

<div class="pih-wrap flex min-w-0 flex-1 flex-col gap-1" data-slot="project-detail-heading">
	<div class="flex items-start gap-2">
		{#if editing}
			<div class="flex min-w-0 flex-1 flex-col gap-1.5" data-slot="project-detail-name-edit">
				<label class="sr-only" for="project-detail-name-input">Friendly name for this project</label
				>
				<div class="flex items-center gap-2">
					<input
						id="project-detail-name-input"
						bind:this={nameInput}
						bind:value={draftName}
						type="text"
						class="min-w-0 flex-1 rounded border border-[var(--border-default)] bg-[var(--surface-bg)] px-2 py-1 font-mono text-sm text-[var(--text-1)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--interactive-focus-ring)]"
						placeholder={project.project_path}
						disabled={saving}
						onkeydown={onNameInputKeydown}
					/>
					<button
						type="button"
						class="rounded border border-[var(--border-default)] px-2 py-1 text-label text-[var(--text-2)] hover:border-[var(--interactive-active-border)] hover:text-[var(--interactive-active-fg)]"
						aria-label="Save friendly name"
						disabled={saving}
						onclick={commitEdit}
					>
						Save
					</button>
					<button
						type="button"
						class="rounded border border-[var(--border-default)] px-2 py-1 text-label text-[var(--text-3)] hover:text-[var(--text-1)]"
						aria-label="Cancel editing friendly name"
						disabled={saving}
						onclick={cancelEdit}
					>
						Cancel
					</button>
					{#if project.custom_name}
						<button
							type="button"
							class="rounded border border-[var(--border-default)] px-2 py-1 text-label text-[var(--text-3)] hover:text-[var(--text-1)]"
							aria-label="Clear friendly name"
							disabled={saving}
							onclick={clearName}
						>
							Clear
						</button>
					{/if}
				</div>
				{#if saveError}
					<p
						class="text-label text-[var(--status-danger-fg)]"
						data-slot="project-detail-name-error"
					>
						{saveError}
					</p>
				{/if}
			</div>
		{:else}
			<h2
				class="pih-heading min-w-0 flex-1 break-words text-lg text-[var(--text-strong)]"
				data-slot="project-detail-custom-name"
			>
				{project.custom_name ?? project.project_path}
			</h2>
			<button
				type="button"
				class="mt-0.5 shrink-0 rounded p-1 text-[var(--text-3)] hover:bg-[var(--surface-2)] hover:text-[var(--text-1)]"
				aria-label="Edit friendly name for this project"
				onclick={startEdit}
			>
				<Pencil size={14} aria-hidden="true" />
			</button>
		{/if}
	</div>
	<p
		class="font-mono text-sm break-words text-[var(--text-3)]"
		data-slot="project-detail-path"
		title={project.project_path}
	>
		{project.project_path}
	</p>
</div>

<style>
	.pih-heading {
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		letter-spacing: -0.02em;
	}
</style>
