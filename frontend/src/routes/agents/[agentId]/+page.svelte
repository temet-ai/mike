<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import {
		getAgent,
		patchAgent,
		deleteAgent,
		type AgentDetail,
		type AgentSource,
		type AgentDispatchRow
	} from '$lib/api/agents';
	import { ApiError } from '$lib/api';
	import { onApiCacheUpdate } from '$lib/stores/cache/apiCache.svelte';
	import { formatRelative } from '$lib/time';
	import { shortSessionId } from '$lib/format';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import HelpHint from '$lib/components/shared/HelpHint.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Send, Fingerprint, Bot, Layers } from '@lucide/svelte';

	const DISPATCH_PAGE_SIZE = 50;

	const agentId = $derived(page.params.agentId ?? '');

	let detail = $state<AgentDetail | null>(null);
	let error = $state<string | null>(null);
	let loading = $state(true);

	let loadingMoreDispatches = $state(false);
	let loadMoreDispatchesError = $state<string | null>(null);
	let expandedDispatches = $state<Set<number>>(new Set());

	function toggleDispatch(toolUseId: number): void {
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- cloned, mutated, then reassigned wholesale; reactivity comes from the reassignment below, not in-place mutation
		const next = new Set(expandedDispatches);
		if (next.has(toolUseId)) next.delete(toolUseId);
		else next.add(toolUseId);
		expandedDispatches = next;
	}

	async function loadMoreDispatches(): Promise<void> {
		if (detail === null || loadingMoreDispatches) return;
		loadingMoreDispatches = true;
		loadMoreDispatchesError = null;
		try {
			const page = await getAgent(agentId, {
				dispatch_limit: DISPATCH_PAGE_SIZE,
				dispatch_offset: detail.dispatches.rows.length
			});
			const merged: AgentDispatchRow[] = [...detail.dispatches.rows, ...page.dispatches.rows];
			detail = {
				...detail,
				dispatches: { total_count: page.dispatches.total_count, rows: merged }
			};
		} catch (err) {
			loadMoreDispatchesError = err instanceof ApiError ? err.message : String(err);
		} finally {
			loadingMoreDispatches = false;
		}
	}

	let editing = $state(false);
	let editName = $state('');
	let editDefinitionPath = $state('');
	let editDescription = $state('');
	let editSource = $state<AgentSource>('observed');
	let saveInFlight = $state(false);
	let saveError = $state<string | null>(null);

	let deleting = $state(false);
	let deleteError = $state<string | null>(null);
	let confirmDelete = $state(false);

	function openEditor(d: AgentDetail): void {
		confirmDelete = false;
		deleteError = null;
		editName = d.name;
		editDefinitionPath = d.definition_path ?? '';
		editDescription = d.description ?? '';
		editSource = d.source;
		saveError = null;
		editing = true;
	}

	function cancelEdit(): void {
		editing = false;
		saveError = null;
	}

	function openDeleteConfirm(): void {
		editing = false;
		saveError = null;
		deleteError = null;
		confirmDelete = true;
	}

	async function save(): Promise<void> {
		if (detail === null) return;
		const trimmedName = editName.trim();
		if (trimmedName.length === 0) {
			saveError = 'Name must not be empty.';
			return;
		}
		saveInFlight = true;
		saveError = null;
		try {
			await patchAgent(detail.id, {
				name: trimmedName === detail.name ? undefined : trimmedName,
				definition_path:
					editDefinitionPath.trim().length === 0
						? detail.definition_path === null
							? undefined
							: null
						: editDefinitionPath.trim(),
				description:
					editDescription.trim().length === 0
						? detail.description === null
							? undefined
							: null
						: editDescription.trim(),
				source: editSource === detail.source ? undefined : editSource
			});
			editing = false;
			await load(detail.id);
		} catch (err) {
			saveError = err instanceof ApiError ? err.message : String(err);
		} finally {
			saveInFlight = false;
		}
	}

	async function performDelete(): Promise<void> {
		if (detail === null) return;
		deleting = true;
		deleteError = null;
		try {
			await deleteAgent(detail.id);
			await goto(resolve('/agents'));
		} catch (err) {
			deleteError = err instanceof ApiError ? err.message : String(err);
			deleting = false;
		}
	}

	async function load(id: number | string): Promise<void> {
		if (typeof id === 'string' && id.length === 0) return;
		loading = true;
		error = null;
		try {
			detail = await getAgent(id, { dispatch_limit: DISPATCH_PAGE_SIZE, dispatch_offset: 0 });
			expandedDispatches = new Set();
		} catch (err) {
			detail = null;
			error = err instanceof ApiError ? err.message : String(err);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		const id = agentId;
		void load(id);
		return onApiCacheUpdate(
			`/api/agents/${id}`,
			() => {
				void load(id);
			},
			{ prefix: true }
		);
	});

	const SOURCE_TOOLTIP: Record<AgentSource, string> = {
		observed: 'Auto-captured from request traffic; no on-disk definition confirmed.',
		file_backed: 'Operator confirmed this agent has a matching .md file.',
		dynamic: 'One-shot / dynamically generated by Claude.',
		builtin: 'Ships with the harness itself (e.g. the default catch-all agent).'
	};

	const SOURCE_SLUG: Record<AgentSource, string | undefined> = {
		observed: 'mode-observed',
		file_backed: 'mode-file-backed',
		dynamic: 'mode-dynamic',
		builtin: 'mode-builtin'
	};

	function sessionHref(sessionUuid: string): `/sessions/${string}` {
		return `/sessions/${encodeURIComponent(sessionUuid)}`;
	}

	function requestHref(
		sessionUuid: string,
		requestId: number
	): `/sessions/${string}/requests/${string}` {
		return `/sessions/${encodeURIComponent(sessionUuid)}/requests/${requestId}`;
	}
</script>

<svelte:head>
	<title>Agent — {detail?.name ?? agentId} — mike</title>
</svelte:head>

<section class="flex h-full flex-col overflow-hidden" data-slot="agent-detail-page">
	{#if loading}
		<div class="flex flex-col gap-4 p-6">
			<div class="h-8 w-1/3 animate-pulse rounded bg-[var(--surface-2)]"></div>
			<div class="h-32 animate-pulse rounded bg-[var(--surface-2)]"></div>
		</div>
	{:else if error !== null}
		<div class="m-6">
			<LoadErrorBanner
				title="Couldn't load this agent"
				detail={error}
				hint="The agent id may have been deleted, or the inspect API may be unreachable."
				onRetry={() => void load(agentId)}
				backHref="/agents"
				backLabel="Back to agents"
			/>
		</div>
	{:else if detail !== null}
		<header
			class="flex flex-col gap-1 border-b border-[var(--border-default)] px-6 py-4"
			data-slot="agent-detail-header"
		>
			<PageIntro title="Agent" learnMoreHref="/help/glossary#agent-registry">
				<span class="flex items-center gap-1.5">
					<span>One identity from the</span>
					<span class="inline-flex items-center gap-1">
						<span>agent registry</span>
						<HelpHint slug="agent-registry" />
					</span>
					<span>and every piece of evidence captured for it.</span>
				</span>
			</PageIntro>
			<div class="flex flex-wrap items-center gap-2">
				<h2 class="ad-name">{detail.name}</h2>
				<Tooltip text={SOURCE_TOOLTIP[detail.source]} slug={SOURCE_SLUG[detail.source]}>
					<span class="ad-chip-backing ad-chip-backing--bg">
						<span
							class="ad-chip"
							data-augmented-ui="tl-clip br-clip border"
							class:agent-source-observed={detail.source === 'observed'}
							class:agent-source-file_backed={detail.source === 'file_backed'}
							class:agent-source-dynamic={detail.source === 'dynamic'}
							class:agent-source-builtin={detail.source === 'builtin'}
							data-slot="agent-detail-source"
						>
							{detail.source === 'file_backed' ? 'file-backed' : detail.source}
						</span>
					</span>
				</Tooltip>
				<span class="inline-flex items-center gap-1 text-label text-[var(--text-3)]">
					{detail.prompt_version_count} prompt version{detail.prompt_version_count === 1 ? '' : 's'}
					<HelpHint slug="agent-version" />
				</span>
				<div class="ml-auto flex items-center gap-2">
					{#if !editing}
						<Button
							type="button"
							variant="outline"
							size="sm"
							onclick={() => detail !== null && openEditor(detail)}
							data-slot="agent-detail-edit-btn"
						>
							edit
						</Button>
						<Button
							type="button"
							variant="destructive"
							size="sm"
							onclick={openDeleteConfirm}
							data-slot="agent-detail-delete-btn"
						>
							delete
						</Button>
					{/if}
				</div>
			</div>
			{#if detail.aliases && detail.aliases.length > 0}
				<div class="flex flex-wrap items-center gap-1.5" data-slot="agent-detail-aliases">
					<span class="ad-caption">Also known as</span>
					{#each detail.aliases as alias (alias.alias)}
						<span class="ad-chip-backing ad-chip-backing--bg">
							<span
								class="ad-chip ad-chip--neutral"
								data-augmented-ui="tl-clip br-clip border"
								title={alias.kind === 'persona' ? 'Persona alias' : 'Name-variant alias'}
							>
								{alias.alias}
							</span>
						</span>
					{/each}
				</div>
			{/if}
			{#if detail.definition_path !== null && detail.definition_path.length > 0}
				<div class="font-mono text-label text-[var(--text-2)]">
					Defined in: <span data-slot="agent-detail-definition-path">{detail.definition_path}</span>
				</div>
			{/if}
			{#if detail.description !== null && detail.description.length > 0}
				<p class="mt-1 text-sm text-[var(--text-1)]" data-slot="agent-detail-description">
					{detail.description}
				</p>
			{/if}
			<div class="mt-1 flex flex-wrap items-center gap-2 text-label text-[var(--text-3)]">
				<span>created <span class="font-mono">{formatRelative(detail.created_at)}</span></span>
				<span>·</span>
				<span>last updated <span class="font-mono">{formatRelative(detail.updated_at)}</span></span>
			</div>

			{#if editing}
				<form
					class="ad-form-card mt-3 flex flex-col gap-2"
					data-augmented-ui="tl-clip br-clip border"
					data-slot="agent-detail-edit-form"
					onsubmit={(e) => {
						e.preventDefault();
						void save();
					}}
				>
					<label class="flex items-center gap-2 text-label text-[var(--text-2)]">
						Name
						<input type="text" class="ad-input h-8 w-64" bind:value={editName} />
					</label>
					<label class="flex items-center gap-2 text-label text-[var(--text-2)]">
						Definition path
						<input
							type="text"
							class="ad-input h-8 w-96"
							bind:value={editDefinitionPath}
							placeholder=".claude/agents/<name>.md"
						/>
					</label>
					<label class="flex items-start gap-2 text-label text-[var(--text-2)]">
						Description
						<textarea class="ad-input min-h-[3rem] w-96" bind:value={editDescription}></textarea>
					</label>
					<label class="flex items-center gap-2 text-label text-[var(--text-2)]">
						Source
						<select class="ad-input h-8" bind:value={editSource}>
							<option value="observed">observed</option>
							<option value="file_backed">file-backed</option>
							<option value="dynamic">dynamic</option>
							<option value="builtin">builtin</option>
						</select>
					</label>
					{#if saveError !== null}
						<p class="text-label text-[var(--status-danger-fg)]" role="alert">{saveError}</p>
					{/if}
					<div class="flex gap-2">
						<Button type="submit" size="sm" disabled={saveInFlight}>
							{saveInFlight ? 'saving…' : 'save'}
						</Button>
						<Button
							type="button"
							variant="outline"
							size="sm"
							onclick={cancelEdit}
							disabled={saveInFlight}
						>
							cancel
						</Button>
					</div>
				</form>
			{/if}

			{#if confirmDelete}
				<div
					class="ad-danger-card mt-3 flex flex-col gap-2"
					data-augmented-ui="tl-clip br-clip border"
					data-slot="agent-detail-delete-confirm"
				>
					<p class="text-sm text-[var(--text-1)]">
						Delete agent <span class="font-mono">{detail.name}</span>? Linked compositions stay
						(their <code>agent_id</code> becomes NULL); the captured evidence trail is preserved.
					</p>
					{#if deleteError !== null}
						<p class="text-label text-[var(--status-danger-fg)]" role="alert">{deleteError}</p>
					{/if}
					<div class="flex gap-2">
						<Button
							type="button"
							variant="destructive"
							size="sm"
							onclick={() => void performDelete()}
							disabled={deleting}
						>
							{deleting ? 'deleting…' : 'confirm delete'}
						</Button>
						<Button
							type="button"
							variant="outline"
							size="sm"
							onclick={() => (confirmDelete = false)}
							disabled={deleting}
						>
							cancel
						</Button>
					</div>
				</div>
			{/if}
		</header>

		<div
			class="flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto px-6 py-4"
			data-slot="agent-detail-body"
		>
			<section
				class="ad-section-band"
				data-augmented-ui="tl-clip br-clip border"
				data-slot="agent-dispatches-section"
			>
				<h3 class="ad-section-title">
					<Send class="size-3.5 text-[var(--data-teal-fg)]" aria-hidden="true" />
					Dispatches
					<HelpHint slug="agent-dispatches" />
					<span class="ad-section-count">({detail.dispatches.total_count})</span>
				</h3>
				{#if detail.dispatches.rows.length === 0}
					<p class="mt-2 text-label text-[var(--text-3)]" data-slot="agent-dispatches-empty">
						No dispatches captured yet for this agent.
					</p>
				{:else}
					<div class="mt-2 flex flex-col gap-2" data-slot="agent-dispatches-list">
						{#each detail.dispatches.rows as d (d.tool_use_id)}
							{@const isOpen = expandedDispatches.has(d.tool_use_id)}
							<div
								class="ad-card"
								data-augmented-ui="tl-clip br-clip border"
								data-slot="agent-dispatch-row"
								data-tool-use-id={d.tool_use_id}
							>
								<div class="ad-dispatch-row-header flex flex-wrap items-center gap-2 px-3 py-2">
									<button
										type="button"
										class="ad-dispatch-toggle"
										aria-expanded={isOpen}
										aria-label={isOpen
											? `Collapse prompt for dispatch ${d.tool_use_id}`
											: `Expand prompt for dispatch ${d.tool_use_id}`}
										onclick={() => toggleDispatch(d.tool_use_id)}
									></button>
									<Tooltip text={d.dispatched_at}>
										<span class="font-mono text-label text-[var(--text-2)]"
											>{formatRelative(d.dispatched_at)}</span
										>
									</Tooltip>
									<a
										class="font-mono text-label text-[var(--interactive-active-fg)] hover:underline"
										href={resolve(sessionHref(d.session_uuid))}
									>
										{shortSessionId(d.session_uuid)}
									</a>
									<a
										class="font-mono text-label text-[var(--interactive-active-fg)] hover:underline"
										href={resolve(requestHref(d.session_uuid, d.request_id))}
									>
										request #{d.request_id}
									</a>
								</div>
								{#if !isOpen}
									<p class="ad-expand-hint">click to expand prompt</p>
								{:else}
									<div class="border-t border-[var(--border-subtle)] px-3 py-2">
										<pre
											class="ad-prompt max-h-[480px] overflow-auto whitespace-pre-wrap break-words text-label text-[var(--text-1)]">{d.prompt}</pre>
									</div>
								{/if}
							</div>
						{/each}
					</div>
					{#if detail.dispatches.rows.length < detail.dispatches.total_count}
						<div class="mt-2 flex items-center gap-2">
							<Button
								type="button"
								variant="outline"
								size="sm"
								onclick={() => void loadMoreDispatches()}
								disabled={loadingMoreDispatches}
								data-slot="agent-dispatches-load-more"
							>
								{loadingMoreDispatches
									? 'loading…'
									: `load more (${detail.dispatches.total_count - detail.dispatches.rows.length} remaining)`}
							</Button>
							{#if loadMoreDispatchesError !== null}
								<span class="text-label text-[var(--status-danger-fg)]" role="alert"
									>{loadMoreDispatchesError}</span
								>
							{/if}
						</div>
					{/if}
				{/if}
			</section>

			<section
				class="ad-section-band"
				data-augmented-ui="tl-clip br-clip border"
				data-slot="agent-prompt-versions-section"
			>
				<h3 class="ad-section-title">
					<Fingerprint class="size-3.5 text-[var(--data-teal-fg)]" aria-hidden="true" />
					Prompt versions
					<HelpHint slug="agent-prompt-versions" />
					<span class="ad-section-count">({detail.prompt_versions.length})</span>
				</h3>
				{#if detail.prompt_versions.length === 0}
					<p class="mt-2 text-label text-[var(--text-3)]" data-slot="agent-prompt-versions-empty">
						No prompt versions captured yet.
					</p>
				{:else}
					<div class="mt-2 flex flex-col gap-2" data-slot="agent-prompt-versions-list">
						{#each detail.prompt_versions as v (v.content_sha256)}
							<a
								href={resolve('/prompts')}
								class="ad-card ad-card--link flex flex-wrap items-center gap-3"
								data-augmented-ui="tl-clip br-clip border"
								data-slot="agent-prompt-version-row"
							>
								<span class="ad-chip-backing ad-chip-backing--card">
									<span class="ad-chip ad-chip--neutral">
										sha {v.content_sha256}
									</span>
								</span>
								<span class="text-label text-[var(--text-3)]">
									first seen <span class="font-mono">{formatRelative(v.first_seen_at)}</span>
								</span>
								<span class="text-label text-[var(--text-3)]">
									last seen <span class="font-mono">{formatRelative(v.last_seen_at)}</span>
								</span>
								<span class="text-label text-[var(--text-3)]">
									{v.request_count} request{v.request_count === 1 ? '' : 's'}
								</span>
							</a>
						{/each}
					</div>
				{/if}
			</section>

			<section
				class="ad-section-band"
				data-augmented-ui="tl-clip br-clip border"
				data-slot="agent-instances-section"
			>
				<h3 class="ad-section-title">
					<Bot class="size-3.5 text-[var(--data-teal-fg)]" aria-hidden="true" />
					Instances
					<HelpHint slug="agent-instances" />
					<span class="ad-section-count">({detail.instances.length})</span>
				</h3>
				{#if detail.instances.length === 0}
					<p class="mt-2 text-label text-[var(--text-3)]" data-slot="agent-instances-empty">
						No sub-agent instances captured yet.
					</p>
				{:else}
					<div class="mt-2 flex flex-col gap-2" data-slot="agent-instances-list">
						{#each detail.instances as inst (inst.id)}
							<div
								class="ad-card ad-card--pad flex flex-wrap items-center gap-3"
								data-augmented-ui="tl-clip br-clip border"
								data-slot="agent-instance-row"
							>
								<a
									class="font-mono text-label text-[var(--interactive-active-fg)] hover:underline"
									href={resolve(sessionHref(inst.session_uuid))}
								>
									{shortSessionId(inst.session_uuid)}
								</a>
								<span class="text-label text-[var(--text-2)]"
									>{inst.subagent_persona !== null && inst.subagent_persona.length > 0
										? inst.subagent_persona
										: '—'}</span
								>
								<span class="text-label text-[var(--text-3)]">
									started <span class="font-mono"
										>{inst.started_at !== null ? formatRelative(inst.started_at) : '—'}</span
									>
								</span>
								{#if inst.first_request_id !== null}
									<a
										class="font-mono text-label text-[var(--interactive-active-fg)] hover:underline"
										href={resolve(requestHref(inst.session_uuid, inst.first_request_id))}
									>
										request #{inst.first_request_id}
									</a>
								{:else}
									<span class="font-mono text-label text-[var(--text-3)]">—</span>
								{/if}
							</div>
						{/each}
					</div>
				{/if}
			</section>

			{#if detail.compositions.length > 0}
				<section
					class="ad-section-band"
					data-augmented-ui="tl-clip br-clip border"
					data-slot="agent-compositions-section"
				>
					<h3 class="ad-section-title">
						<Layers class="size-3.5 text-[var(--data-teal-fg)]" aria-hidden="true" />
						Compositions
						<span class="ad-section-count">({detail.compositions.length})</span>
					</h3>
					<div
						class="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1.5 text-label text-[var(--text-3)]"
						data-slot="agent-detail-legend"
						aria-label="Composition legend"
					>
						<Tooltip text="Composition fingerprint — click a version to inspect it.">
							<span class="font-mono">sha256</span>
						</Tooltip>
						<Tooltip
							text="How this version's outbound system block is treated."
							slug="composition-mode"
						>
							<span class="font-mono">mode</span>
						</Tooltip>
					</div>
					<div class="mt-1 flex flex-col gap-2" data-slot="agent-compositions-list">
						{#each detail.compositions as comp, idx (comp.id)}
							<a
								href={resolve('/prompts')}
								class="ad-card ad-card--link flex items-start gap-3"
								data-augmented-ui="tl-clip br-clip border"
								data-slot="composition-row"
								data-composition-id={comp.id}
							>
								<div class="min-w-0 flex-1">
									<div class="flex flex-wrap items-center gap-2">
										<span class="ad-chip-backing ad-chip-backing--card">
											<span class="ad-chip ad-chip--neutral">v{idx + 1}</span>
										</span>
										<span class="text-sm font-medium text-[var(--text-strong)]">{comp.label}</span>
										<span class="ad-chip-backing ad-chip-backing--card">
											<span class="ad-chip ad-chip--neutral">{comp.mode}</span>
										</span>
										<span class="ad-comp-sha font-mono text-label text-[var(--text-3)]">
											sha {comp.prompt_sha256}
										</span>
										<span class="text-label text-[var(--text-3)]">
											· {comp.applied_count_24h} req/24h
										</span>
									</div>
									<div
										class="mt-0.5 flex flex-wrap items-center gap-2 text-label text-[var(--text-3)]"
									>
										<span
											>captured <span class="font-mono">{formatRelative(comp.created_at)}</span
											></span
										>
										<span>·</span>
										<span
											>last updated <span class="font-mono">{formatRelative(comp.updated_at)}</span
											></span
										>
									</div>
								</div>
							</a>
						{/each}
					</div>
				</section>
			{/if}
		</div>
	{/if}
</section>

<style>
	.ad-name {
		margin: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		font-size: 1.125rem;
		letter-spacing: -0.02em;
		color: var(--text-strong);
	}

	.ad-caption {
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}

	.ad-section-title {
		display: flex;
		align-items: center;
		gap: 6px;
		margin: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		font-size: var(--text-sm, 0.8125rem);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--text-strong);
	}
	.ad-section-count {
		font-family: 'JetBrains Mono Variable', monospace;
		font-weight: 400;
		text-transform: none;
		letter-spacing: normal;
		color: var(--text-3);
	}

	.ad-section-band {
		--edge: 6px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(135deg, var(--data-teal-fg) 0%, var(--border-default) 60%);
		background: var(--surface-card);
		padding: var(--edge-pad, calc(var(--edge) + var(--space-1)));
	}

	.ad-chip-backing {
		display: inline-flex;
		background: var(--surface-bg);
	}
	.ad-chip-backing--bg {
		background: var(--surface-bg);
	}
	.ad-chip-backing--card {
		background: var(--surface-card);
	}
	.ad-chip {
		--edge: 2px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		display: inline-block;
		padding: calc(var(--edge) - 1px) calc(var(--edge) + 6px);
		font-family: 'JetBrains Mono Variable', monospace;
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		overflow-wrap: anywhere;
	}
	.ad-chip--neutral {
		--aug-border-bg: var(--border-subtle);
		color: var(--text-2);
		text-transform: none;
		letter-spacing: normal;
		background: var(--surface-card);
	}

	.agent-source-observed {
		--aug-border-bg: var(--status-warning-border);
		color: var(--status-warning-fg);
		background:
			linear-gradient(0deg, var(--status-warning-bg), var(--status-warning-bg)), var(--surface-bg);
	}
	.agent-source-file_backed {
		--aug-border-bg: var(--status-success-border);
		color: var(--status-success-fg);
		background:
			linear-gradient(0deg, var(--status-success-bg), var(--status-success-bg)), var(--surface-bg);
	}
	.agent-source-dynamic {
		--aug-border-bg: var(--status-info-border);
		color: var(--status-info-fg);
		background:
			linear-gradient(0deg, var(--status-info-bg), var(--status-info-bg)), var(--surface-bg);
	}
	.agent-source-builtin {
		--aug-border-bg: var(--status-muted-border);
		color: var(--status-muted-fg);
		background:
			linear-gradient(0deg, var(--status-muted-bg), var(--status-muted-bg)), var(--surface-bg);
	}

	.ad-card {
		--edge: 6px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--data-teal-fg) 0%,
			var(--data-teal-border) 35%,
			var(--border-default) 70%
		);
		background: var(--surface-card);
	}
	.ad-card--link {
		display: flex;
		padding: calc(var(--edge) + 6px);
		text-decoration: none;
	}

	.ad-card--pad {
		padding: calc(var(--edge) + 6px);
	}
	.ad-card--link:hover {
		--aug-border-bg: linear-gradient(
			135deg,
			var(--data-teal-fg) 0%,
			var(--interactive-active-border) 60%
		);
	}
	.ad-card--link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.ad-comp-sha {
		overflow-wrap: anywhere;
	}

	.ad-dispatch-row-header {
		position: relative;
	}
	.ad-dispatch-toggle {
		position: absolute;
		inset: 0;
		z-index: 0;
		margin: 0;
		padding: 0;
		border: 0;
		background: transparent;
		cursor: pointer;
	}
	.ad-dispatch-toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.ad-dispatch-row-header :global(.tooltip-trigger),
	.ad-dispatch-row-header > a {
		position: relative;
		z-index: 1;
	}

	.ad-expand-hint {
		margin: 0;
		padding: 0 12px 8px;
		font-size: var(--text-label);
		color: var(--text-faint, var(--text-3));
	}
	.ad-prompt {
		background: var(--surface-2);
		padding: 8px;
	}

	.ad-form-card {
		--edge: 8px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(135deg, var(--border-strong) 0%, var(--border-default) 60%);
		padding: calc(var(--edge) + 4px);
		background: var(--surface-2);
	}

	.ad-danger-card {
		--edge: 8px;
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

	.ad-input {
		--edge: 4px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		padding: calc(var(--edge) - 1px) calc(var(--edge) + 4px);
		background: var(--surface-bg);
		font-family: 'JetBrains Mono Variable', monospace;
		font-size: 0.8125rem;
		color: var(--text-1);
		outline: none;
	}
	.ad-input:focus-visible {
		--aug-border-bg: var(--interactive-focus-ring);
	}
</style>
