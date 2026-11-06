<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { getContext } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { PanelLeftClose, PanelLeftOpen } from '@lucide/svelte';
	import { ApiError, dryRunFilterRules, getRequestDetail, listProjects } from '$lib/api';
	import { FILTER_RULES_STORE_KEY, type FilterRulesStore } from '$lib/stores/filterRules.svelte';
	import type {
		FilterAction,
		FilterDryRunResponse,
		FilterRuleDto,
		ProjectSummaryDto,
		RequestDetail,
		SystemBlock
	} from '$lib/types';
	import { showsSessionMainNote, systemPromptPlaceholder } from './systemPromptPlaceholder';
	import type { AssistantProposal } from '$lib/api/assistant';
	import { buildPreviewRules } from './proposalRules';
	import { resolveDryRunSessionMessage } from './lastUserMessage';
	import WorkbenchSourcePane, { type PromptLens } from './WorkbenchSourcePane.svelte';
	import WorkbenchRuleStack from './WorkbenchRuleStack.svelte';
	import WorkbenchLanding from './WorkbenchLanding.svelte';
	import WorkbenchNavigator from './WorkbenchNavigator.svelte';
	import WorkbenchAssistantPanel, { type AssistantDraftHint } from './WorkbenchAssistantPanel.svelte';
	import WorkbenchDivider from './WorkbenchDivider.svelte';
	import WorkbenchRail from './WorkbenchRail.svelte';
	import WorkbenchProjectPicker from './WorkbenchProjectPicker.svelte';
	import {
		classForTurnOrigin,
		classTitle,
		parseReleaseFromBlockText,
		type RequestClassIdentity
	} from './promptCatalogue';
	import { createPromptCatalogue } from './promptCatalogue.svelte';
	import { paragraphContainsRule, paragraphIndicesTouchedByRule } from './rule-matching';
	import { buildPromptOutline } from './blockParagraphs';
	import { parseFromParam } from './workbench';
	import {
		findLatestCaptureInProject,
		type ProjectPromptLookupResult
	} from './projectPromptLookup';
	import {
		formatProjectParam,
		parseProjectParam,
		resolveInitialProject,
		saveRememberedProject
	} from './workbenchProject';
	import {
		clampColumnForContainer,
		defaultAssistantOpen,
		defaultNavOpen,
		loadWorkbenchLayout,
		saveWorkbenchLayout,
		WORKBENCH_LAYOUT_BOUNDS,
		type WorkbenchLayout,
		type WorkbenchOpenState
	} from './workbenchLayout';

	interface Props {
		requestId: number | null;
		blockParam: number | null;
		prefillParam: string | null;
		ruleParam: string | null;
	}

	let { requestId, blockParam, prefillParam, ruleParam }: Props = $props();

	const store = getContext<FilterRulesStore | undefined>(FILTER_RULES_STORE_KEY);

	const catalogue = createPromptCatalogue();
	onMount(() => {
		void catalogue.load();
	});

	let detail = $state<RequestDetail | null>(null);
	let sourceLoading = $state(false);
	let sourceError = $state<string | null>(null);

	$effect(() => {
		const id = requestId;
		untrack(() => {
			void loadSource(id);
		});
	});

	async function loadSource(id: number | null): Promise<void> {
		if (id === null) {
			detail = null;
			sourceError = null;
			return;
		}
		sourceLoading = true;
		sourceError = null;
		try {
			const loaded = await getRequestDetail(id);
			if (requestId !== id) return;
			detail = loaded;
		} catch (err) {
			if (requestId !== id) return;
			detail = null;
			sourceError = err instanceof ApiError ? err.message : String(err);
		} finally {
			if (requestId === id) sourceLoading = false;
		}
	}

	const hasSource = $derived(requestId !== null);

	const sourceClassIdentity = $derived(
		detail !== null
			? classForTurnOrigin(detail.turn_origin ?? null, detail.subagent_persona ?? null)
			: null
	);

	const promptTitle = $derived(
		sourceClassIdentity !== null
			? classTitle(sourceClassIdentity.lane, sourceClassIdentity.persona)
			: null
	);

	// ---- Project picker: "Global" (current behaviour) or one captured project, whose latest
	// capture of the SAME model·class replaces what the rest of the workbench treats as "the
	// open request" (spec: "A project = the latest capture of the same model·class from that
	// project"). `detail`/`requestId` above stay the fixed ANCHOR (the URL's own request) for
	// its whole lifetime — they never get swapped — so the lookup below always has a stable,
	// non-circular (lane, persona, model) basis to search a project with, however many times the
	// picked project itself changes.

	let projects = $state<ProjectSummaryDto[]>([]);
	let projectsStatus = $state<'loading' | 'ready' | 'error'>('loading');
	let projectsError = $state<string | null>(null);

	onMount(() => {
		void loadProjects();
	});

	async function loadProjects(): Promise<void> {
		projectsStatus = 'loading';
		projectsError = null;
		try {
			projects = await listProjects({ counts: true });
			projectsStatus = 'ready';
		} catch (err) {
			projects = [];
			projectsStatus = 'error';
			projectsError = err instanceof ApiError ? err.message : String(err);
		}
	}

	let pickedProject = $state<string | null>(
		resolveInitialProject(page.url.searchParams.get('project'))
	);

	function writeProjectParam(next: string | null): void {
		const url = new URL(page.url);
		const formatted = formatProjectParam(next);
		if (url.searchParams.get('project') === formatted) return;
		url.searchParams.set('project', formatted);
		const qs = url.searchParams.toString();
		void goto(`${url.pathname}${qs ? `?${qs}` : ''}`, {
			replaceState: true,
			keepFocus: true,
			noScroll: true
		});
	}

	onMount(() => {
		writeProjectParam(pickedProject);
	});

	function setPickedProject(next: string | null): void {
		pickedProject = next;
		saveRememberedProject(next);
		writeProjectParam(next);
	}

	let projectLookupStatus = $state<'idle' | 'loading' | 'ready' | 'error'>('idle');
	let projectLookupResult = $state<ProjectPromptLookupResult | null>(null);
	let projectDetail = $state<RequestDetail | null>(null);
	let projectDetailLoading = $state(false);
	let projectDetailError = $state<string | null>(null);

	$effect(() => {
		const project = pickedProject;
		const identity = sourceClassIdentity;
		const model = detail?.model ?? null;
		untrack(() => void runProjectLookup(project, identity, model));
	});

	async function runProjectLookup(
		project: string | null,
		identity: RequestClassIdentity | null,
		model: string | null
	): Promise<void> {
		if (project === null || identity === null) {
			projectLookupStatus = 'idle';
			projectLookupResult = null;
			projectDetail = null;
			projectDetailError = null;
			return;
		}
		projectLookupStatus = 'loading';
		projectLookupResult = null;
		const result = await findLatestCaptureInProject(identity, model, project);
		if (project !== pickedProject) return;
		projectLookupResult = result;
		projectLookupStatus = result.kind === 'error' ? 'error' : 'ready';
		if (result.kind === 'found') {
			void loadProjectDetail(result.requestId, project);
		} else {
			projectDetail = null;
			projectDetailError = null;
		}
	}

	async function loadProjectDetail(id: number, project: string): Promise<void> {
		if (id === requestId && detail !== null) {
			projectDetail = detail;
			return;
		}
		projectDetailLoading = true;
		projectDetailError = null;
		try {
			const loaded = await getRequestDetail(id);
			if (project !== pickedProject) return;
			projectDetail = loaded;
		} catch (err) {
			if (project !== pickedProject) return;
			projectDetail = null;
			projectDetailError = err instanceof ApiError ? err.message : String(err);
		} finally {
			if (project === pickedProject) projectDetailLoading = false;
		}
	}

	const effectiveDetail = $derived(
		pickedProject === null ? detail : projectLookupResult?.kind === 'found' ? projectDetail : null
	);
	const effectiveRequestId = $derived(
		pickedProject === null
			? requestId
			: projectLookupResult?.kind === 'found'
				? projectLookupResult.requestId
				: null
	);
	const effectiveLoading = $derived(
		pickedProject === null
			? sourceLoading
			: projectLookupStatus === 'loading' ||
					(projectLookupResult?.kind === 'found' && projectDetailLoading)
	);
	const effectiveError = $derived.by(() => {
		if (pickedProject === null) return sourceError;
		if (projectLookupResult?.kind === 'error') return projectLookupResult.message;
		if (projectDetailError !== null) return projectDetailError;
		return null;
	});
	const noCaptureInProject = $derived(
		pickedProject !== null && projectLookupResult?.kind === 'no_capture'
			? projectLookupResult.availableClasses
			: null
	);
	const pickedProjectName = $derived(
		pickedProject === null
			? null
			: (projects.find((p) => p.project_path === pickedProject)?.custom_name ?? pickedProject)
	);

	/**
	 * A "no capture" fallback link (spec: "the project's available model·classes as links") —
	 * `listPromptClasses` says the class HAS a capture in this project, but not which request,
	 * so this resolves it lazily on click (the same lookup driving the main redirect) rather
	 * than eagerly N+1-fetching every fallback row just to render the list.
	 */
	async function openAvailableClassInProject(cls: { lane: 'main' | 'subagent'; subagent_persona: string | null }): Promise<void> {
		if (pickedProject === null) return;
		const result = await findLatestCaptureInProject(
			{ lane: cls.lane, persona: cls.subagent_persona },
			null,
			pickedProject
		);
		if (result.kind !== 'found') return;
		void goto(resolve(`/filter/workbench?from=request:${result.requestId}&project=${encodeURIComponent(pickedProject)}`));
	}

	const blocks = $derived<SystemBlock[]>(effectiveDetail?.system_blocks?.blocks ?? []);
	const systemPromptSource = $derived(effectiveDetail?.system_prompt_source ?? null);
	const placeholder = $derived(systemPromptPlaceholder(blocks.length, systemPromptSource));
	const showSessionMainNote = $derived(showsSessionMainNote(blocks.length, systemPromptSource));
	const blockTexts = $derived(blocks.map((b) => b.text));
	const totalChars = $derived(blockTexts.reduce((acc, t) => acc + t.length, 0));

	/** The toolbar's top "back" link — always the request the operator actually OPENED via the
	 *  URL, never the project's redirected one, so it's a stable way back to origin regardless
	 *  of which project is picked. */
	const backHref = $derived(
		detail !== null && requestId !== null
			? (`/sessions/${detail.session_id}/requests/${requestId}` as const)
			: '/'
	);

	/** WorkbenchNavigator's own "open request" link, which should open whatever request is
	 *  CURRENTLY displayed (the project's redirected one, when one is picked and found). */
	const effectiveBackHref = $derived(
		effectiveDetail !== null && effectiveRequestId !== null
			? (`/sessions/${effectiveDetail.session_id}/requests/${effectiveRequestId}` as const)
			: backHref
	);

	const requestRelease = $derived(parseReleaseFromBlockText(blocks[0]?.text ?? ''));

	const orderedRules = $derived.by(() => {
		const rules = store?.rules ?? [];
		return [...rules].sort((a, b) => a.sort_order - b.sort_order);
	});
	const enabledRules = $derived(orderedRules.filter((r) => r.enabled));

	// ---- Layout: four columns (outline / prompt / your edits / prompt assistant), each of the
	// three side columns independently resizable and collapsible to a 36px rail. Widths persist;
	// open/collapsed state is a responsive DEFAULT applied once, the first time the workbench's
	// own rendered width is measured, and never re-applied on a later resize — so it can't
	// clobber a manual toggle mid-session. The workbench fills essentially the whole viewport in
	// the real route (`wb-page-shell { height: calc(100vh - 44px) }`, full width), so its own
	// measured width and the browser viewport width are the same number in practice — using the
	// component's own width instead of `window.innerWidth` directly is what makes the responsive
	// defaults reproducible in Storybook (a fixed-width wrapper div), not just in a real browser.

	let layout = $state<WorkbenchLayout>(loadWorkbenchLayout());
	$effect(() => {
		saveWorkbenchLayout(layout);
	});

	let navOpen = $state(true);
	let editsOpen = $state(true);
	let assistantOpen = $state(true);

	/** Measured live via `bind:clientWidth` on `.wb-workspace` — 0 before the first layout tick,
	 *  at which point `clampColumnForContainer` just falls back to each column's own bounds. */
	let workspaceWidth = $state(0);

	let responsiveDefaultsApplied = false;
	$effect(() => {
		const width = workspaceWidth;
		if (responsiveDefaultsApplied || width <= 0) return;
		untrack(() => {
			navOpen = defaultNavOpen(width);
			assistantOpen = defaultAssistantOpen(width);
			responsiveDefaultsApplied = true;
		});
	});

	/** True once both "Your edits" and "Prompt assistant" are collapsed — the toolbar's
	 *  "Read only" / "Show edits" toggle flips both together (spec: "Read only collapses edits +
	 *  assistant together"), same underlying per-column collapse each also drives on its own. */
	const readOnly = $derived(!editsOpen && !assistantOpen);

	function toggleReadOnly(): void {
		const nextOpen = readOnly;
		editsOpen = nextOpen;
		assistantOpen = nextOpen;
	}

	function openState(): WorkbenchOpenState {
		return { nav: navOpen, edits: editsOpen, assistant: assistantOpen };
	}

	function resizeNav(deltaPx: number): void {
		layout = {
			...layout,
			navWidth: clampColumnForContainer(
				'navWidth',
				layout.navWidth + deltaPx,
				layout,
				openState(),
				workspaceWidth
			)
		};
	}
	function resizeEdits(deltaPx: number): void {
		layout = {
			...layout,
			editsWidth: clampColumnForContainer(
				'editsWidth',
				layout.editsWidth - deltaPx,
				layout,
				openState(),
				workspaceWidth
			)
		};
	}
	function resizeAssistant(deltaPx: number): void {
		layout = {
			...layout,
			assistantWidth: clampColumnForContainer(
				'assistantWidth',
				layout.assistantWidth - deltaPx,
				layout,
				openState(),
				workspaceWidth
			)
		};
	}

	/** Bumped to move focus into the assistant composer once its column is open (spec: "Ask
	 *  about this"/"Reword with assistant" "expand the assistant if collapsed and focus the
	 *  composer") — `WorkbenchAssistantPanel` reacts to it via its own `$effect`. */
	let assistantFocusSignal = $state(0);
	/** True while the assistant is streaming a reply — bound from `WorkbenchAssistantPanel` so
	 *  its collapsed rail can show the same signal (spec: "its rail shows an indicator while a
	 *  reply streams"). */
	let assistantStreaming = $state(false);

	// ---- Prompt lens + margin/assistant cross-linking ----

	let lens = $state<PromptLens>('changes');

	let assistantQuote = $state('');
	let assistantDraftHint = $state<AssistantDraftHint | null>(null);

	function handleAskAbout(quote: string): void {
		assistantQuote = quote;
		assistantOpen = true;
		assistantFocusSignal += 1;
	}

	function handleReword(rule: FilterRuleDto, quote: string): void {
		assistantQuote = quote;
		assistantDraftHint = {
			text: 'Reword this so it reads better and stays accurate.',
			editId: rule.id
		};
		assistantOpen = true;
		assistantFocusSignal += 1;
	}

	function handleOpenEdit(ruleId: string): void {
		if (!store) return;
		store.scrollTargetRuleId = ruleId;
		editsOpen = true;
	}

	/**
	 * The first paragraph (in source order, across every block) a rule attributes to. Uses the
	 * offset-based `paragraphIndicesTouchedByRule` rather than a per-paragraph substring scan so
	 * a match spanning a paragraph boundary (a heading immediately followed by a blank line then
	 * its body) still resolves to a real paragraph instead of silently matching nothing.
	 */
	function findParagraphKeyForRule(rule: FilterRuleDto): string | null {
		for (const block of blocks) {
			const touched = paragraphIndicesTouchedByRule(block.text, rule);
			const first = touched[0];
			if (first !== undefined) return `${block.index}:${first}`;
		}
		return null;
	}

	function handleShowInPrompt(rule: FilterRuleDto): void {
		if (lens === 'result') lens = 'changes';
		const key = findParagraphKeyForRule(rule);
		if (key !== null) paragraphScrollTarget = key;
	}

	function handleJumpToParagraph(key: string): void {
		if (lens === 'result') lens = 'changes';
		paragraphScrollTarget = key;
	}

	let promptScrollTarget = $state<number | null>(null);
	let paragraphScrollTarget = $state<string | null>(null);
	$effect(() => {
		if (blockParam !== null) untrack(() => (promptScrollTarget = blockParam));
	});

	const promptOutline = $derived(
		buildPromptOutline(blocks, (paragraphText) =>
			enabledRules.some((r) => paragraphContainsRule(paragraphText, r))
		)
	);

	// ---- Create-rule (prefill from URL, or the blank "Add rule" button) ----

	let createRequest = $state<{ value: string; action: FilterAction } | null>(null);

	onMount(() => {
		if (prefillParam !== null && prefillParam.length > 0) {
			createRequest = { value: prefillParam, action: 'drop' };
			editsOpen = true;
		}
	});

	let ruleTargetApplied = false;
	$effect(() => {
		if (ruleTargetApplied || !store || ruleParam === null) return;
		if (!store.initialized) return;
		store.scrollTargetRuleId = ruleParam;
		editsOpen = true;
		ruleTargetApplied = true;
	});

	// ---- The current-edits dry run (drives the "changes"/"result" lenses) ----

	type PreviewStatus = 'idle' | 'loading' | 'ready' | 'unavailable' | 'error';

	let dryRunStatus = $state<PreviewStatus>('idle');
	let dryRunResult = $state<FilterDryRunResponse | null>(null);
	let dryRunTimer: ReturnType<typeof setTimeout> | null = null;
	let dryRunToken = 0;

	const dryRunFingerprint = $derived(
		JSON.stringify({
			effectiveRequestId,
			rules: enabledRules.map((r) => [r.id, r.kind, r.value, r.action, r.text])
		})
	);

	$effect(() => {
		void dryRunFingerprint;
		untrack(() => scheduleDryRun());
	});

	function scheduleDryRun(): void {
		if (dryRunTimer !== null) clearTimeout(dryRunTimer);
		if (effectiveRequestId === null || enabledRules.length === 0) {
			dryRunStatus = 'idle';
			dryRunResult = null;
			return;
		}
		dryRunStatus = 'loading';
		dryRunTimer = setTimeout(() => {
			dryRunTimer = null;
			void runDryRun();
		}, 300);
	}

	async function runDryRun(): Promise<void> {
		if (effectiveRequestId === null || enabledRules.length === 0) return;
		const token = ++dryRunToken;
		dryRunStatus = 'loading';
		try {
			const res = await dryRunFilterRules({ request_id: effectiveRequestId, rules: enabledRules });
			if (token !== dryRunToken) return;
			dryRunResult = res;
			dryRunStatus = 'ready';
		} catch (err) {
			if (token !== dryRunToken) return;
			dryRunResult = null;
			dryRunStatus = err instanceof ApiError && (err.status === 404 || err.status === 405)
				? 'unavailable'
				: 'error';
		}
	}

	onMount(() => {
		return () => {
			if (dryRunTimer !== null) clearTimeout(dryRunTimer);
		};
	});

	const resultBlocks = $derived(
		dryRunStatus === 'ready' && dryRunResult !== null
			? (dryRunResult.filtered_system?.map((b) => b.text) ?? [])
			: null
	);

	// `dryRunResult.filtered_len` is the dry-run engine's WHOLE-REQUEST-BODY length (it also
	// covers the message history), not the system prompt alone — showing it next to `totalChars`
	// (a system-prompt-only sum) produced a nonsensical "11,395 → 184,253 chars" label live on
	// request 421514 (round-4 defect #3). The comparable "after" number is the same sum, taken
	// over `filtered_system`'s block texts instead of the source blocks'.
	const afterChars = $derived(
		resultBlocks !== null ? resultBlocks.reduce((acc, t) => acc + t.length, 0) : totalChars
	);

	/**
	 * The dry-run's own authoritative answer to "does this rule change this request" — used in
	 * place of a client-side substring/heuristic match wherever one was standing in for it
	 * (round-4 defect #2/#4). `null` before any dry run has completed, so call sites can fall
	 * back to their own provisional heuristic rather than claiming "not in this prompt" for
	 * everything while the debounced dry run is still in flight.
	 */
	const firingRuleIds = $derived(
		dryRunStatus === 'ready' && dryRunResult !== null
			? new Set(dryRunResult.firing_rule_ids.map((id) => String(id)))
			: null
	);

	// ---- Session message (spec 4.2/G9): a rule can land in the last user message, not just
	// the system blocks — it gets the same diff and margin-title treatment as a system block.

	const sessionMessage = $derived(effectiveDetail?.last_user_message ?? '');
	const resultSessionMessage = $derived(
		dryRunStatus === 'ready' && dryRunResult !== null
			? resolveDryRunSessionMessage(dryRunResult.filtered_messages, sessionMessage)
			: null
	);

	// ---- Assistant proposal preview (a SEPARATE dry run with the proposal applied) ----

	let previewingProposal = $state<AssistantProposal | null>(null);
	let previewResult = $state<FilterDryRunResponse | null>(null);
	let previewToken = 0;

	async function startPreview(proposal: AssistantProposal): Promise<void> {
		previewingProposal = proposal;
		lens = 'changes';
		if (effectiveRequestId === null) return;
		const token = ++previewToken;
		try {
			const res = await dryRunFilterRules({
				request_id: effectiveRequestId,
				rules: buildPreviewRules(enabledRules, proposal)
			});
			if (token !== previewToken) return;
			previewResult = res;
		} catch {
			if (token !== previewToken) return;
			previewResult = null;
		}
	}

	function stopPreview(): void {
		previewToken++;
		previewingProposal = null;
		previewResult = null;
	}

	function handleApplied(ruleId: string): void {
		stopPreview();
		void store?.refresh();
		if (store) store.scrollTargetRuleId = ruleId;
		editsOpen = true;
	}

	const previewSessionMessage = $derived(
		previewingProposal !== null
			? resolveDryRunSessionMessage(previewResult?.filtered_messages ?? null, sessionMessage)
			: null
	);

	const previewBlocks = $derived(
		previewingProposal !== null
			? (previewResult?.filtered_system?.map((b) => b.text) ?? blocks.map((b) => b.text))
			: null
	);
</script>

<div class="wb-shell" data-slot="filter-workbench">
	<header class="wb-toolbar">
		<button
			type="button"
			class="wb-nav-toggle"
			aria-label={navOpen ? 'Hide outline' : 'Show outline'}
			aria-pressed={navOpen}
			onclick={() => (navOpen = !navOpen)}
		>
			{#if navOpen}
				<PanelLeftClose size={14} aria-hidden="true" />
			{:else}
				<PanelLeftOpen size={14} aria-hidden="true" />
			{/if}
		</button>
		<a href={resolve(backHref)} class="wb-back-link" aria-label="Back to request">
			{#if requestId !== null}Request #{requestId}{:else}Home{/if}
		</a>
		<h1 class="wb-title">Filter workbench</h1>

		{#if hasSource}
			<div class="wb-toolbar-chips">
				<span class="wb-chip">{effectiveDetail?.model ?? 'unknown model'}</span>
				{#if promptTitle !== null}
					<span class="wb-chip">{promptTitle}</span>
				{/if}
				{#if requestRelease !== null}
					<span class="wb-chip">Claude Code {requestRelease}</span>
				{/if}
			</div>

			<WorkbenchProjectPicker
				{projects}
				loading={projectsStatus === 'loading'}
				error={projectsError}
				value={pickedProject}
				onSelect={setPickedProject}
			/>

			<div class="wb-lens-tabs" role="tablist" aria-label="Prompt view">
				{#each [['source', 'As sent'], ['changes', 'With my edits'], ['result', 'Final']] as [key, label] (key)}
					<button
						type="button"
						role="tab"
						aria-selected={lens === key}
						class="wb-lens-tab"
						class:wb-lens-tab--active={lens === key}
						onclick={() => (lens = key as PromptLens)}
					>
						{label}
					</button>
				{/each}
			</div>

			<span class="wb-size-chip" title="What Anthropic receives after your edits">
				{totalChars.toLocaleString()} → {afterChars.toLocaleString()} chars
			</span>
			<button
				type="button"
				class="wb-readonly-toggle"
				aria-pressed={readOnly}
				onclick={toggleReadOnly}
			>
				{readOnly ? 'Show edits' : 'Read only'}
			</button>
		{/if}
	</header>

	<div class="wb-workspace" bind:clientWidth={workspaceWidth}>
		{#if hasSource}
			{#if navOpen}
				<aside
					class="wb-nav-slot"
					aria-label="Filter workbench outline"
					style="width: {layout.navWidth}px"
				>
					{#if effectiveLoading && effectiveDetail === null}
						<section class="wb-pane wb-pane--placeholder" data-augmented-ui="tl-clip bl-clip border" aria-label="Prompt navigator">
							<div class="wb-placeholder" aria-live="polite">
								<p class="wb-placeholder__title">Loading…</p>
							</div>
						</section>
					{:else if effectiveError !== null}
						<section class="wb-pane wb-pane--placeholder" data-augmented-ui="tl-clip bl-clip border" aria-label="Prompt navigator">
							<div class="wb-placeholder" role="alert">
								<p class="wb-placeholder__title">Could not load the prompt</p>
								<p class="wb-placeholder__hint">{effectiveError}</p>
							</div>
						</section>
					{:else if effectiveDetail !== null}
						<WorkbenchNavigator
							requestId={effectiveRequestId ?? 0}
							sessionId={effectiveDetail.session_id}
							model={effectiveDetail.model}
							backHref={effectiveBackHref}
							blockCount={blocks.length}
							{totalChars}
							classIdentity={sourceClassIdentity}
							catalogueClasses={catalogue.classes}
							catalogueStatus={catalogue.status}
							{requestRelease}
							onRetryVersions={(key) => catalogue.retryVersions(key)}
							outline={promptOutline}
							onJumpToParagraph={handleJumpToParagraph}
							onToggleCollapse={() => (navOpen = false)}
						/>
					{:else if noCaptureInProject !== null}
						<section class="wb-pane wb-pane--placeholder" data-augmented-ui="tl-clip bl-clip border" aria-label="Prompt navigator">
							<div class="wb-placeholder">
								<p class="wb-placeholder__title">No capture in {pickedProjectName}</p>
							</div>
						</section>
					{/if}
				</aside>
				<WorkbenchDivider
					orientation="vertical"
					label="Resize outline"
					value={layout.navWidth}
					min={WORKBENCH_LAYOUT_BOUNDS.navWidth.min}
					max={WORKBENCH_LAYOUT_BOUNDS.navWidth.max}
					onResize={resizeNav}
				/>
			{:else}
				<WorkbenchRail label="Outline" onExpand={() => (navOpen = true)} />
			{/if}
		{/if}

		<main class="wb-prompt-slot" class:wb-prompt-slot--centered={readOnly}>
			{#if hasSource}
				{#if noCaptureInProject !== null}
					<section class="wb-pane wb-pane--placeholder" aria-label="System prompt">
						<div class="wb-placeholder" data-slot="no-capture-in-project">
							<p class="wb-placeholder__title">
								No capture of {promptTitle ?? 'this prompt class'} in {pickedProjectName}
							</p>
							{#if noCaptureInProject.length > 0}
								<p class="wb-placeholder__hint">Available in {pickedProjectName}:</p>
								<div class="wb-available-classes">
									{#each noCaptureInProject as cls (`${cls.lane}:${cls.subagent_persona ?? ''}`)}
										<button
											type="button"
											class="wb-available-class-link"
											onclick={() => void openAvailableClassInProject(cls)}
										>
											{classTitle(cls.lane, cls.subagent_persona)}
										</button>
									{/each}
								</div>
							{/if}
						</div>
					</section>
				{:else if effectiveLoading && effectiveDetail === null}
					<section class="wb-pane wb-pane--placeholder" aria-label="System prompt">
						<div class="wb-placeholder" aria-live="polite">
							<p class="wb-placeholder__title">Loading…</p>
						</div>
					</section>
				{:else if effectiveError !== null}
					<section class="wb-pane wb-pane--placeholder" aria-label="System prompt">
						<div class="wb-placeholder" role="alert">
							<p class="wb-placeholder__title">Could not load the prompt</p>
							<p class="wb-placeholder__hint">{effectiveError}</p>
						</div>
					</section>
				{:else if placeholder.kind === 'not_captured'}
					<section class="wb-pane wb-pane--placeholder" aria-label="System prompt">
						<div class="wb-placeholder" data-slot="system-prompt-not-captured-notice">
							<p class="wb-placeholder__title">System prompt not captured for this request</p>
							<p class="wb-placeholder__hint">
								Request #{effectiveRequestId} has no system-prompt capture of its own, so nothing
								is shown here — never another request's prompt. Pick a request with a system
								prompt (most primary-turn requests have one).
							</p>
						</div>
					</section>
				{:else if placeholder.kind === 'no_blocks'}
					<section class="wb-pane wb-pane--placeholder" aria-label="System prompt">
						<div class="wb-placeholder">
							<p class="wb-placeholder__title">No system prompt on this request</p>
							<p class="wb-placeholder__hint">
								Request #{effectiveRequestId} carries no <code>system_blocks</code>
								projection — there is nothing to filter. Pick a request with a system prompt (most
								primary-turn requests have one).
							</p>
						</div>
					</section>
				{:else}
					{#if showSessionMainNote}
						<p class="wb-source-note" data-slot="system-prompt-session-main-note">
							<span class="badge-nx badge-analyzed">shared</span>
							shared with the session's main prompt
						</p>
					{/if}
					<WorkbenchSourcePane
						{blocks}
						{orderedRules}
						{resultBlocks}
						{previewBlocks}
						{firingRuleIds}
						{sessionMessage}
						{resultSessionMessage}
						{previewSessionMessage}
						{lens}
						scrollToBlock={promptScrollTarget}
						scrollToParagraph={paragraphScrollTarget}
						onAskAbout={handleAskAbout}
						onOpenEdit={handleOpenEdit}
					/>
				{/if}
			{:else}
				<WorkbenchLanding
					{orderedRules}
					classes={catalogue.classes}
					status={catalogue.status}
					error={catalogue.error}
					onRetryCatalogue={() => void catalogue.load()}
					onRetryModel={(key, modelIndex) => catalogue.retryModel(key, modelIndex)}
					onRetryVersions={(key) => catalogue.retryVersions(key)}
				/>
			{/if}
		</main>

		{#if hasSource}
			{#if editsOpen}
				<WorkbenchDivider
					orientation="vertical"
					label="Resize your edits"
					value={layout.editsWidth}
					min={WORKBENCH_LAYOUT_BOUNDS.editsWidth.min}
					max={WORKBENCH_LAYOUT_BOUNDS.editsWidth.max}
					onResize={resizeEdits}
				/>
				<aside class="wb-edits-slot" aria-label="Filter workbench edits" style="width: {layout.editsWidth}px">
					<WorkbenchRuleStack
						{orderedRules}
						{blockTexts}
						{firingRuleIds}
						hasSource={hasSource && blocks.length > 0}
						loading={store?.loading ?? false}
						error={store?.error ?? null}
						catalogueClasses={catalogue.classes}
						catalogueDetailsSettled={catalogue.detailsSettled}
						{createRequest}
						onCreateConsumed={() => (createRequest = null)}
						onAsk={handleReword}
						onShowInPrompt={handleShowInPrompt}
						onToggleCollapse={() => (editsOpen = false)}
						{pickedProject}
						{pickedProjectName}
						{projects}
					/>
				</aside>
			{:else}
				<WorkbenchRail label="Your edits" onExpand={() => (editsOpen = true)} />
			{/if}

			{#if assistantOpen}
				<WorkbenchDivider
					orientation="vertical"
					label="Resize prompt assistant"
					value={layout.assistantWidth}
					min={WORKBENCH_LAYOUT_BOUNDS.assistantWidth.min}
					max={WORKBENCH_LAYOUT_BOUNDS.assistantWidth.max}
					onResize={resizeAssistant}
				/>
				<aside
					class="wb-assistant-slot"
					aria-label="Filter workbench prompt assistant"
					style="width: {layout.assistantWidth}px"
				>
					<WorkbenchAssistantPanel
						requestId={effectiveRequestId}
						quote={assistantQuote}
						onClearQuote={() => (assistantQuote = '')}
						draftHint={assistantDraftHint}
						onDraftHintConsumed={() => (assistantDraftHint = null)}
						previewingProposalId={previewingProposal?.id ?? null}
						onPreview={(p) => void startPreview(p)}
						onStopPreview={stopPreview}
						onApplied={handleApplied}
						onToggleCollapse={() => (assistantOpen = false)}
						bind:streaming={assistantStreaming}
						focusSignal={assistantFocusSignal}
						workbenchScope={pickedProject === null ? 'all' : 'project'}
						workbenchProjectPath={pickedProject}
					/>
				</aside>
			{:else}
				<WorkbenchRail
					label="Prompt assistant"
					streaming={assistantStreaming}
					onExpand={() => (assistantOpen = true)}
				/>
			{/if}
		{/if}
	</div>
</div>

<style>
	.wb-shell {
		display: flex;
		flex-direction: column;
		width: 100%;
		height: 100%;
		min-height: 0;
		background: var(--surface-bg);
		color: var(--text-1);
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 12px;
	}

	.wb-toolbar {
		flex-shrink: 0;
		height: 52px;
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 0 10px 0 6px;
		border-bottom: 1px solid var(--border-default);
		background: var(--surface-sidebar);
		overflow-x: auto;
	}

	.wb-nav-toggle {
		flex-shrink: 0;
		width: 30px;
		height: 30px;
		display: flex;
		align-items: center;
		justify-content: center;
		background: transparent;
		border: 1px solid var(--border-default);
		color: var(--text-2);
		cursor: pointer;
	}
	.wb-nav-toggle:hover {
		color: var(--text-strong);
	}
	.wb-nav-toggle:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}

	.wb-back-link {
		flex-shrink: 0;
		font-size: 11px;
		color: var(--text-3);
		text-decoration: none;
		white-space: nowrap;
	}
	.wb-back-link:hover {
		color: var(--text-strong);
	}

	.wb-title {
		flex-shrink: 0;
		margin: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 800;
		font-size: 13px;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-strong);
		white-space: nowrap;
	}

	.wb-toolbar-chips {
		flex-shrink: 0;
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.wb-chip {
		display: inline-flex;
		align-items: center;
		padding: 3px 7px;
		font-size: 11px;
		border: 1px solid var(--border-default);
		color: var(--text-2);
		white-space: nowrap;
	}

	.wb-lens-tabs {
		flex-shrink: 0;
		display: flex;
		height: 52px;
		margin-left: 8px;
	}

	.wb-lens-tab {
		height: 52px;
		padding: 0 12px;
		background: transparent;
		border: none;
		border-bottom: 2px solid transparent;
		color: var(--text-2);
		font-size: 11px;
		cursor: pointer;
		white-space: nowrap;
	}
	.wb-lens-tab--active {
		color: var(--funk-cyan);
		border-bottom-color: var(--funk-cyan);
	}
	.wb-lens-tab:focus-visible {
		outline: none;
		box-shadow: inset 0 0 0 2px var(--interactive-focus-ring);
	}

	.wb-size-chip {
		margin-left: auto;
		flex-shrink: 0;
		height: 28px;
		display: flex;
		align-items: center;
		padding: 0 8px;
		border: 1px solid var(--border-default);
		color: var(--text-2);
		font-size: 11px;
		white-space: nowrap;
	}

	.wb-readonly-toggle {
		flex-shrink: 0;
		height: 28px;
		padding: 0 12px;
		font-size: 10px;
		cursor: pointer;
		background: transparent;
		color: var(--text-2);
		border: 1px solid var(--border-default);
		white-space: nowrap;
	}
	.wb-readonly-toggle[aria-pressed='true'] {
		color: var(--funk-cyan);
		background: var(--funk-chip-cyan);
		border-color: var(--funk-cyan-border);
	}
	.wb-readonly-toggle:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}

	.wb-workspace {
		flex: 1;
		display: flex;
		min-height: 0;
	}

	.wb-nav-slot {
		flex-shrink: 0;
		overflow-y: auto;
		background: var(--surface-sidebar);
	}

	.wb-prompt-slot {
		flex: 1;
		/* Keep in sync with workbenchLayout.ts's PROMPT_MIN_WIDTH, which the divider clamp math
		   uses to stop a resize before squeezing this column past its floor. */
		min-width: 520px;
		display: flex;
		flex-direction: column;
	}
	.wb-prompt-slot--centered {
		max-width: 960px;
		margin: 0 auto;
	}

	.wb-edits-slot,
	.wb-assistant-slot {
		flex-shrink: 0;
		min-height: 0;
		overflow: hidden;
	}

	.wb-pane {
		display: flex;
		flex-direction: column;
		min-height: 0;
	}
	.wb-pane--placeholder {
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		overflow-y: auto;
	}

	.wb-placeholder {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 48px 24px;
		text-align: center;
		align-items: center;
	}

	.wb-placeholder__title {
		font: 500 12px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-strong);
		margin: 0;
	}

	.wb-placeholder__hint {
		font: 11px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-2);
		margin: 0;
		max-width: 36rem;
		line-height: 1.6;
	}
	.wb-placeholder__hint code {
		background: var(--surface-2);
		border-radius: 3px;
		padding: 0 4px;
		color: var(--text-1);
	}

	.wb-available-classes {
		display: flex;
		flex-wrap: wrap;
		justify-content: center;
		gap: 8px;
	}
	.wb-available-class-link {
		height: 26px;
		padding: 0 10px;
		font-size: 11px;
		cursor: pointer;
		background: transparent;
		color: var(--funk-cyan);
		border: 1px solid var(--funk-cyan-border);
	}
	.wb-available-class-link:hover {
		background: var(--funk-chip-cyan);
	}
	.wb-available-class-link:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}

	.wb-source-note {
		flex-shrink: 0;
		display: flex;
		align-items: center;
		gap: 6px;
		margin: 0;
		padding: 8px 18px;
		font-size: 11px;
		color: var(--text-2);
		border-bottom: 1px solid var(--border-subtle);
	}
</style>
