<script lang="ts">
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import {
		ApiError,
		createVolatileLine,
		deleteVolatileLine,
		listVolatileLines,
		patchVolatileLine
	} from '$lib/api';
	import type { VolatileLinePattern, VolatileLinePatternCreate } from '$lib/types';
	import { ArrowLeft, Pencil, Trash2 } from '@lucide/svelte';
	import { onApiCacheUpdate } from '$lib/stores/cache/apiCache.svelte';
	import HelpHint from '$lib/components/shared/HelpHint.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import { Button } from '$lib/components/ui/button';

	function blockLabel(idx: 0 | 1 | 2): string {
		if (idx === 0) return 'block[0] · billing-header';
		if (idx === 1) return 'block[1] · identity preamble';
		return 'block[2] · skeleton + role overlay';
	}

	let patterns = $state<VolatileLinePattern[]>([]);
	let loading = $state(false);
	let initialized = $state(false);
	let listError = $state<string | null>(null);

	async function refresh(): Promise<void> {
		loading = true;
		try {
			patterns = await listVolatileLines();
			listError = null;
		} catch (err) {
			listError = err instanceof ApiError ? err.message : String(err);
		} finally {
			loading = false;
			initialized = true;
		}
	}

	onMount(() => {
		void refresh();
		const unsub = onApiCacheUpdate(
			'/api/volatile-lines',
			() => {
				void refresh();
			},
			{ prefix: true }
		);
		return () => {
			unsub();
		};
	});

	let formName = $state('');
	let formPattern = $state('');
	let formBlockIndex = $state<0 | 1 | 2>(2);
	let formEnabled = $state(true);
	let formNotes = $state('');
	let formError = $state<string | null>(null);
	let formSubmitting = $state(false);

	function resetForm(): void {
		formName = '';
		formPattern = '';
		formBlockIndex = 2;
		formEnabled = true;
		formNotes = '';
		formError = null;
	}

	const formCanSubmit = $derived(
		!formSubmitting && formName.trim().length > 0 && formPattern.length > 0
	);

	async function submitAdd(e: Event): Promise<void> {
		e.preventDefault();
		if (!formCanSubmit) return;
		formSubmitting = true;
		formError = null;
		try {
			const body: VolatileLinePatternCreate = {
				name: formName.trim(),
				pattern: formPattern,
				applies_to_block_index: formBlockIndex,
				enabled: formEnabled,
				notes: formNotes.length > 0 ? formNotes : null
			};
			const created = await createVolatileLine(body);
			patterns = [...patterns, created];
			resetForm();
		} catch (err) {
			formError = err instanceof ApiError ? err.message : String(err);
		} finally {
			formSubmitting = false;
		}
	}

	let editingId = $state<string | null>(null);
	let editName = $state('');
	let editPattern = $state('');
	let editBlockIndex = $state<0 | 1 | 2>(2);
	let editEnabled = $state(true);
	let editNotes = $state('');
	let editError = $state<string | null>(null);
	let editSubmitting = $state(false);

	function beginEdit(p: VolatileLinePattern): void {
		editingId = p.id;
		editName = p.name;
		editPattern = p.pattern;
		editBlockIndex = p.applies_to_block_index;
		editEnabled = p.enabled;
		editNotes = p.notes ?? '';
		editError = null;
		if (confirmDeleteId === p.id) confirmDeleteId = null;
	}

	function cancelEdit(): void {
		editingId = null;
		editError = null;
	}

	async function submitEdit(e: Event): Promise<void> {
		e.preventDefault();
		if (editingId === null || editSubmitting) return;
		editSubmitting = true;
		editError = null;
		try {
			const updated = await patchVolatileLine(editingId, {
				name: editName.trim(),
				pattern: editPattern,
				applies_to_block_index: editBlockIndex,
				enabled: editEnabled,
				notes: editNotes.length > 0 ? editNotes : null
			});
			patterns = patterns.map((p) => (p.id === updated.id ? updated : p));
			editingId = null;
		} catch (err) {
			editError = err instanceof ApiError ? err.message : String(err);
		} finally {
			editSubmitting = false;
		}
	}

	let toggleErrors = $state<Record<string, string>>({});

	async function toggleEnabled(p: VolatileLinePattern): Promise<void> {
		const previous = p.enabled;
		const next = !previous;
		patterns = patterns.map((row) => (row.id === p.id ? { ...row, enabled: next } : row));
		if (toggleErrors[p.id]) {
			const cleaned = { ...toggleErrors };
			delete cleaned[p.id];
			toggleErrors = cleaned;
		}
		try {
			const updated = await patchVolatileLine(p.id, { enabled: next });
			patterns = patterns.map((row) => (row.id === updated.id ? updated : row));
		} catch (err) {
			patterns = patterns.map((row) => (row.id === p.id ? { ...row, enabled: previous } : row));
			toggleErrors = {
				...toggleErrors,
				[p.id]: err instanceof ApiError ? err.message : String(err)
			};
		}
	}

	let deleteErrors = $state<Record<string, string>>({});
	let deletingId = $state<string | null>(null);
	let confirmDeleteId = $state<string | null>(null);

	function beginDeleteConfirm(p: VolatileLinePattern): void {
		confirmDeleteId = p.id;
		if (deleteErrors[p.id]) {
			const cleaned = { ...deleteErrors };
			delete cleaned[p.id];
			deleteErrors = cleaned;
		}
		if (editingId === p.id) editingId = null;
	}

	function cancelDeleteConfirm(): void {
		confirmDeleteId = null;
	}

	async function performDelete(p: VolatileLinePattern): Promise<void> {
		deletingId = p.id;
		try {
			await deleteVolatileLine(p.id);
			patterns = patterns.filter((row) => row.id !== p.id);
			confirmDeleteId = null;
		} catch (err) {
			if (err instanceof ApiError && err.status === 404) {
				patterns = patterns.filter((row) => row.id !== p.id);
				confirmDeleteId = null;
			} else {
				deleteErrors = {
					...deleteErrors,
					[p.id]: err instanceof ApiError ? err.message : String(err)
				};
			}
		} finally {
			deletingId = null;
		}
	}
</script>

<svelte:head>
	<title>Volatile lines — Prompts — mike</title>
</svelte:head>

<section
	class="flex flex-col gap-6 overflow-y-auto p-6"
	data-slot="volatile-lines-page"
	aria-label="Volatile-line registry"
>
	<div class="flex flex-wrap items-baseline justify-between gap-4">
		<PageIntro title="Volatile lines">
			<span class="flex flex-wrap items-center gap-1.5">
				<span>Lines matching these patterns are stripped before computing the</span>
				<span class="inline-flex items-center gap-1">
					<span>prompt class hash.</span>
					<HelpHint slug="volatile-lines" />
				</span>
				<span>Bytes forwarded upstream to Anthropic are unchanged.</span>
			</span>
		</PageIntro>
		<span class="pr-back-link-backing">
			<a href={resolve('/prompts')} class="pr-back-link" data-augmented-ui="tr-clip bl-clip border">
				<ArrowLeft class="size-3" aria-hidden="true" />
				<span>back to prompts</span>
			</a>
		</span>
	</div>

	{#if listError}
		<LoadErrorBanner
			title="Couldn't load volatile-line patterns"
			detail={listError ?? undefined}
			onRetry={() => void refresh()}
		/>
	{/if}

	<section
		class="pr-panel"
		data-augmented-ui="tl-clip br-clip border"
		aria-label="Volatile-line patterns"
		data-slot="volatile-lines-panel"
	>
		<header class="pr-panel-head">
			<span class="pr-panel-tag-backing">
				<h3 class="pr-panel-tag" data-augmented-ui="tl-clip br-clip border">
					Volatile-line patterns
				</h3>
			</span>
			<span class="pr-panel-count">
				{patterns.length}
			</span>
		</header>

		<div class="vl-add-block" data-slot="volatile-lines-add">
			<h4 class="vl-subhead">Add pattern</h4>
			<form class="flex flex-col gap-3" onsubmit={submitAdd}>
				<label class="flex flex-col gap-1">
					<span class="text-label uppercase tracking-kpi text-[var(--text-3)]"> Name </span>
					<input
						bind:value={formName}
						type="text"
						maxlength="120"
						autocomplete="off"
						spellcheck="false"
						placeholder="e.g. working-directory-line"
						class="rounded border border-[var(--border-default)] bg-[var(--surface-bg)] px-2 py-1.5 font-mono text-xs text-[var(--text-strong)] outline-none focus-visible:border-[var(--interactive-focus-ring)] focus-visible:ring-1 focus-visible:ring-[var(--interactive-focus-ring)]"
					/>
				</label>

				<label class="flex flex-col gap-1">
					<span
						class="flex items-center gap-1 text-label uppercase tracking-kpi text-[var(--text-3)]"
					>
						Pattern (regex)
						<Tooltip text="A regular expression, matched per-line against the chosen block's text.">
							<span class="normal-case">(?)</span>
						</Tooltip>
					</span>
					<textarea
						bind:value={formPattern}
						rows="3"
						autocomplete="off"
						spellcheck="false"
						placeholder="^Working directory: .+$"
						class="rounded border border-[var(--border-default)] bg-[var(--surface-bg)] px-2 py-1.5 font-mono text-xs text-[var(--text-strong)] outline-none focus-visible:border-[var(--interactive-focus-ring)] focus-visible:ring-1 focus-visible:ring-[var(--interactive-focus-ring)]"
					></textarea>
				</label>

				<fieldset class="flex flex-col gap-1.5">
					<legend
						class="flex items-center gap-1 text-label uppercase tracking-kpi text-[var(--text-3)]"
					>
						Applies to block
						<Tooltip
							text="Scope the pattern to a single system block — only that block's text is normalised."
						>
							<span class="normal-case">(?)</span>
						</Tooltip>
					</legend>
					{#each [0, 1, 2] as bi (bi)}
						<label class="flex items-center gap-2 text-xs text-[var(--text-1)]">
							<input
								type="radio"
								name="add-block-index"
								value={bi}
								checked={formBlockIndex === bi}
								onchange={() => (formBlockIndex = bi as 0 | 1 | 2)}
								class="accent-[var(--interactive-active-fg)]"
							/>
							{blockLabel(bi as 0 | 1 | 2)}
						</label>
					{/each}
				</fieldset>

				<label class="flex items-center gap-2 text-xs text-[var(--text-1)]">
					<input
						type="checkbox"
						bind:checked={formEnabled}
						class="accent-[var(--interactive-active-fg)]"
					/>
					<span class="flex items-center gap-1">
						Enabled
						<Tooltip
							text="When off, this pattern is kept but not applied to the prompt class hash."
						>
							<span class="text-[var(--text-3)]">(?)</span>
						</Tooltip>
					</span>
				</label>

				<label class="flex flex-col gap-1">
					<span class="text-label uppercase tracking-kpi text-[var(--text-3)]">
						Notes (optional)
					</span>
					<textarea
						bind:value={formNotes}
						rows="2"
						autocomplete="off"
						class="rounded border border-[var(--border-default)] bg-[var(--surface-bg)] px-2 py-1.5 font-mono text-xs text-[var(--text-strong)] outline-none focus-visible:border-[var(--interactive-focus-ring)] focus-visible:ring-1 focus-visible:ring-[var(--interactive-focus-ring)]"
					></textarea>
				</label>

				{#if formError}
					<div
						class="rounded border border-[var(--status-danger-border)] bg-[var(--status-danger-bg)] px-3 py-2 text-label text-[var(--status-danger-fg)]"
						role="alert"
						data-slot="volatile-lines-add-error"
					>
						{formError}
					</div>
				{/if}

				<div class="flex items-center justify-end">
					<Button type="submit" size="sm" disabled={!formCanSubmit}>
						{formSubmitting ? 'adding…' : 'add pattern'}
					</Button>
				</div>
			</form>
		</div>

		<div class="vl-divider" role="none"></div>

		<div class="vl-list-block" data-slot="volatile-lines-list-section">
			{#if !initialized && loading}
				<div class="flex flex-col gap-2" data-slot="volatile-lines-list-loading">
					<div class="h-12 animate-pulse rounded bg-[var(--surface-2)]"></div>
					<div class="h-12 animate-pulse rounded bg-[var(--surface-2)]"></div>
					<div class="h-12 animate-pulse rounded bg-[var(--surface-2)]"></div>
				</div>
			{:else if patterns.length === 0}
				<div
					class="flex flex-col items-center justify-center gap-3 py-12 text-center"
					data-slot="volatile-lines-list-empty"
				>
					<p class="text-sm font-medium text-[var(--text-strong)]">No patterns registered</p>
					<p class="max-w-md text-label text-[var(--text-3)]">
						Add a pattern above to start normalising volatile lines out of the prompt class hash.
					</p>
				</div>
			{:else}
				<div class="overflow-x-auto" data-slot="volatile-lines-table-wrapper">
					<table class="volatile-lines-table w-full font-mono text-xs">
						<thead>
							<tr>
								<th class="text-left text-label uppercase tracking-kpi text-[var(--text-3)]">
									Name
								</th>
								<th class="text-left text-label uppercase tracking-kpi text-[var(--text-3)]">
									Pattern
								</th>
								<th class="text-left text-label uppercase tracking-kpi text-[var(--text-3)]">
									Block
								</th>
								<th class="text-left text-label uppercase tracking-kpi text-[var(--text-3)]">
									Enabled
								</th>
								<th class="text-right text-label uppercase tracking-kpi text-[var(--text-3)]">
									Actions
								</th>
							</tr>
						</thead>
						<tbody>
							{#each patterns as p (p.id)}
								{@const isEditing = editingId === p.id}
								<tr
									data-slot="volatile-lines-row"
									data-pattern-id={p.id}
									class="border-t border-[var(--border-subtle)]"
								>
									<td class="py-2 pr-3 align-top text-[var(--text-strong)]">
										{p.name}
									</td>
									<td class="py-2 pr-3 align-top text-[var(--text-1)]">
										<code
											class="block whitespace-pre-wrap break-all rounded bg-[var(--surface-2)] px-1.5 py-0.5"
										>
											{p.pattern}
										</code>
									</td>
									<td class="py-2 pr-3 align-top text-[var(--text-2)]">
										{blockLabel(p.applies_to_block_index)}
									</td>
									<td class="py-2 pr-3 align-top">
										<label class="flex items-center gap-2">
											<input
												type="checkbox"
												checked={p.enabled}
												onchange={() => void toggleEnabled(p)}
												class="accent-[var(--interactive-active-fg)]"
												aria-label={`toggle ${p.name}`}
											/>
											<span class="text-[var(--text-2)]">
												{p.enabled ? 'on' : 'off'}
											</span>
										</label>
										{#if toggleErrors[p.id]}
											<span class="ml-1 text-[var(--status-danger-fg)]" role="alert">
												{toggleErrors[p.id]}
											</span>
										{/if}
									</td>
									<td class="py-2 align-top">
										<div class="flex items-center justify-end gap-1.5">
											<Tooltip text={`Edit ${p.name}`}>
												{#snippet child({ props })}
													<Button
														{...props}
														variant="ghost"
														size="icon"
														class="size-6"
														onclick={() => beginEdit(p)}
														disabled={isEditing}
														aria-label={`edit ${p.name}`}
													>
														<Pencil class="size-3" aria-hidden="true" />
													</Button>
												{/snippet}
											</Tooltip>
											<Tooltip text={`Delete ${p.name}`}>
												{#snippet child({ props })}
													<Button
														{...props}
														variant="ghost"
														size="icon"
														class="size-6"
														onclick={() => beginDeleteConfirm(p)}
														disabled={confirmDeleteId === p.id}
														aria-label={`delete ${p.name}`}
													>
														<Trash2
															class="size-3 text-[var(--status-danger-fg)]"
															aria-hidden="true"
														/>
													</Button>
												{/snippet}
											</Tooltip>
										</div>
									</td>
								</tr>
								{#if confirmDeleteId === p.id}
									<tr>
										<td colspan="5" class="px-0 pb-3">
											<div
												class="vl-danger-card flex flex-col gap-2"
												data-augmented-ui="tl-clip br-clip border"
												data-slot="volatile-lines-delete-confirm"
											>
												<p class="text-xs text-[var(--text-1)]">
													Delete pattern <span class="font-mono">{p.name}</span>? Matching lines
													stop being stripped from the prompt class hash.
												</p>
												{#if deleteErrors[p.id]}
													<p class="text-label text-[var(--status-danger-fg)]" role="alert">
														delete failed: {deleteErrors[p.id]}
													</p>
												{/if}
												<div class="flex items-center gap-2">
													<Button
														type="button"
														variant="destructive"
														size="sm"
														onclick={() => void performDelete(p)}
														disabled={deletingId === p.id}
													>
														{deletingId === p.id ? 'deleting…' : 'confirm delete'}
													</Button>
													<Button
														type="button"
														variant="outline"
														size="sm"
														onclick={cancelDeleteConfirm}
														disabled={deletingId === p.id}
													>
														cancel
													</Button>
												</div>
											</div>
										</td>
									</tr>
								{/if}
								{#if isEditing}
									<tr>
										<td colspan="5" class="px-0 pb-3">
											<form
												class="flex flex-col gap-3 rounded-md border border-[var(--border-subtle)] bg-[var(--surface-2)] p-3"
												onsubmit={submitEdit}
												data-slot="volatile-lines-edit-form"
											>
												<label class="flex flex-col gap-1">
													<span class="text-label uppercase tracking-kpi text-[var(--text-3)]">
														Name
													</span>
													<input
														bind:value={editName}
														type="text"
														maxlength="120"
														autocomplete="off"
														spellcheck="false"
														class="rounded border border-[var(--border-default)] bg-[var(--surface-bg)] px-2 py-1.5 font-mono text-xs text-[var(--text-strong)] outline-none focus-visible:border-[var(--interactive-focus-ring)] focus-visible:ring-1 focus-visible:ring-[var(--interactive-focus-ring)]"
													/>
												</label>
												<label class="flex flex-col gap-1">
													<span class="text-label uppercase tracking-kpi text-[var(--text-3)]">
														Pattern (regex)
													</span>
													<textarea
														bind:value={editPattern}
														rows="3"
														autocomplete="off"
														spellcheck="false"
														class="rounded border border-[var(--border-default)] bg-[var(--surface-bg)] px-2 py-1.5 font-mono text-xs text-[var(--text-strong)] outline-none focus-visible:border-[var(--interactive-focus-ring)] focus-visible:ring-1 focus-visible:ring-[var(--interactive-focus-ring)]"
													></textarea>
												</label>
												<fieldset class="flex flex-col gap-1.5">
													<legend class="text-label uppercase tracking-kpi text-[var(--text-3)]">
														Applies to block
													</legend>
													{#each [0, 1, 2] as bi (bi)}
														<label class="flex items-center gap-2 text-xs text-[var(--text-1)]">
															<input
																type="radio"
																name="edit-block-index"
																value={bi}
																checked={editBlockIndex === bi}
																onchange={() => (editBlockIndex = bi as 0 | 1 | 2)}
																class="accent-[var(--interactive-active-fg)]"
															/>
															{blockLabel(bi as 0 | 1 | 2)}
														</label>
													{/each}
												</fieldset>
												<label class="flex items-center gap-2 text-xs text-[var(--text-1)]">
													<input
														type="checkbox"
														bind:checked={editEnabled}
														class="accent-[var(--interactive-active-fg)]"
													/>
													Enabled
												</label>
												<label class="flex flex-col gap-1">
													<span class="text-label uppercase tracking-kpi text-[var(--text-3)]">
														Notes (optional)
													</span>
													<textarea
														bind:value={editNotes}
														rows="2"
														autocomplete="off"
														class="rounded border border-[var(--border-default)] bg-[var(--surface-bg)] px-2 py-1.5 font-mono text-xs text-[var(--text-strong)] outline-none focus-visible:border-[var(--interactive-focus-ring)] focus-visible:ring-1 focus-visible:ring-[var(--interactive-focus-ring)]"
													></textarea>
												</label>
												{#if editError}
													<div
														class="rounded border border-[var(--status-danger-border)] bg-[var(--status-danger-bg)] px-3 py-2 text-label text-[var(--status-danger-fg)]"
														role="alert"
													>
														{editError}
													</div>
												{/if}
												<div class="flex items-center justify-end gap-2">
													<Button type="button" variant="outline" size="sm" onclick={cancelEdit}>
														cancel
													</Button>
													<Button type="submit" size="sm" disabled={editSubmitting}>
														{editSubmitting ? 'saving…' : 'save'}
													</Button>
												</div>
											</form>
										</td>
									</tr>
								{/if}
							{/each}
						</tbody>
					</table>
				</div>
			{/if}
		</div>
	</section>
</section>

<style>
	.pr-back-link-backing {
		display: inline-flex;
		background: var(--surface-card);
	}
	.pr-back-link {
		--edge: 8px;
		--aug-tl: 0;
		--aug-tr: var(--edge);
		--aug-br: 0;
		--aug-bl: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		display: inline-flex;
		align-items: center;
		gap: 8px;
		padding: 6px 14px;
		background: var(--surface-card);
		color: var(--text-2);
		font-size: var(--text-label);
		font-weight: 600;
		text-decoration: none;
		transition: color 0.12s ease;
	}
	.pr-back-link:hover,
	.pr-back-link:focus-visible {
		color: var(--text-strong);
		--aug-border-bg: var(--interactive-active-border);
	}
	.pr-back-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.vl-subhead {
		margin: 0 0 2px;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		font-size: 0.8125rem;
		letter-spacing: -0.01em;
		color: var(--text-strong);
	}

	.vl-divider {
		height: 1px;
		flex-shrink: 0;
		background: var(--border-subtle);
	}

	.vl-danger-card {
		--edge: 6px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--status-danger-fg) 0%,
			var(--status-danger-border) 60%
		);
		padding: calc(var(--edge) + 4px);
		background: var(--status-danger-bg);
	}

	.pr-panel {
		--edge-tl: 8px;
		--edge-br: 12px;
		--aug-tl: var(--edge-tl);
		--aug-br: var(--edge-br);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--data-teal-fg) 0%,
			var(--data-teal-border) 35%,
			var(--border-default) 70%
		);
		display: flex;
		flex-direction: column;
		gap: 12px;
		background: var(--surface-card);
		padding: calc(var(--edge-tl) + 4px) calc(var(--edge-br) + 4px) calc(var(--edge-br) + 4px)
			calc(var(--edge-tl) + 4px);
	}
	.pr-panel-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}
	.pr-panel-tag-backing {
		display: inline-flex;
		align-self: flex-start;
		background: var(--surface-card);
	}

	.pr-panel-tag {
		--aug-tl: 6px;
		--aug-tr: 0;
		--aug-br: 6px;
		--aug-bl: 0;
		--aug-border-all: 1px;
		--aug-border-bg: var(--data-teal-border);
		margin: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 800;
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--data-teal-fg);
		padding: 3px 8px;
		background:
			linear-gradient(0deg, var(--data-teal-bg), var(--data-teal-bg)), var(--surface-card);
	}
	.pr-panel-count {
		font-family: 'JetBrains Mono Variable', ui-monospace, monospace;
		font-size: var(--text-label);
		color: var(--text-3);
	}

	.volatile-lines-table {
		border-collapse: collapse;
	}
	.volatile-lines-table th,
	.volatile-lines-table td {
		padding: 0.5rem 0.75rem;
		vertical-align: top;
	}
	.volatile-lines-table th:first-child,
	.volatile-lines-table td:first-child {
		padding-left: 0;
	}
	.volatile-lines-table th:last-child,
	.volatile-lines-table td:last-child {
		padding-right: 0;
	}
</style>
