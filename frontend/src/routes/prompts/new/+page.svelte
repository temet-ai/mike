<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import {
		ApiError,
		createPromptComposition,
		getPromptClassMain,
		getPromptClassSubagent,
		getRequestDetail,
		UNRESOLVED_PERSONA_SEGMENT
	} from '$lib/api';
	import type { PromptClassDetailDto, PromptCompositionBlock, RequestDetail } from '$lib/types';
	import BlockCard from '$lib/components/prompts/BlockCard.svelte';
	import ClassChip from '$lib/components/prompts/ClassChip.svelte';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import HelpHint from '$lib/components/shared/HelpHint.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import { suggestCompositionName } from '$lib/prompts/suggestName';

	type PrefillSource =
		| { kind: 'request'; id: number }
		| { kind: 'class'; lane: 'main' }
		| { kind: 'class'; lane: 'subagent'; personaSegment: string };

	type ClassSource = Extract<PrefillSource, { kind: 'class' }>;

	function parseRequestSource(
		raw: string | null
	): Extract<PrefillSource, { kind: 'request' }> | null {
		if (raw === null || raw.length === 0) return null;
		const match = raw.match(/^request:(\d+)$/);
		if (match === null) return null;
		const idStr = match[1];
		if (idStr === undefined) return null;
		const id = Number.parseInt(idStr, 10);
		if (!Number.isFinite(id) || id < 0) return null;
		return { kind: 'request', id };
	}

	function parsePrefillSource(params: URLSearchParams): PrefillSource | null {
		const requestSource = parseRequestSource(params.get('from'));
		if (requestSource !== null) return requestSource;
		const persona = params.get('persona');
		if (persona !== null && persona.length > 0) {
			return { kind: 'class', lane: 'subagent', personaSegment: persona };
		}
		if (params.get('lane') === 'main') {
			return { kind: 'class', lane: 'main' };
		}
		return null;
	}

	const source = $derived(parsePrefillSource(new URLSearchParams(page.url.search)));

	function isRealPersonaSegment(p: string | null): p is string {
		return p !== null && p.length > 0 && p !== UNRESOLVED_PERSONA_SEGMENT;
	}

	function classLabel(s: ClassSource): string {
		if (s.lane === 'main') return 'Claude Code (main agent)';
		return s.personaSegment === UNRESOLVED_PERSONA_SEGMENT
			? 'Subagent (persona unresolved)'
			: s.personaSegment;
	}

	/** A class detail page as a Kit 3 pathname, which has no leading slash. */
	type ClassPath = 'prompts/main' | `prompts/subagent/${string}`;

	function classDetailPath(s: ClassSource): ClassPath {
		return s.lane === 'main'
			? 'prompts/main'
			: `prompts/subagent/${encodeURIComponent(s.personaSegment)}`;
	}

	/** `resolve()` takes one pathname shape per call, so the main class is resolved by route. */
	function resolveClassPath(path: ClassPath): string {
		return path === 'prompts/main' ? resolve('/prompts/main') : resolve(path);
	}

	async function sha256Hex(input: string): Promise<string> {
		const encoder = new TextEncoder();
		const buf = await crypto.subtle.digest('SHA-256', encoder.encode(input));
		const bytes = new Uint8Array(buf);
		let hex = '';
		for (let i = 0; i < bytes.length; i += 1) {
			const byte = bytes[i] ?? 0;
			hex += byte.toString(16).padStart(2, '0');
		}
		return hex;
	}

	let loading = $state(false);
	let initialized = $state(false);
	let loadError = $state<string | null>(null);
	let resolvedRequestId = $state<number | null>(null);
	let resolvedSessionId = $state<string | null>(null);

	let classResolutionError = $state<{
		kind: 'not-found' | 'unresolvable';
		label: string;
		classPath: ClassPath;
		detail: string | null;
	} | null>(null);

	let label = $state('');
	let freeAgentName = $state('');
	let agentDefinitionPath = $state('');
	let draftBlocks = $state<PromptCompositionBlock[]>([]);
	let promptSha256 = $state<string>('');
	let promptSha256Source = $state<'server' | 'computed' | 'fallback' | null>(null);

	let saveInFlight = $state(false);
	let saveError = $state<string | null>(null);

	const personaParam = $derived(page.url.searchParams.get('persona'));
	const laneParam = $derived(page.url.searchParams.get('lane'));

	type BoundIdentity =
		{ kind: 'main' } | { kind: 'subagent'; persona: string } | { kind: 'subagent-unresolved' };

	const boundIdentity: BoundIdentity | null = $derived(
		isRealPersonaSegment(personaParam)
			? { kind: 'subagent', persona: personaParam }
			: personaParam === UNRESOLVED_PERSONA_SEGMENT
				? { kind: 'subagent-unresolved' }
				: laneParam === 'main'
					? { kind: 'main' }
					: null
	);

	const boundIdentityLabel = $derived.by(() => {
		if (boundIdentity === null) return '';
		if (boundIdentity.kind === 'subagent') return boundIdentity.persona;
		if (boundIdentity.kind === 'main') return 'Claude Code (main agent)';
		return 'Subagent (persona unresolved)';
	});

	function computeBoundIdentityPath(): ClassPath | undefined {
		if (boundIdentity === null) return undefined;
		if (boundIdentity.kind === 'subagent') {
			return `prompts/subagent/${encodeURIComponent(boundIdentity.persona)}`;
		}
		if (boundIdentity.kind === 'main') return 'prompts/main';
		return `prompts/subagent/${encodeURIComponent(UNRESOLVED_PERSONA_SEGMENT)}`;
	}
	const boundIdentityPath = $derived(computeBoundIdentityPath());

	const effectiveAgentName: string | null = $derived(
		boundIdentity?.kind === 'subagent'
			? boundIdentity.persona
			: boundIdentity?.kind === 'main' || boundIdentity?.kind === 'subagent-unresolved'
				? null
				: freeAgentName.trim().length > 0
					? freeAgentName.trim()
					: null
	);

	/** The label is prefilled once; clearing it afterwards leaves it empty for the operator to type. */
	let labelPrefilled = false;
	$effect(() => {
		if (labelPrefilled) return;
		if (boundIdentity?.kind === 'subagent') {
			labelPrefilled = true;
			label = boundIdentity.persona;
			return;
		}
		if (boundIdentity?.kind === 'main') {
			labelPrefilled = true;
			label = 'Claude Code (main agent)';
			return;
		}
		// No persona to name it by: suggest a name from the agent instructions once they load.
		const instructions = draftBlocks.find((b) => b.index === 2)?.text;
		if (instructions === undefined) return;
		labelPrefilled = true;
		label = suggestCompositionName(instructions) ?? boundIdentityLabel;
	});

	async function deriveBlocks(detail: RequestDetail): Promise<{
		blocks: PromptCompositionBlock[];
		promptSha256: string;
		promptSha256Source: 'server' | 'computed' | 'fallback';
	}> {
		const projection = detail.system_blocks;
		if (projection && projection.blocks.length > 0) {
			const blocks: PromptCompositionBlock[] = projection.blocks.map((b) => ({
				index: b.index,
				text: b.text,
				cache_control: b.cache_control
			}));
			if (projection.prompt_sha256 !== null && projection.prompt_sha256.length > 0) {
				return {
					blocks,
					promptSha256: projection.prompt_sha256,
					promptSha256Source: 'server'
				};
			}
			const skeleton = blocks[2]?.text ?? '';
			if (skeleton.length > 0) {
				return {
					blocks,
					promptSha256: await sha256Hex(skeleton),
					promptSha256Source: 'computed'
				};
			}
			const flat = blocks
				.map((b) => b.text)
				.join('\n\n')
				.trim();
			return {
				blocks,
				promptSha256: await sha256Hex(flat),
				promptSha256Source: 'fallback'
			};
		}

		return {
			blocks: [],
			promptSha256: '',
			promptSha256Source: 'fallback'
		};
	}

	async function resolveClassSource(s: ClassSource): Promise<number | null> {
		const displayLabel = classLabel(s);
		const classPath = classDetailPath(s);
		let detail: PromptClassDetailDto;
		try {
			detail =
				s.lane === 'main'
					? await getPromptClassMain({ limit: 1 })
					: await getPromptClassSubagent(s.personaSegment, { limit: 1 });
		} catch (err) {
			classResolutionError = {
				kind: 'not-found',
				label: displayLabel,
				classPath,
				detail: err instanceof ApiError ? err.message : String(err)
			};
			return null;
		}
		const observation = detail.recent_observations[0];
		if (observation === undefined) {
			classResolutionError = {
				kind: 'unresolvable',
				label: displayLabel,
				classPath,
				detail: null
			};
			return null;
		}
		const id = Number.parseInt(observation.request_id, 10);
		if (!Number.isFinite(id) || id < 0) {
			classResolutionError = {
				kind: 'unresolvable',
				label: displayLabel,
				classPath,
				detail: null
			};
			return null;
		}
		return id;
	}

	async function loadSource(s: PrefillSource | null): Promise<void> {
		if (s === null) {
			draftBlocks = [];
			promptSha256 = '';
			promptSha256Source = null;
			loadError = null;
			classResolutionError = null;
			resolvedRequestId = null;
			resolvedSessionId = null;
			initialized = true;
			return;
		}
		loading = true;
		loadError = null;
		classResolutionError = null;
		resolvedRequestId = null;
		resolvedSessionId = null;
		try {
			const requestId = s.kind === 'request' ? s.id : await resolveClassSource(s);
			if (requestId === null) return;
			resolvedRequestId = requestId;
			const detail = await getRequestDetail(requestId);
			resolvedSessionId = detail.session_id;
			const {
				blocks,
				promptSha256: derived,
				promptSha256Source: derivedSource
			} = await deriveBlocks(detail);
			draftBlocks = blocks;
			promptSha256 = derived;
			promptSha256Source = derivedSource;
		} catch (err) {
			draftBlocks = [];
			promptSha256 = '';
			promptSha256Source = null;
			loadError = err instanceof ApiError ? err.message : String(err);
		} finally {
			loading = false;
			initialized = true;
		}
	}

	$effect(() => {
		void loadSource(source);
	});

	function railRoleFor(idx: number): 'preamble' | 'skeleton' {
		return idx === 2 ? 'skeleton' : 'preamble';
	}

	const meaningfulBlockCount = $derived(Math.max(0, draftBlocks.length - 2));
	function labelFor(idx: number): string {
		if (meaningfulBlockCount <= 1) return 'Agent instructions';
		return `Agent instructions — part ${idx - 1}`;
	}

	const trimmedLabel = $derived(label.trim());
	const hasBlocks = $derived(draftBlocks.length > 0);
	const canSave = $derived(
		!saveInFlight &&
			trimmedLabel.length > 0 &&
			promptSha256.length > 0 &&
			hasBlocks &&
			loadError === null
	);

	const saveDisabledReason: string | null = $derived.by(() => {
		if (saveInFlight || canSave) return null;
		if (trimmedLabel.length === 0) return 'enter a label to save';
		if (!hasBlocks) return 'no blocks to save yet';
		if (promptSha256.length === 0) return 'waiting for a prompt hash';
		return null;
	});

	const showEmpty = $derived(initialized && source === null);
	const showLoading = $derived(loading);
	const showClassNotFound = $derived(
		initialized && source !== null && classResolutionError?.kind === 'not-found'
	);
	const showClassUnresolvable = $derived(
		initialized && source !== null && classResolutionError?.kind === 'unresolvable'
	);
	const showLoadError = $derived(
		initialized && source !== null && classResolutionError === null && loadError !== null
	);
	const showNoBlocks = $derived(
		initialized &&
			source !== null &&
			classResolutionError === null &&
			loadError === null &&
			!hasBlocks
	);
	const showForm = $derived(
		initialized &&
			source !== null &&
			classResolutionError === null &&
			loadError === null &&
			hasBlocks
	);

	async function save(): Promise<void> {
		if (!canSave) return;
		saveInFlight = true;
		saveError = null;
		try {
			const trimmedDefinitionPath = agentDefinitionPath.trim();
			const envelope = await createPromptComposition({
				prompt_sha256: promptSha256,
				label: trimmedLabel,
				blocks: draftBlocks.map((b) =>
					b.index <= 1
						? { index: b.index, text: '', cache_control: null }
						: { index: b.index, text: b.text, cache_control: b.cache_control }
				),
				...(effectiveAgentName !== null ? { agent_name: effectiveAgentName } : {}),
				...(trimmedDefinitionPath.length > 0
					? { agent_definition_path: trimmedDefinitionPath }
					: {})
			});
			await goto(
				boundIdentityPath === undefined
					? resolve(envelope.composition.lane === 'main' ? '/prompts/main' : '/prompts')
					: boundIdentityPath === 'prompts/main'
						? resolve('/prompts/main')
						: resolve(boundIdentityPath)
			);
		} catch (err) {
			saveError = err instanceof ApiError ? err.message : String(err);
		} finally {
			saveInFlight = false;
		}
	}
</script>

<svelte:head>
	<title>New composition — mike</title>
</svelte:head>

<section
	class="flex h-full flex-col overflow-hidden"
	data-slot="prompt-new-page"
	aria-label="New composition"
>
	{#if showLoading && !showForm}
		<div class="flex flex-col gap-4 p-6" data-slot="prompt-new-loading">
			<div class="h-8 w-1/3 animate-pulse rounded bg-[var(--surface-2)]"></div>
			<div class="h-32 animate-pulse rounded bg-[var(--surface-2)]"></div>
			<div class="h-32 animate-pulse rounded bg-[var(--surface-2)]"></div>
		</div>
	{:else if showEmpty}
		<div
			class="flex min-h-0 flex-1 flex-col items-center justify-center p-6"
			data-slot="prompt-new-empty"
		>
			<div class="pr-new-panel" data-augmented-ui="tl-clip br-clip border">
				<h2 class="pr-new-heading">Choose what to register</h2>
				<p class="text-label text-[var(--text-3)]">
					Open the prompt catalogue and choose "Register this" on an unregistered row — this page
					fills in the form from that row's captured traffic.
				</p>
				<span class="pr-back-link-backing mt-2">
					<a
						href={resolve('/prompts')}
						class="pr-back-link"
						data-augmented-ui="tr-clip bl-clip border"
					>
						open prompt catalogue
					</a>
				</span>
			</div>
		</div>
	{:else if showClassNotFound}
		<div
			class="flex min-h-0 flex-1 flex-col items-center justify-center p-6"
			data-slot="prompt-new-class-not-found"
		>
			<div class="w-full max-w-lg">
				<LoadErrorBanner
					title={`Couldn't find ${classResolutionError?.label ?? ''}`}
					detail={classResolutionError?.detail ?? undefined}
					hint="This wasn't recognized — it may have been removed or renamed. Pick another row from the catalogue."
					backHref="/prompts"
					backLabel="Back to all prompts"
				/>
			</div>
		</div>
	{:else if showClassUnresolvable}
		<div
			class="flex min-h-0 flex-1 flex-col items-center justify-center p-6"
			data-slot="prompt-new-class-unresolvable"
		>
			<div class="pr-new-panel" data-augmented-ui="tl-clip br-clip border">
				<h2 class="pr-new-heading">Nothing captured yet for {classResolutionError?.label}</h2>
				<p class="text-label text-[var(--text-3)]">
					There's no captured traffic to prefill from yet. Once a matching request comes in, you'll
					be able to register it here.
				</p>
				<div class="mt-2 flex flex-wrap items-center justify-center gap-2">
					<span class="pr-back-link-backing">
						<a
							href={classResolutionError === null
								? resolve('/prompts')
								: classResolutionError.classPath === 'prompts/main'
									? resolve('/prompts/main')
									: resolve(classResolutionError.classPath)}
							class="pr-back-link"
							data-augmented-ui="tr-clip bl-clip border"
						>
							open this row
						</a>
					</span>
					<span class="pr-back-link-backing">
						<a
							href={resolve('/prompts')}
							class="pr-back-link"
							data-augmented-ui="tr-clip bl-clip border"
						>
							back to all prompts
						</a>
					</span>
				</div>
			</div>
		</div>
	{:else if showLoadError}
		<div
			class="flex min-h-0 flex-1 flex-col items-center justify-center p-6"
			data-slot="prompt-new-load-error"
		>
			<div class="w-full max-w-lg">
				<LoadErrorBanner
					title="Couldn't load the captured request"
					detail={loadError ?? undefined}
					hint={`Request #${resolvedRequestId ?? '?'} may have been removed. Try again, or pick another row from the catalogue.`}
					onRetry={() => void loadSource(source)}
					backHref="/prompts"
					backLabel="Back to all prompts"
				/>
			</div>
		</div>
	{:else if showNoBlocks}
		<div
			class="flex min-h-0 flex-1 flex-col items-center justify-center p-6"
			data-slot="prompt-new-no-blocks"
		>
			<div class="pr-new-panel" data-augmented-ui="tl-clip br-clip border">
				<h2 class="pr-new-heading">No prompt content on request #{resolvedRequestId}</h2>
				<p class="text-label text-[var(--text-3)]">
					This request was captured but has no system prompt to prefill from. Try a different
					request captured for this row.
				</p>
				<div class="mt-2 flex flex-wrap items-center justify-center gap-2">
					{#if resolvedSessionId !== null && resolvedRequestId !== null}
						<span class="pr-back-link-backing">
							<a
								href={resolve(
									`sessions/${encodeURIComponent(resolvedSessionId)}/requests/${resolvedRequestId}`
								)}
								class="pr-back-link"
								data-augmented-ui="tr-clip bl-clip border"
							>
								view request
							</a>
						</span>
					{/if}
					<span class="pr-back-link-backing">
						<a
							href={resolve('/prompts')}
							class="pr-back-link"
							data-augmented-ui="tr-clip bl-clip border"
						>
							back to all prompts
						</a>
					</span>
				</div>
			</div>
		</div>
	{:else if showForm}
		<header
			class="flex flex-col gap-3 border-b border-[var(--border-default)] px-6 py-4"
			data-slot="prompt-new-header"
		>
			<div class="flex items-center gap-3">
				<PageIntro title="New composition" learnMoreHref="/help/glossary#composition">
					<span class="flex flex-wrap items-center gap-1.5">
						<span>Register a new</span>
						<span class="inline-flex items-center gap-1">
							<span>composition</span>
							<HelpHint slug="composition" />
						</span>
						<span>prefilled from request #{resolvedRequestId}.</span>
						{#if resolvedSessionId !== null && resolvedRequestId !== null}
							<a
								href={resolve(
									`sessions/${encodeURIComponent(resolvedSessionId)}/requests/${resolvedRequestId}`
								)}
								class="pr-source-link"
								data-slot="prompt-new-source-link"
							>
								view captured request →
							</a>
						{/if}
					</span>
				</PageIntro>
				{#if promptSha256Source === 'fallback'}
					<span
						class="badge-nx badge-warning"
						title="block[2] not found in source — prompt_sha256 derived from the full system prompt; traffic will not match until block[2] is present."
					>
						<span class="badge-nx-dot"></span>
						prompt_sha256 fallback
					</span>
				{/if}
			</div>
			<div class="flex flex-wrap items-center gap-3">
				<label
					class="flex items-center gap-2 text-label text-[var(--text-2)]"
					for="prompt-new-label"
				>
					<span class="flex items-center gap-1">
						Label
						<Tooltip
							text="A human-friendly name for this composition, shown in the prompt-classes list."
						>
							<span class="text-[var(--text-3)]">(?)</span>
						</Tooltip>
					</span>
					<input
						id="prompt-new-label"
						type="text"
						class="h-8 w-64 rounded border border-[var(--border-default)] bg-[var(--surface-bg)] px-2 font-mono text-sm text-[var(--text-1)] outline-none focus-visible:ring-2 focus-visible:ring-[var(--ring)]"
						placeholder="e.g. main-agent"
						bind:value={label}
					/>
				</label>
				<span
					class="inline-flex items-center gap-1 font-mono text-label text-[var(--text-3)]"
					title={promptSha256}
				>
					sha256 · {promptSha256.slice(0, 12)}…{promptSha256.slice(-4)}
					<HelpHint slug="prompt-sha256" />
				</span>
			</div>
			<div class="flex flex-wrap items-center gap-3" data-slot="prompt-new-agent-fields">
				{#if boundIdentity !== null}
					<div
						class="flex items-center gap-2 text-label text-[var(--text-2)]"
						data-slot="prompt-new-bound-identity"
					>
						<span>Registering for</span>
						<ClassChip
							label={boundIdentityLabel}
							href={boundIdentityPath === undefined
								? undefined
								: resolveClassPath(boundIdentityPath)}
							isActive
						/>
						<Tooltip
							text="This composition is linked to the class shown here — the agent name is fixed to that class's identity so registration can't silently create a duplicate row."
						>
							<span class="text-[var(--text-3)]">(?)</span>
						</Tooltip>
					</div>
				{:else}
					<label
						class="flex items-center gap-2 text-label text-[var(--text-2)]"
						for="prompt-new-agent-name"
					>
						Agent name
						<input
							id="prompt-new-agent-name"
							type="text"
							class="h-8 w-64 rounded border border-[var(--border-default)] bg-[var(--surface-bg)] px-2 font-mono text-sm text-[var(--text-1)] outline-none focus-visible:ring-2 focus-visible:ring-[var(--ring)]"
							placeholder="optional · must exactly match the observed persona to link a class"
							bind:value={freeAgentName}
						/>
					</label>
				{/if}
				<label
					class="flex items-center gap-2 text-label text-[var(--text-2)]"
					for="prompt-new-agent-definition-path"
				>
					Definition path
					<input
						id="prompt-new-agent-definition-path"
						type="text"
						class="h-8 w-96 rounded border border-[var(--border-default)] bg-[var(--surface-bg)] px-2 font-mono text-sm text-[var(--text-1)] outline-none focus-visible:ring-2 focus-visible:ring-[var(--ring)]"
						placeholder="optional · e.g. .claude/agents/frontend-developer.md"
						bind:value={agentDefinitionPath}
					/>
				</label>
			</div>
		</header>

		<div
			class="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-6 py-4"
			data-slot="prompt-new-blocks"
		>
			<div class="flex items-center justify-between gap-3" data-slot="prompt-new-blocks-head">
				<h3 class="pr-blocks-title">Blocks</h3>
				<span class="font-mono text-label text-[var(--text-3)]">
					{meaningfulBlockCount} block{meaningfulBlockCount === 1 ? '' : 's'} selected
				</span>
			</div>
			{#each draftBlocks.filter((draft) => draft.index >= 2) as draft (draft.index)}
				<BlockCard
					label={labelFor(draft.index)}
					bind:text={draft.text}
					badge="draft"
					railRole={railRoleFor(draft.index)}
					editable
				/>
			{/each}
		</div>

		{#if saveError}
			<div
				class="border-t border-[var(--status-danger-border)] bg-[var(--status-danger-bg)] px-6 py-2 text-label text-[var(--status-danger-fg)]"
				data-slot="prompt-new-save-error"
				role="alert"
			>
				save failed: {saveError}
			</div>
		{/if}

		<footer
			class="flex items-center gap-2 border-t border-[var(--border-default)] bg-[var(--surface-card)] px-6 py-3"
			data-slot="prompt-new-footer"
		>
			<span class="pr-footer-btn-backing">
				<button
					type="button"
					class="pr-save-btn"
					data-augmented-ui="tr-clip bl-clip border"
					disabled={!canSave}
					onclick={() => void save()}
				>
					{saveInFlight ? 'saving…' : 'save'}
				</button>
			</span>
			<span class="pr-footer-btn-backing">
				<a
					href={resolve('/prompts')}
					class="pr-cancel-btn"
					data-augmented-ui="tr-clip bl-clip border"
				>
					cancel
				</a>
			</span>
			{#if saveDisabledReason}
				<span
					class="ml-auto font-mono text-label text-[var(--text-3)]"
					data-slot="prompt-new-save-hint"
				>
					{saveDisabledReason}
				</span>
			{/if}
		</footer>
	{/if}
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

	.pr-new-panel {
		--edge: 10px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--data-teal-fg) 0%,
			var(--data-teal-border) 35%,
			var(--border-default) 70%
		);
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 12px;
		max-width: 32rem;
		background: var(--surface-card);
		padding: calc(var(--edge) + 4px);
		text-align: center;
	}
	.pr-new-heading {
		margin: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		font-size: 1rem;
		letter-spacing: -0.02em;
		color: var(--text-strong);
	}

	.pr-source-link {
		display: inline-flex;
		align-items: center;
		color: var(--funk-cyan);
		text-underline-offset: 2px;
		transition: color 0.12s ease;
	}
	.pr-source-link:hover,
	.pr-source-link:focus-visible {
		text-decoration: underline;
	}
	.pr-source-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.pr-blocks-title {
		margin: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		font-size: 0.8125rem;
		letter-spacing: -0.01em;
		color: var(--text-strong);
	}

	.pr-footer-btn-backing {
		display: inline-flex;
		background: var(--surface-card);
	}
	.pr-save-btn,
	.pr-cancel-btn {
		--edge: 8px;
		--aug-tl: 0;
		--aug-tr: var(--edge);
		--aug-br: 0;
		--aug-bl: var(--edge);
		--aug-border-all: 1px;
		display: inline-flex;
		height: 32px;
		align-items: center;
		padding: 0 14px;
		font-size: var(--text-label);
		font-weight: 600;
		text-decoration: none;
		cursor: pointer;
		transition:
			color 0.12s ease,
			opacity 0.12s ease;
	}
	.pr-save-btn {
		--aug-border-bg: var(--interactive-active-border);
		border: none;
		background: var(--interactive-active-fg);
		color: var(--surface-bg);
	}
	.pr-save-btn:hover:not(:disabled),
	.pr-save-btn:focus-visible {
		opacity: 0.9;
	}
	.pr-save-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	.pr-cancel-btn {
		--aug-border-bg: var(--border-default);
		background: var(--surface-card);
		color: var(--text-1);
	}
	.pr-cancel-btn:hover,
	.pr-cancel-btn:focus-visible {
		color: var(--text-strong);
		--aug-border-bg: var(--interactive-active-border);
	}
	.pr-save-btn:focus-visible,
	.pr-cancel-btn:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
</style>
