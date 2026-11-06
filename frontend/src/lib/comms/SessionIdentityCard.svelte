<script lang="ts">
	import { Info, Pencil } from '@lucide/svelte';
	import { ApiError } from '$lib/api';
	import { getSessionIdentity, putSessionIdentity } from './sessionIdentity';
	import type { SessionIdentityDto, SetSessionIdentityBody } from './sessionIdentity';
	import { roleTokens } from '$lib/fleet/roleTokens';
	import { shortSessionId } from '$lib/format';
	import * as Field from '$lib/components/ui/field';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';

	interface Props {
		sessionId: string;
		fallbackLabel?: string | null;
	}

	const { sessionId, fallbackLabel = null }: Props = $props();

	let identity = $state<SessionIdentityDto | null>(null);
	let loading = $state(true);
	let loadError = $state<string | null>(null);
	let loadedFor = $state<string | null>(null);

	let editing = $state(false);
	let draftName = $state('');
	let draftRole = $state('');
	let draftGoal = $state('');
	let draftNextSteps = $state('');
	let draftInstructions = $state('');
	let saving = $state(false);
	let saveError = $state<string | null>(null);
	let nameInputEl: HTMLInputElement | HTMLTextAreaElement | null = $state(null);

	async function load(id: string): Promise<void> {
		loading = true;
		loadError = null;
		try {
			identity = await getSessionIdentity(id);
			loadedFor = id;
		} catch (err) {
			loadError = err instanceof ApiError ? err.message : String(err);
		} finally {
			loading = false;
		}
	}

	function retry(): void {
		void load(sessionId);
	}

	$effect(() => {
		const id = sessionId;
		if (loadedFor === id) return;
		void load(id);
	});

	const displayName = $derived(identity?.name ?? fallbackLabel ?? shortSessionId(sessionId));
	const tokens = $derived(roleTokens(identity?.role ?? null));

	function startEdit(): void {
		draftName = identity?.name ?? '';
		draftRole = identity?.role ?? '';
		draftGoal = identity?.goal ?? '';
		draftNextSteps = identity?.next_steps ?? '';
		draftInstructions = identity?.instructions ?? '';
		saveError = null;
		editing = true;
	}

	function cancelEdit(): void {
		editing = false;
		saveError = null;
	}

	function changedField(draft: string, current: string | null | undefined): string | undefined {
		const trimmed = draft.trim();
		const existing = (current ?? '').trim();
		if (trimmed === existing) return undefined;
		return trimmed;
	}

	function buildPatch(): SetSessionIdentityBody {
		const patch: SetSessionIdentityBody = {};
		const name = changedField(draftName, identity?.name);
		if (name !== undefined) patch.name = name;
		const role = changedField(draftRole, identity?.role);
		if (role !== undefined) patch.role = role;
		const goal = changedField(draftGoal, identity?.goal);
		if (goal !== undefined) patch.goal = goal;
		const nextSteps = changedField(draftNextSteps, identity?.next_steps);
		if (nextSteps !== undefined) patch.next_steps = nextSteps;
		const instructions = changedField(draftInstructions, identity?.instructions);
		if (instructions !== undefined) patch.instructions = instructions;
		return patch;
	}

	async function save(): Promise<void> {
		saving = true;
		saveError = null;
		const patch = buildPatch();
		try {
			if (Object.keys(patch).length > 0) {
				await putSessionIdentity(sessionId, patch);
			}
			identity = await getSessionIdentity(sessionId);
			editing = false;
		} catch (err) {
			saveError = err instanceof ApiError ? err.message : String(err);
		} finally {
			saving = false;
		}
	}

	function onFormKeydown(e: KeyboardEvent): void {
		if (e.key === 'Escape') {
			e.preventDefault();
			cancelEdit();
		}
	}

	$effect(() => {
		if (editing) nameInputEl?.focus();
	});
</script>

<section
	class="sid-card"
	data-slot="session-identity-card"
	aria-label="Session identity"
	data-augmented-ui="tl-clip br-clip border"
>
	<div class="sid-head">
		<span class="sid-head-label">
			<h3 class="sid-title">Identity</h3>
			<Tooltip
				text="This session's declared name, role, standing instructions and current goal — set by the session itself or an operator."
			>
				<Info size={12} aria-hidden="true" class="sid-help" />
			</Tooltip>
		</span>
		{#if !editing}
			<button
				type="button"
				class="sid-edit-btn"
				aria-label="Edit session identity"
				onclick={startEdit}
				disabled={loading}
			>
				<Pencil size={12} aria-hidden="true" />
				Edit
			</button>
		{/if}
	</div>

	{#if loading && !identity}
		<p class="sid-status">Loading identity</p>
	{:else if loadError && !identity}
		<LoadErrorBanner title="Couldn't load session identity" detail={loadError} onRetry={retry} />
	{:else if editing}
		<!-- svelte-ignore a11y_no_noninteractive_element_interactions -- onkeydown only catches Escape bubbling from the form's own focusable controls, never a click/nav target on the form itself. -->
		<form
			class="sid-form"
			onsubmit={(e) => {
				e.preventDefault();
				void save();
			}}
			onkeydown={onFormKeydown}
		>
			<Field.Root
				label="Name"
				bind:value={draftName}
				bind:ref={nameInputEl}
				placeholder="e.g. observer-1"
				disabled={saving}
			/>
			<Field.Root
				label="Role"
				bind:value={draftRole}
				placeholder="e.g. observer, rust-developer"
				disabled={saving}
			/>
			<Field.Root
				label="Goal"
				multiline
				rows={2}
				bind:value={draftGoal}
				placeholder="what this session is trying to accomplish now"
				disabled={saving}
			/>
			<Field.Root
				label="Next steps"
				multiline
				rows={2}
				bind:value={draftNextSteps}
				placeholder="what it will do next"
				disabled={saving}
			/>
			<Field.Root
				label="Instructions"
				multiline
				rows={3}
				bind:value={draftInstructions}
				placeholder="the standing charter this session works under"
				disabled={saving}
			/>

			{#if saveError}
				<p class="sid-error" role="alert">{saveError}</p>
			{/if}

			<div class="sid-actions">
				<button type="button" class="sid-cancel" onclick={cancelEdit} disabled={saving}>
					Cancel
				</button>
				<button
					type="submit"
					class="sid-save"
					data-augmented-ui="tl-clip br-clip border"
					disabled={saving}
				>
					{saving ? 'Saving' : 'Save'}
				</button>
			</div>
		</form>
	{:else}
		<div class="sid-body">
			<div class="sid-name-row">
				<span class="sid-name" data-slot="session-identity-name">{displayName}</span>
				{#if identity?.role}
					<span
						class="badge-nx badge-muted"
						data-slot="session-identity-role"
						style:color={tokens.fg}
					>
						{identity.role}
					</span>
				{/if}
			</div>

			<div class="sid-grp">
				<span class="sid-k">Goal</span>
				<span class="sid-v">{identity?.goal ?? '—'}</span>
			</div>
			<div class="sid-grp">
				<span class="sid-k">Next steps</span>
				<span class="sid-v">{identity?.next_steps ?? '—'}</span>
			</div>
			{#if identity?.instructions}
				<div class="sid-grp sid-grp--muted">
					<span class="sid-k">Instructions</span>
					<span class="sid-v sid-v--muted">{identity.instructions}</span>
				</div>
			{/if}
		</div>
	{/if}
</section>

<style>
	.sid-card {
		--edge: 8px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px 14px;
		background: var(--surface-card);
	}

	.sid-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
	}

	.sid-head-label {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
	}

	.sid-title {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
		cursor: default;
	}

	:global(.sid-help) {
		flex: none;
		color: var(--text-faint);
	}

	.sid-edit-btn {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 2px 8px;
		border: 1px solid var(--border-subtle);
		border-radius: 4px;
		background: transparent;
		color: var(--text-3);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		cursor: pointer;
		transition:
			color 0.14s,
			border-color 0.14s;
	}

	.sid-edit-btn:hover:not(:disabled) {
		color: var(--text-strong);
		border-color: var(--interactive-active-border);
	}

	.sid-edit-btn:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.sid-edit-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.sid-status {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
	}

	.sid-body {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.sid-name-row {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
	}

	.sid-name {
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		font-size: 16px;
		letter-spacing: -0.01em;
		color: var(--text-strong);
		overflow-wrap: break-word;
	}

	.sid-grp {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.sid-k {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--text-faint);
	}

	.sid-v {
		font-size: 13px;
		line-height: 1.5;
		color: var(--text-1);
		overflow-wrap: break-word;
		white-space: pre-wrap;
	}

	.sid-grp--muted {
		padding-top: 4px;
		border-top: 1px solid var(--border-subtle);
	}

	.sid-v--muted {
		font-size: 12px;
		color: var(--text-3);
	}

	.sid-form {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.sid-error {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--status-danger-fg);
	}

	.sid-actions {
		display: flex;
		gap: 8px;
	}

	.sid-cancel {
		padding: 5px 14px;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		background: var(--surface-2);
		color: var(--text-2);
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		cursor: pointer;
	}

	.sid-cancel:hover:not(:disabled) {
		color: var(--text-strong);
		border-color: var(--interactive-active-border);
	}

	.sid-cancel:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.sid-cancel:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.sid-save {
		--edge: 4px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-steering-border);
		padding: 5px 14px;
		background: var(--status-steering-bg);
		color: var(--status-steering-fg);
		font-family: var(--font-display);
		font-size: var(--text-label);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		cursor: pointer;
	}

	.sid-save:hover:not(:disabled) {
		background: var(--status-steering-border);
	}

	.sid-save:disabled {
		opacity: 0.55;
		cursor: not-allowed;
	}
</style>
