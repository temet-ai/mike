<script lang="ts">
	import { getContext, onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { Settings as SettingsIcon } from '@lucide/svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import MarkdownText from '$lib/components/shared/MarkdownText.svelte';
	import { FILTER_RULES_STORE_KEY, type FilterRulesStore } from '$lib/stores/filterRules.svelte';
	import { ApiError } from '$lib/api';
	import type { FilterRuleDto } from '$lib/types';
	import {
		createAssistantConversation,
		getAssistantConversation,
		getAssistantHealth,
		sendAssistantMessage,
		type AssistantHealth,
		type AssistantMessageDto,
		type AssistantProposal,
		type AssistantScope
	} from '$lib/api/assistant';
	import { planApplyProposal } from './proposalRules';

	/** A "Reword with assistant" click's context: the draft text to prefill, and which saved
	 *  edit (if any) it's about — carried into `send()` as `edit_id` (spec 4.4/G5). */
	export interface AssistantDraftHint {
		text: string;
		editId: string | null;
	}

	interface Props {
		requestId: number | null;
		quote: string;
		onClearQuote: () => void;
		draftHint: AssistantDraftHint | null;
		onDraftHintConsumed: () => void;
		previewingProposalId: string | null;
		onPreview: (proposal: AssistantProposal) => void;
		onStopPreview: () => void;
		onApplied: (ruleId: string) => void;
		onToggleCollapse: () => void;
		/** True while a reply is streaming — bindable so the parent can show the same signal on
		 *  this column's collapsed rail (spec: "its rail shows an indicator while a reply streams"). */
		streaming?: boolean;
		/** Bumped by the parent (e.g. "Ask about this" / "Reword with assistant") to move focus
		 *  into the composer once the column is open — see the `$effect` below. 0 = no request yet. */
		focusSignal?: number;
		/**
		 * What to check against — now driven entirely by the workbench's own Project picker
		 * ('all' for Global, 'project' for a picked project), not a control inside this panel
		 * (spec: "remove its own 'Check against' control"). `workbenchProjectPath` rides along
		 * with `scope: 'project'` so the service reads THAT project's CLAUDE.md/rules rather
		 * than defaulting to the open request's own project.
		 */
		workbenchScope: AssistantScope;
		workbenchProjectPath: string | null;
	}

	let {
		requestId,
		quote,
		onClearQuote,
		draftHint,
		onDraftHintConsumed,
		previewingProposalId,
		onPreview,
		onStopPreview,
		onApplied,
		onToggleCollapse,
		streaming = $bindable(false),
		focusSignal = 0,
		workbenchScope,
		workbenchProjectPath
	}: Props = $props();

	let composerTextarea = $state<HTMLTextAreaElement | undefined>();

	$effect(() => {
		if (focusSignal > 0) composerTextarea?.focus();
	});

	const store = getContext<FilterRulesStore | undefined>(FILTER_RULES_STORE_KEY);

	const HEALTH_POLL_MS = 30_000;
	const CONVERSATION_STORAGE_PREFIX = 'proxy-inspector:assistant-conversation:';

	function conversationStorageKey(id: number): string {
		return `${CONVERSATION_STORAGE_PREFIX}${id}`;
	}
	function loadStoredConversationId(id: number): string | null {
		try {
			return localStorage.getItem(conversationStorageKey(id));
		} catch {
			return null;
		}
	}
	function storeConversationId(id: number, idToStore: string): void {
		try {
			localStorage.setItem(conversationStorageKey(id), idToStore);
		} catch {
			// best-effort — a private window or a full storage quota just means no rehydration
		}
	}
	function clearStoredConversationId(id: number): void {
		try {
			localStorage.removeItem(conversationStorageKey(id));
		} catch {
			/* noop */
		}
	}

	/**
	 * A proposal that rewords an EXISTING edit (`edit_id` set) should read the same as that
	 * edit's own row in "Your edits" — the operator's saved `rule.title`, not the model's own
	 * `proposal.title` for the reword, which can drift from what the operator named it. Falls
	 * back to the proposal's own title for a brand-new rule (`edit_id: null`) or if the id no
	 * longer matches a rule (deleted mid-conversation).
	 */
	function proposalDisplayTitle(proposal: AssistantProposal): string {
		if (proposal.edit_id !== null) {
			const existing = store?.rules.find((r) => r.id === proposal.edit_id);
			if (existing && existing.title !== null && existing.title.trim().length > 0) {
				return existing.title.trim();
			}
		}
		return proposal.title;
	}

	interface PanelMessage {
		id: number;
		role: 'user' | 'assistant';
		text: string;
		quote: string | null;
		/** A turn can call `propose_edit` more than once — never assume a single proposal. */
		proposals: AssistantProposal[];
		phase: string | null;
		error: string | null;
	}

	let health = $state<AssistantHealth | null>(null);
	let healthError = $state<string | null>(null);
	let conversationId = $state<string | null>(null);
	let messages = $state<PanelMessage[]>([]);
	let draft = $state('');
	let sending = $state(false);
	let sendError = $state<string | null>(null);
	let appliedProposalIds = $state<Set<string>>(new Set());
	let applyingProposalId = $state<string | null>(null);
	let refuseMessages = $state<Record<string, string>>({});
	let useSuggestedTitle = $state<Record<string, boolean>>({});
	let nextMsgId = 0;
	/** Set by `handleReword` → `draftHint`, cleared once it's ridden along in `send()`'s body. */
	let pendingEditId = $state<string | null>(null);
	/** Forced true the moment an in-band "turned off" error arrives, so the panel doesn't wait
	 *  for the next poll tick to say so — cleared once a fresh health fetch reports enabled again. */
	let serviceOffOverride = $state(false);
	/** Which requestId the panel is currently showing a conversation for — drives the
	 *  reset-on-request-change / rehydrate-on-mount-or-remount logic (G6). */
	let currentRequestId = $state<number | null>(null);

	$effect(() => {
		streaming = sending;
	});

	onMount(() => {
		void loadHealth();
		const timer = setInterval(() => void loadHealth(), HEALTH_POLL_MS);
		const onFocus = (): void => void loadHealth();
		const onVisibility = (): void => {
			if (document.visibilityState === 'visible') void loadHealth();
		};
		window.addEventListener('focus', onFocus);
		document.addEventListener('visibilitychange', onVisibility);
		return () => {
			clearInterval(timer);
			window.removeEventListener('focus', onFocus);
			document.removeEventListener('visibilitychange', onVisibility);
		};
	});

	$effect(() => {
		if (draftHint !== null) {
			draft = draftHint.text;
			pendingEditId = draftHint.editId;
			onDraftHintConsumed();
		}
	});

	/**
	 * Resets the conversation when the operator moves to a different request, and rehydrates
	 * it — from `localStorage` + `GET .../conversations/:id` — whenever the panel (re)mounts
	 * showing a request it already has a stored conversation id for. Toggling "Read only" off
	 * and back on unmounts and remounts this whole component (`FilterWorkbench.svelte`'s
	 * `{#if sideOpen}`), which would otherwise throw the conversation away (G6).
	 */
	$effect(() => {
		const id = requestId;
		if (id === currentRequestId) return;
		currentRequestId = id;
		conversationId = null;
		messages = [];
		sendError = null;
		appliedProposalIds = new Set();
		if (id !== null) void rehydrateConversation(id);
	});

	function toPanelMessage(m: AssistantMessageDto): PanelMessage {
		return {
			id: nextMsgId++,
			role: m.role,
			text: m.text,
			quote: m.quote ?? null,
			proposals: m.proposals ?? [],
			phase: null,
			error: null
		};
	}

	async function rehydrateConversation(id: number): Promise<void> {
		const stored = loadStoredConversationId(id);
		if (stored === null) return;
		try {
			const conv = await getAssistantConversation(stored);
			if (currentRequestId !== id) return;
			conversationId = conv.id;
			messages = conv.messages.map(toPanelMessage);
		} catch {
			clearStoredConversationId(id);
		}
	}

	async function loadHealth(): Promise<void> {
		healthError = null;
		try {
			health = await getAssistantHealth();
			if (health.enabled) serviceOffOverride = false;
		} catch (err) {
			health = null;
			healthError = err instanceof ApiError ? err.message : String(err);
		}
	}

	const STARTERS = [
		'Anything here that fights my CLAUDE.md?',
		'What changed since the last Claude Code version?',
		'Shorten this section',
		'Does this fit my other projects?'
	];

	function pushMessage(msg: Omit<PanelMessage, 'id'>): number {
		const id = nextMsgId++;
		messages = [...messages, { ...msg, id }];
		return id;
	}

	function patchMessage(id: number, patch: Partial<PanelMessage>): void {
		messages = messages.map((m) => (m.id === id ? { ...m, ...patch } : m));
	}

	function appendProposal(id: number, proposal: AssistantProposal): void {
		messages = messages.map((m) =>
			m.id === id ? { ...m, proposals: [...m.proposals, proposal], phase: null } : m
		);
	}

	async function send(): Promise<void> {
		if (requestId === null || sending) return;
		const text = draft.trim();
		const sentQuote = quote.trim().length > 0 ? quote : null;
		if (text.length === 0 && sentQuote === null) return;

		// Re-check health right before sending (G7) — the toggle/model can change while the
		// panel sits open, and this is the one moment it actually matters.
		await loadHealth();
		if (serviceDown || serviceOff) return;

		const editIdForThisMessage = pendingEditId;
		pendingEditId = null;

		pushMessage({
			role: 'user',
			text: text.length > 0 ? text : 'What do you make of this part?',
			quote: sentQuote,
			proposals: [],
			phase: null,
			error: null
		});
		draft = '';
		onClearQuote();
		sendError = null;
		sending = true;

		const placeholderId = pushMessage({
			role: 'assistant',
			text: '',
			quote: null,
			proposals: [],
			phase: 'reading prompt',
			error: null
		});

		try {
			if (conversationId === null) {
				const created = await createAssistantConversation(requestId);
				conversationId = created.id;
				storeConversationId(requestId, created.id);
			}
			let assistantText = '';
			await sendAssistantMessage(
				conversationId,
				{
					text: text.length > 0 ? text : 'What do you make of this part?',
					quote: sentQuote ?? undefined,
					edit_id: editIdForThisMessage ?? undefined,
					scope: workbenchScope,
					project_path: workbenchScope === 'project' ? (workbenchProjectPath ?? undefined) : undefined
				},
				(event) => {
					if (event.type === 'text') {
						assistantText += event.delta;
						patchMessage(placeholderId, { text: assistantText, phase: null });
					} else if (event.type === 'status') {
						patchMessage(placeholderId, { phase: event.phase });
					} else if (event.type === 'proposal') {
						appendProposal(placeholderId, event.proposal);
					} else if (event.type === 'error') {
						patchMessage(placeholderId, { error: event.message, phase: null });
						if (event.message.toLowerCase().includes('turned off')) {
							serviceOffOverride = true;
						}
					} else if (event.type === 'done') {
						if (health !== null) {
							health = { ...health, spent_today_usd: health.spent_today_usd + event.cost_usd };
						}
					} else if (event.type === 'applied') {
						appliedProposalIds = new Set(appliedProposalIds).add(event.proposal_id);
						onApplied(event.rule_id);
					}
				}
			);
		} catch (err) {
			const message = err instanceof ApiError ? err.message : String(err);
			patchMessage(placeholderId, { error: message, phase: null });
		} finally {
			sending = false;
		}
	}

	function handleSubmit(e: SubmitEvent): void {
		e.preventDefault();
		void send();
	}

	function onComposerKeydown(e: KeyboardEvent): void {
		if (e.key === 'Enter' && !e.shiftKey) {
			e.preventDefault();
			void send();
		}
	}

	function pickStarter(s: string): void {
		draft = s;
	}

	function togglePreview(proposal: AssistantProposal): void {
		if (previewingProposalId === proposal.id) onStopPreview();
		else onPreview(proposal);
	}

	function existingRuleFor(proposal: AssistantProposal): FilterRuleDto | null {
		if (proposal.edit_id === null) return null;
		return store?.rules.find((r) => r.id === proposal.edit_id) ?? null;
	}

	/** Whether the "use suggested title" opt-in is worth showing at all — only when there's an
	 *  operator-saved title on the existing rule that actually DIFFERS from the model's own. */
	function suggestedTitleAvailable(proposal: AssistantProposal): boolean {
		const existing = existingRuleFor(proposal);
		if (existing === null) return false;
		const saved = existing.title?.trim() ?? '';
		return saved.length > 0 && saved !== proposal.title.trim();
	}

	async function applyProposal(proposal: AssistantProposal): Promise<void> {
		if (!store || applyingProposalId !== null) return;
		refuseMessages = { ...refuseMessages, [proposal.id]: '' };
		const plan = planApplyProposal(proposal, existingRuleFor(proposal), {
			useSuggestedTitle: useSuggestedTitle[proposal.id] ?? false,
			workbenchProjectPath
		});
		if (plan.kind === 'refuse') {
			refuseMessages = { ...refuseMessages, [proposal.id]: plan.reason };
			return;
		}
		applyingProposalId = proposal.id;
		try {
			const rule =
				plan.kind === 'update'
					? await store.update(plan.id, plan.patch)
					: await store.create(plan.body);
			appliedProposalIds = new Set(appliedProposalIds).add(proposal.id);
			if (previewingProposalId === proposal.id) onStopPreview();
			onApplied(rule.id);
		} catch (err) {
			sendError = err instanceof ApiError ? err.message : String(err);
		} finally {
			applyingProposalId = null;
		}
	}

	const serviceDown = $derived(healthError !== null || (health !== null && !health.ok));
	const serviceOff = $derived(
		serviceOffOverride || (health !== null && health.ok && !health.enabled)
	);
</script>

<section
	class="wb-assistant"
	aria-label="Prompt assistant"
	data-slot="workbench-assistant-panel"
>
	<header class="wb-assistant__head">
		<!-- A real heading (h2), matching WorkbenchNavigator/WorkbenchRuleStack's own column
		     titles — see their comments for why each column needs its own heading rather than
		     relying on a sibling column supplying one (axe heading-order; each column collapses
		     independently). The collapse toggle and the tooltip trigger also have to be the SAME
		     element: bits-ui's Tooltip.Trigger renders its own focusable/interactive node, so
		     wrapping it in a second <button> would nest two interactive elements (an axe
		     nested-interactive violation) — the same conflict SectionCard.svelte resolves by
		     keeping its title button and its tooltip as siblings. Here the title needs BOTH
		     behaviours on one label, so the tooltip's own `child` render prop is used to make ITS
		     trigger the collapse button, rather than adding a second one. -->
		<h2 class="wb-assistant__head-heading">
			<Tooltip slug="prompt-assistant">
				{#snippet child({ props })}
					<button
						type="button"
						{...props}
						class="wb-assistant__head-toggle"
						aria-expanded="true"
						aria-label="Collapse prompt assistant"
						onclick={onToggleCollapse}
					>
						<span class="wb-assistant__title">Prompt assistant</span>
						{#if health !== null}
							<span class="wb-assistant__subtitle">
								{health.model} · {health.effort} · {health.may_apply === 'apply'
									? 'applies automatically'
									: 'suggests only'}
							</span>
						{/if}
					</button>
				{/snippet}
			</Tooltip>
		</h2>
		<a
			href={resolve('/settings/assistants')}
			class="wb-assistant__settings"
			aria-label="Prompt assistant settings"
			title="Model, what it can read, limits"
		>
			<SettingsIcon size={13} aria-hidden="true" />
		</a>
	</header>

	{#if health === null && healthError === null}
		<p class="wb-assistant__status">Checking the prompt assistant…</p>
	{:else if serviceDown}
		<p class="wb-assistant__status wb-assistant__status--down">
			Prompt assistant is unreachable{healthError !== null ? ` (${healthError})` : ''}. <a
				href={resolve('/settings/assistants')}>Check settings</a
			>
			or
			<button type="button" class="wb-assistant__retry" onclick={() => void loadHealth()}>
				Retry
			</button>.
		</p>
	{:else if serviceOff}
		<p class="wb-assistant__status">
			Prompt assistant is turned off. <a href={resolve('/settings/assistants')}>Turn it on</a> to talk
			to it here.
		</p>
	{:else if requestId === null}
		<p class="wb-assistant__status">Open a captured prompt to talk to the assistant.</p>
	{:else}
		<div class="wb-assistant__messages" role="log" aria-label="Conversation">
			{#each messages as m (m.id)}
				<div class="wb-msg" class:wb-msg--user={m.role === 'user'}>
					{#if m.quote !== null}
						<div class="wb-msg__quote">{m.quote}</div>
					{/if}
					{#if m.text.length > 0}
						<div class="wb-msg__bubble">
							{#if m.role === 'assistant'}
								<MarkdownText text={m.text} />
							{:else}
								{m.text}
							{/if}
						</div>
					{/if}
					{#if m.phase !== null}
						<p class="wb-msg__phase" aria-live="polite">
							<span class="wb-msg__phase-dots" aria-hidden="true">
								<span></span><span></span><span></span>
							</span>
							{m.phase}
						</p>
					{/if}
					{#if m.error !== null}
						<p class="wb-msg__error" role="alert">{m.error}</p>
					{/if}
					{#each m.proposals as proposal (proposal.id)}
						{@const applied = appliedProposalIds.has(proposal.id)}
						{@const previewing = previewingProposalId === proposal.id}
						<div class="wb-proposal" class:wb-proposal--previewing={previewing}>
							<div class="wb-proposal__title-row">
								<div class="wb-proposal__title">{proposalDisplayTitle(proposal)}</div>
								{#if proposal.project_path}
									<span class="wb-proposal__project-badge" title={proposal.project_path}>
										This project only
									</span>
								{/if}
							</div>
							{#if proposal.before.length > 0}
								<div class="wb-proposal__before">{proposal.before}</div>
							{/if}
							<div class="wb-proposal__after">{proposal.after}</div>
							<span class="wb-proposal__reach">{proposal.reach}</span>
							{#if !applied && suggestedTitleAvailable(proposal)}
								<label class="wb-proposal__title-opt">
									<input
										type="checkbox"
										checked={useSuggestedTitle[proposal.id] ?? false}
										onchange={(e) =>
											(useSuggestedTitle = {
												...useSuggestedTitle,
												[proposal.id]: (e.currentTarget as HTMLInputElement).checked
											})}
									/>
									Use suggested title: “{proposal.title}”
								</label>
							{/if}
							{#if refuseMessages[proposal.id]}
								<p class="wb-proposal__refuse" role="alert">{refuseMessages[proposal.id]}</p>
							{/if}
							<div class="wb-proposal__actions">
								<button
									type="button"
									class="wb-proposal__preview"
									class:wb-proposal__preview--on={previewing}
									aria-pressed={previewing}
									onclick={() => togglePreview(proposal)}
								>
									{previewing ? 'Previewing in prompt' : 'Preview in prompt'}
								</button>
								{#if applied}
									<span class="wb-proposal__applied">applied · live on the next request</span>
								{:else}
									<button
										type="button"
										class="wb-proposal__apply"
										disabled={applyingProposalId === proposal.id}
										onclick={() => void applyProposal(proposal)}
									>
										Apply
									</button>
								{/if}
							</div>
						</div>
					{/each}
				</div>
			{/each}
			{#if messages.length === 0}
				<p class="wb-assistant__empty">
					Ask to reword, shorten, or check a part of the prompt. Hover a paragraph and click "Ask
					about this" to quote it here.
				</p>
			{/if}
		</div>

		{#if sendError !== null}
			<p class="wb-assistant__send-error" role="alert">{sendError}</p>
		{/if}

		<form class="wb-composer" onsubmit={handleSubmit}>
			{#if quote.length > 0}
				<div class="wb-composer__quote">
					<span class="wb-composer__quote-text">{quote}</span>
					<button
						type="button"
						class="wb-composer__quote-clear"
						aria-label="Remove quote"
						onclick={onClearQuote}
					>
						×
					</button>
				</div>
			{/if}
			<div class="wb-composer__starters">
				{#each STARTERS as s (s)}
					<button type="button" class="wb-composer__starter" onclick={() => pickStarter(s)}>
						{s}
					</button>
				{/each}
			</div>
			<div class="wb-composer__row">
				<label class="wb-composer__field">
					<span class="sr-only">Message the prompt assistant</span>
					<textarea
						rows="2"
						placeholder="Ask to reword, shorten, or check a part of the prompt…"
						bind:value={draft}
						bind:this={composerTextarea}
						onkeydown={onComposerKeydown}
					></textarea>
				</label>
				<button type="submit" class="wb-composer__send" disabled={sending}>
					{sending ? 'Sending…' : 'Send'}
				</button>
			</div>
		</form>
	{/if}
</section>

<style>
	.wb-assistant {
		display: flex;
		flex-direction: column;
		min-height: 0;
		height: 100%;
		background: linear-gradient(160deg, var(--funk-magenta-dim), transparent 30%);
	}

	.wb-assistant__head {
		flex-shrink: 0;
		display: flex;
		align-items: center;
		gap: 8px;
		min-height: 38px;
		padding: 0 10px 0 12px;
		border-bottom: 1px solid var(--border-subtle);
	}

	.wb-assistant__head-heading {
		display: contents;
	}

	.wb-assistant__head-toggle {
		flex: 1 1 auto;
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 8px;
		background: transparent;
		border: none;
		padding: 0;
		margin: 0;
		cursor: pointer;
		text-align: left;
	}
	.wb-assistant__head-toggle:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}

	.wb-assistant__title {
		flex-shrink: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--funk-magenta);
	}

	.wb-assistant__subtitle {
		flex: 1 1 auto;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 10px;
		color: var(--text-3);
	}

	.wb-assistant__settings {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		font-size: 10px;
		color: var(--text-3);
		text-decoration: none;
		border: 1px solid var(--border-default);
		border-radius: 3px;
		padding: 4px;
	}
	.wb-assistant__settings:hover {
		color: var(--text-strong);
	}

	.wb-assistant__status {
		margin: 0;
		padding: 12px;
		font-size: 12px;
		line-height: 1.6;
		color: var(--text-2);
	}
	.wb-assistant__status--down {
		color: var(--status-danger-fg);
	}
	.wb-assistant__status a {
		color: inherit;
		text-decoration: underline;
	}
	.wb-assistant__retry {
		background: transparent;
		border: none;
		padding: 0;
		color: inherit;
		text-decoration: underline;
		font: inherit;
		cursor: pointer;
	}

	.wb-assistant__messages {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: 12px;
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.wb-assistant__empty {
		margin: 0;
		font-size: 11px;
		color: var(--text-3);
		line-height: 1.6;
	}

	.wb-msg {
		display: flex;
		flex-direction: column;
		gap: 6px;
		align-self: stretch;
	}
	.wb-msg--user {
		align-self: flex-end;
		max-width: 85%;
		align-items: flex-end;
	}

	.wb-msg__quote {
		padding: 4px 8px;
		border-left: 2px solid var(--border-strong);
		color: var(--text-3);
		font-size: 11px;
		line-height: 1.5;
	}

	.wb-msg__bubble {
		font-size: 12px;
		line-height: 1.6;
		color: var(--text-1);
		white-space: pre-wrap;
	}
	.wb-msg--user .wb-msg__bubble {
		padding: 8px 10px;
		background: var(--surface-2);
		border: 1px solid var(--border-default);
	}

	.wb-msg__phase {
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 3px;
		font-size: 10px;
		color: var(--text-3);
	}
	.wb-msg__phase-dots {
		display: inline-flex;
		gap: 3px;
	}
	.wb-msg__phase-dots span {
		width: 3px;
		height: 3px;
		border-radius: 50%;
		background: var(--text-3);
		animation: wb-dots 1.2s ease-in-out infinite;
	}
	.wb-msg__phase-dots span:nth-child(2) {
		animation-delay: 0.2s;
	}
	.wb-msg__phase-dots span:nth-child(3) {
		animation-delay: 0.4s;
	}
	@keyframes wb-dots {
		0%,
		80%,
		100% {
			opacity: 0.25;
		}
		40% {
			opacity: 1;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.wb-msg__phase-dots span {
			animation: none;
			opacity: 0.6;
		}
	}

	.wb-msg__error {
		margin: 0;
		font-size: 11px;
		color: var(--status-danger-fg);
	}

	.wb-proposal {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 10px;
		background: var(--surface-2);
		border: 1px solid var(--border-default);
	}
	.wb-proposal--previewing {
		border-color: var(--funk-blue-border);
	}

	.wb-proposal__title-row {
		display: flex;
		align-items: center;
		gap: 6px;
		flex-wrap: wrap;
	}

	.wb-proposal__title {
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-3);
	}

	.wb-proposal__project-badge {
		padding: 1px 6px;
		font: 9px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-3);
		border: 1px solid var(--border-default);
		border-radius: 3px;
		white-space: nowrap;
	}

	.wb-proposal__before {
		padding: 6px 8px;
		border-left: 2px solid var(--border-strong);
		color: var(--text-3);
		font-size: 11px;
		line-height: 1.55;
		text-decoration: line-through;
		text-decoration-color: var(--border-strong);
		white-space: pre-wrap;
	}

	.wb-proposal__after {
		padding: 6px 8px;
		border-left: 2px solid var(--funk-blue-border);
		background: var(--funk-blue-dim);
		font-size: 11px;
		line-height: 1.55;
		white-space: pre-wrap;
	}

	.wb-proposal__reach {
		font-size: 10px;
		color: var(--text-3);
	}

	.wb-proposal__title-opt {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 11px;
		color: var(--text-2);
		cursor: pointer;
	}
	.wb-proposal__title-opt input {
		accent-color: var(--funk-magenta);
		cursor: pointer;
	}

	.wb-proposal__refuse {
		margin: 0;
		font-size: 11px;
		color: var(--status-danger-fg);
	}

	.wb-proposal__actions {
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.wb-proposal__preview {
		height: 26px;
		padding: 0 10px;
		font-size: 11px;
		cursor: pointer;
		background: transparent;
		color: var(--text-2);
		border: 1px solid var(--border-default);
	}
	.wb-proposal__preview--on {
		background: var(--funk-chip-blue);
		color: var(--funk-blue);
		border-color: var(--funk-blue-border);
	}

	.wb-proposal__apply {
		height: 26px;
		padding: 0 12px;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		cursor: pointer;
		background: var(--funk-chip-magenta);
		color: var(--funk-magenta);
		border: 1px solid var(--funk-magenta-border);
	}
	.wb-proposal__apply:disabled {
		opacity: 0.6;
		cursor: default;
	}

	.wb-proposal__applied {
		font-size: 11px;
		color: var(--status-success-fg);
	}

	.wb-assistant__send-error {
		margin: 0;
		padding: 0 12px;
		font-size: 11px;
		color: var(--status-danger-fg);
	}

	.wb-composer {
		flex-shrink: 0;
		padding: 10px 12px 12px;
		border-top: 1px solid var(--border-subtle);
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.wb-composer__quote {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 4px 6px 4px 8px;
		border-left: 2px solid var(--funk-magenta-border);
		background: var(--surface-2);
		font-size: 11px;
		color: var(--text-2);
	}
	.wb-composer__quote-text {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.wb-composer__quote-clear {
		width: 22px;
		height: 22px;
		flex-shrink: 0;
		background: transparent;
		border: none;
		color: var(--text-3);
		cursor: pointer;
		font-size: 14px;
		line-height: 1;
	}

	.wb-composer__row {
		display: flex;
		gap: 8px;
		align-items: flex-end;
	}

	.wb-composer__field {
		flex: 1;
		display: flex;
	}
	.wb-composer__field textarea {
		flex: 1;
		resize: none;
		padding: 8px;
		background: var(--surface-bg);
		color: var(--text-1);
		border: 1px solid var(--border-strong);
		font: 12px var(--font-mono, 'JetBrains Mono Variable', monospace);
		line-height: 1.5;
	}
	.wb-composer__field textarea:focus-visible {
		outline: none;
		border-color: var(--interactive-focus-ring);
		box-shadow: 0 0 0 1px var(--interactive-focus-ring);
	}

	.wb-composer__send {
		height: 38px;
		padding: 0 14px;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		font-size: 11px;
		cursor: pointer;
		background: var(--funk-chip-magenta);
		color: var(--funk-magenta);
		border: 1px solid var(--funk-magenta-border);
	}
	.wb-composer__send:disabled {
		opacity: 0.6;
		cursor: default;
	}

	.wb-composer__starters {
		display: flex;
		gap: 6px;
		flex-wrap: wrap;
	}
	.wb-composer__starter {
		height: 24px;
		padding: 0 8px;
		background: transparent;
		border: 1px solid var(--border-default);
		color: var(--text-2);
		font-size: 10px;
		cursor: pointer;
	}
	.wb-composer__starter:hover {
		color: var(--text-strong);
	}

	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip: rect(0 0 0 0);
	}
</style>
