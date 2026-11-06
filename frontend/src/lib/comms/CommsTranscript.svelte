<script lang="ts">
	import { resolve } from '$app/paths';
	import { tick } from 'svelte';
	import type { Component } from 'svelte';
	import {
		AlertTriangle,
		Check,
		CheckCheck,
		CircleAlert,
		Clock,
		CornerUpLeft,
		FileText,
		GitCommitVertical,
		Globe,
		Hash,
		Loader2,
		MessageSquare,
		Paperclip,
		Plus,
		Reply,
		Send,
		StickyNote,
		X
	} from '@lucide/svelte';
	import { shortSessionId } from '$lib/format';
	import { formatRelative, parseIso } from '$lib/time';
	import { splitAckErrorMessage } from '$lib/api/mailbox';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import Badge from '$lib/components/shared/Badge.svelte';
	import { Button } from '$lib/components/ui/button';
	import CommsAvatar from './CommsAvatar.svelte';
	import {
		buildThreadIdentityIndex,
		labelCarriesShortId,
		resolveParticipantLabel
	} from './participantIdentity';
	import { describeMessageCount, projectDisplayName, splitFleetSubject } from './groupThreads';
	import { buildMessageTree, MAX_VISUAL_INDENT_DEPTH, type MessageTreeNode } from './messageTree';
	import {
		contextChipHref,
		contextChipLabel,
		contextChipMode,
		type ContextChipMode
	} from './contextChip';
	import { describeDelivery, toneBadgeClass, type DeliveryTone } from './deliveryPresentation';
	import type {
		ContextItem,
		ContextItemKind,
		PeerSummary,
		ThreadMessageDto,
		ThreadSummaryDto
	} from './types';

	interface Props {
		sessionId: string;
		thread: ThreadSummaryDto | null;
		messages: ThreadMessageDto[];
		loading: boolean;
		error: string | null;
		now: number;
		peers: PeerSummary[];
		replyPhase: 'idle' | 'sending';
		replyError: string | null;
		replyNotice: string | null;
		resolvePhase: 'idle' | 'resolving';
		ackErrors?: Record<string, string>;
		onSendReply: (
			body: string,
			options: { inReplyTo: string | null; context: ContextItem[] }
		) => void;
		onResolve: () => void;
		onAck: (messageUuid: string) => void;
		onOpenThread?: (threadUuid: string) => void;
		readOnly?: boolean;
		subjectLevel?: 'h2' | 'h3';
	}

	const {
		sessionId,
		thread,
		messages,
		loading,
		error,
		now,
		peers,
		replyPhase,
		replyError,
		replyNotice,
		ackErrors = {},
		resolvePhase,
		onSendReply,
		onResolve,
		onAck,
		onOpenThread,
		readOnly = false,
		subjectLevel = 'h3'
	}: Props = $props();

	function deliveryIcon(m: ThreadMessageDto, tone: DeliveryTone): Component {
		if (tone === 'info') return Check;
		if (tone === 'warn' || tone === 'alert') return AlertTriangle;
		if (m.sent_request_id !== null) return CheckCheck;
		if (m.status === 'claimed') return Loader2;
		return Clock;
	}

	const CONTEXT_ICON: Record<ContextItemKind, Component> = {
		request: Hash,
		session: MessageSquare,
		thread: Reply,
		message: CornerUpLeft,
		commit: GitCommitVertical,
		file: FileText,
		issue: CircleAlert,
		url: Globe,
		note: StickyNote
	};

	const CONTEXT_KINDS: ContextItemKind[] = [
		'url',
		'note',
		'request',
		'session',
		'thread',
		'message',
		'commit',
		'file',
		'issue'
	];

	let replyBody = $state('');
	let replyTarget = $state<{ messageUuid: string; senderLabel: string; preview: string } | null>(
		null
	);
	let draftContextItems = $state<ContextItem[]>([]);
	let draftKind = $state<ContextItemKind>('url');
	let draftValue = $state('');
	let draftLabel = $state('');
	let contextOpen = $state(false);
	let listEl: HTMLUListElement | undefined = $state();
	let composeEl: HTMLFormElement | undefined = $state();
	let textareaEl: HTMLTextAreaElement | undefined = $state();
	let highlightedUuid = $state<string | null>(null);
	let highlightTimer: ReturnType<typeof setTimeout> | undefined;

	const canSend = $derived(
		replyPhase !== 'sending' && replyBody.trim().length > 0 && thread !== null
	);
	const tree = $derived(buildMessageTree(messages));
	const threadIdentities = $derived(buildThreadIdentityIndex(thread));
	const peerIdentityByUuid = $derived(new Map(peers.map((p) => [p.sessionId, p.identity])));
	const renderMeta = $derived(computeRenderMeta(tree, now));

	function participantLabel(uuid: string | null): string {
		return resolveParticipantLabel(
			uuid,
			threadIdentities,
			uuid ? peerIdentityByUuid.get(uuid) : undefined
		);
	}

	function participantRoleFields(uuid: string | null): {
		role: string | null;
		agentType: string | null;
	} {
		if (!uuid) return { role: null, agentType: null };
		const identity = threadIdentities.get(uuid);
		if (identity) return { role: identity.role, agentType: identity.agent_type };
		const fallback = peerIdentityByUuid.get(uuid);
		return { role: fallback?.role ?? null, agentType: fallback?.agentType ?? null };
	}

	const replyUuid = $derived(thread?.created_by_session_uuid ?? thread?.participants[0] ?? null);

	function participantHref(uuid: string | null): string | null {
		if (!uuid) return null;
		return resolve(`/sessions/${encodeURIComponent(uuid)}/comms` as `/sessions/${string}/comms`);
	}

	interface MsgRenderMeta {
		showHeader: boolean;
		dayLabel: string | null;
	}

	const GROUP_WINDOW_MS = 5 * 60 * 1000;

	function dayKeyOf(d: Date): string {
		return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
	}

	function dayDividerLabel(iso: string, referenceNow: number): string {
		const d = parseIso(iso) ?? new Date(NaN);
		const nowD = new Date(referenceNow);
		if (dayKeyOf(d) === dayKeyOf(nowD)) return 'Today';
		if (dayKeyOf(d) === dayKeyOf(new Date(referenceNow - 86_400_000))) return 'Yesterday';
		const sameYear = d.getFullYear() === nowD.getFullYear();
		return d.toLocaleDateString(
			'en-US',
			sameYear
				? { month: 'short', day: 'numeric' }
				: { month: 'short', day: 'numeric', year: 'numeric' }
		);
	}

	function computeRenderMeta(
		nodes: MessageTreeNode[],
		referenceNow: number
	): Map<string, MsgRenderMeta> {
		const meta = new Map<string, MsgRenderMeta>();
		let prev: {
			sender: string | null;
			recipient: string;
			depth: number;
			ts: number;
			dayKey: string;
		} | null = null;

		function walk(list: MessageTreeNode[]): void {
			for (const node of list) {
				const m = node.message;
				const parsed = parseIso(m.queued_at);
				const ts = parsed ? parsed.getTime() : NaN;
				const dKey = dayKeyOf(parsed ?? new Date(NaN));
				const dayLabel =
					prev === null || prev.dayKey !== dKey ? dayDividerLabel(m.queued_at, referenceNow) : null;
				const grouped =
					prev !== null &&
					dayLabel === null &&
					prev.sender === m.from_session_uuid &&
					prev.recipient === m.to_session_uuid &&
					prev.depth === node.depth &&
					ts - prev.ts < GROUP_WINDOW_MS;
				meta.set(m.message_uuid, { showHeader: !grouped, dayLabel });
				prev = {
					sender: m.from_session_uuid,
					recipient: m.to_session_uuid,
					depth: node.depth,
					ts,
					dayKey: dKey
				};
				walk(node.children);
			}
		}
		walk(nodes);
		return meta;
	}

	const REPLY_PREVIEW_WORDS = 8;
	function firstWords(body: string): string {
		const words = body.trim().split(/\s+/).filter(Boolean);
		const shown = words.slice(0, REPLY_PREVIEW_WORDS).join(' ');
		return words.length > REPLY_PREVIEW_WORDS ? `${shown} ...` : shown;
	}

	async function startReply(m: ThreadMessageDto): Promise<void> {
		replyTarget = {
			messageUuid: m.message_uuid,
			senderLabel: participantLabel(m.from_session_uuid),
			preview: firstWords(m.body)
		};
		await tick();
		composeEl?.scrollIntoView({
			block: 'nearest',
			behavior: 'smooth'
		});
		textareaEl?.focus();
	}

	function cancelReply(): void {
		replyTarget = null;
	}

	function addContextItem(): void {
		const value = draftValue.trim();
		if (value.length === 0) return;
		const label = draftLabel.trim();
		draftContextItems = [
			...draftContextItems,
			{ kind: draftKind, value, label: label.length > 0 ? label : null }
		];
		draftValue = '';
		draftLabel = '';
	}

	function removeContextItem(index: number): void {
		draftContextItems = draftContextItems.filter((_, i) => i !== index);
	}

	function submit(): void {
		if (!canSend) return;
		onSendReply(replyBody, {
			inReplyTo: replyTarget?.messageUuid ?? null,
			context: draftContextItems
		});
		replyBody = '';
		replyTarget = null;
		draftContextItems = [];
		contextOpen = false;
	}

	function handleKeydown(e: KeyboardEvent): void {
		if (e.key === 'Enter' && !e.shiftKey) {
			e.preventDefault();
			submit();
		} else if (e.key === 'Escape' && replyTarget) {
			e.preventDefault();
			cancelReply();
		}
	}

	function canAck(m: ThreadMessageDto): boolean {
		return (
			m.to_session_uuid === sessionId && m.status !== 'consumed' && m.status !== 'dead_lettered'
		);
	}

	function jumpToMessage(uuid: string): void {
		const el = listEl?.querySelector<HTMLElement>(`[data-message-uuid="${CSS.escape(uuid)}"]`);
		if (!el) return;
		el.scrollIntoView({ block: 'center', behavior: 'smooth' });
		highlightedUuid = uuid;
		if (highlightTimer) clearTimeout(highlightTimer);
		highlightTimer = setTimeout(() => {
			highlightedUuid = null;
		}, 1400);
	}

	function effectiveChipMode(item: ContextItem): ContextChipMode {
		const mode = contextChipMode(item.kind);
		if (mode !== 'action') return mode;
		if (item.kind === 'message')
			return messages.some((m) => m.message_uuid === item.value) ? 'action' : 'label';
		if (item.kind === 'thread') return onOpenThread ? 'action' : 'label';
		return mode;
	}

	function activateChip(item: ContextItem): void {
		if (item.kind === 'message') jumpToMessage(item.value);
		else if (item.kind === 'thread') onOpenThread?.(item.value);
	}
</script>

{#snippet contextChipRow(items: ContextItem[])}
	{#if items.length > 0}
		<ul class="ctx-row" role="list">
			{#each items as item, i (item.kind + item.value + i)}
				{@const Icon = CONTEXT_ICON[item.kind]}
				{@const mode = effectiveChipMode(item)}
				<li>
					{#if mode === 'internal-link'}
						<a
							class="ctx-chip"
							href={contextChipHref(item, sessionId)}
							title="{item.kind}: {item.value}"
						>
							<Icon size={11} aria-hidden="true" />
							{contextChipLabel(item)}
						</a>
					{:else if mode === 'external-link'}
						<a
							class="ctx-chip"
							href={contextChipHref(item, sessionId)}
							target="_blank"
							rel="noopener noreferrer"
							title="{item.kind}: {item.value}"
						>
							<Icon size={11} aria-hidden="true" />
							{contextChipLabel(item)}
						</a>
					{:else if mode === 'action'}
						<button
							type="button"
							class="ctx-chip ctx-chip--action"
							onclick={() => activateChip(item)}
							title="{item.kind}: {item.value}"
						>
							<Icon size={11} aria-hidden="true" />
							{contextChipLabel(item)}
						</button>
					{:else}
						<span class="ctx-chip ctx-chip--label" title="{item.kind}: {item.value}">
							<Icon size={11} aria-hidden="true" />
							{contextChipLabel(item)}
						</span>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
{/snippet}

{#snippet msgNode(node: MessageTreeNode)}
	{@const m = node.message}
	{@const depth = node.depth}
	{@const visualDepth = Math.min(depth, MAX_VISUAL_INDENT_DEPTH)}
	{@const meta = renderMeta.get(m.message_uuid) ?? { showHeader: true, dayLabel: null }}
	{@const fromRoleFields = participantRoleFields(m.from_session_uuid)}
	{@const fromLabel = participantLabel(m.from_session_uuid)}
	{@const fromShort =
		m.from_session_uuid && !labelCarriesShortId(fromLabel, m.from_session_uuid)
			? shortSessionId(m.from_session_uuid)
			: null}
	{@const toLabel = participantLabel(m.to_session_uuid)}
	{@const toShort = labelCarriesShortId(toLabel, m.to_session_uuid)
		? null
		: shortSessionId(m.to_session_uuid)}
	{@const presentation = describeDelivery(m, now)}
	{@const DeliveryIcon = deliveryIcon(m, presentation.tone)}
	{@const ackError = ackErrors[m.message_uuid] ?? null}

	{#if meta.dayLabel}
		<li class="day-divider">
			<span class="day-divider__label">{meta.dayLabel}</span>
		</li>
	{/if}

	<li
		class="transcript-msg"
		class:transcript-msg--reply={depth > 0}
		class:transcript-msg--grouped={!meta.showHeader}
		class:transcript-msg--highlighted={highlightedUuid === m.message_uuid}
		style:margin-left="{visualDepth * 16}px"
		data-message-uuid={m.message_uuid}
		data-depth={depth}
		title={depth > 0 ? `reply — depth ${depth}` : undefined}
	>
		<div class="transcript-msg__row">
			<div class="msg-gutter">
				{#if meta.showHeader}
					<CommsAvatar
						sessionUuid={m.from_session_uuid ?? ''}
						label={fromLabel}
						role={fromRoleFields.role}
						agentType={fromRoleFields.agentType}
						size="message"
						sender={m.from_session_uuid === null ? 'mike' : 'session'}
					/>
				{:else}
					<span class="msg-gutter__time" title={m.queued_at}
						>{formatRelative(m.queued_at, now)}</span
					>
				{/if}
			</div>

			<div class="transcript-msg__body-col">
				<div class="transcript-msg__head">
					{#if meta.showHeader}
						{#if participantHref(m.from_session_uuid)}
							<a
								class="transcript-msg__sender"
								href={participantHref(m.from_session_uuid)}
								title={m.from_session_uuid ?? undefined}
								aria-label={`Open comms for ${fromLabel}`}
							>
								{fromLabel}
								{#if fromShort}<span class="transcript-msg__uuid" aria-hidden="true"
										>{fromShort}</span
									>{/if}
							</a>
						{:else}
							<span class="transcript-msg__sender">{fromLabel}</span>
						{/if}
						{#if m.to_session_uuid !== sessionId}
							<a
								class="transcript-msg__to-label"
								href={participantHref(m.to_session_uuid)}
								title={m.to_session_uuid}
								aria-label={`Open comms for ${toLabel}`}
							>
								to {toLabel}{#if toShort}<span class="transcript-msg__uuid" aria-hidden="true"
										>{toShort}</span
									>{/if}
							</a>
						{/if}
						<span class="transcript-msg__time" title={m.queued_at}
							>{formatRelative(m.queued_at, now)}</span
						>
					{/if}
					<span
						class="badge-nx {toneBadgeClass(presentation.tone)} transcript-msg__status"
						title="{presentation.detail} · {m.delivery_attempts} delivery attempt{m.delivery_attempts ===
						1
							? ''
							: 's'}"
					>
						<DeliveryIcon size={11} aria-hidden="true" />
						<span class="transcript-msg__status-text">{presentation.label}</span>
					</span>
					{#if m.sent_request_id !== null}
						<a
							class="transcript-msg__req"
							href={resolve(
								`/sessions/${encodeURIComponent(m.to_session_uuid)}/requests/${m.sent_request_id}` as `/sessions/${string}/requests/${string}`
							)}
						>
							#{m.sent_request_id}
						</a>
					{/if}
				</div>

				{#if ackError}
					{@const ackErrorParts = splitAckErrorMessage(ackError)}
					<p
						class="transcript-msg__ack-error"
						role="alert"
						title={ackErrorParts.detail ?? undefined}
					>
						Couldn't acknowledge: {ackErrorParts.text}
					</p>
				{/if}

				<pre class="transcript-msg__body">{m.body}</pre>

				{@render contextChipRow(m.context_items)}
			</div>

			{#if !readOnly || canAck(m)}
				<div class="msg-toolbar">
					{#if !readOnly}
						<button
							type="button"
							class="msg-toolbar__btn"
							onclick={() => void startReply(m)}
							aria-label={`Reply to ${fromLabel}`}
						>
							<CornerUpLeft size={11} aria-hidden="true" />
							Reply
						</button>
					{/if}
					{#if canAck(m)}
						<button
							type="button"
							class="msg-toolbar__btn"
							onclick={() => onAck(m.message_uuid)}
							aria-label={`Ack message from ${fromLabel}`}
						>
							Ack
						</button>
					{/if}
				</div>
			{/if}
		</div>
	</li>
	{#each node.children as child (child.message.message_uuid)}
		{@render msgNode(child)}
	{/each}
{/snippet}

<div class="transcript" class:transcript--readonly={readOnly}>
	{#if !thread}
		<p class="transcript__empty">Select a thread to read it</p>
	{:else}
		{@const split = splitFleetSubject(thread)}
		<div class="transcript__head">
			<div class="transcript__head-row">
				<svelte:element this={subjectLevel} class="transcript__subject"
					>{split ? split.lead : thread.subject}</svelte:element
				>
				{#if thread.resolved_at === null}
					<Button
						variant="outline"
						size="sm"
						disabled={resolvePhase === 'resolving'}
						onclick={onResolve}
					>
						{resolvePhase === 'resolving' ? 'Resolving…' : 'Resolve thread'}
					</Button>
				{:else}
					<span class="badge-nx badge-active"
						>resolved {formatRelative(thread.resolved_at, now)}</span
					>
				{/if}
			</div>
			{#if split}
				<p class="transcript__goal">{split.rest}</p>
			{/if}
			<div class="transcript__meta">
				{#if thread.project}
					<span>{projectDisplayName(thread.project.name, thread.project.path)}</span>
				{/if}
				{#if thread.fleet && !split}
					<Badge variant="tag-blue">fleet {thread.fleet.slug}</Badge>
				{/if}
				<span>{describeMessageCount(thread.message_count)}</span>
				<span>started {formatRelative(thread.created_at, now)}</span>
				{#if readOnly}
					<span class="transcript__note">
						Reply from
						{#if replyUuid}
							<a href={participantHref(replyUuid)}>{participantLabel(replyUuid)}</a>
						{:else}
							the participant's own comms page
						{/if}
					</span>
				{/if}
			</div>
		</div>

		{#if loading}
			<div class="transcript__skeleton" aria-hidden="true">
				<div class="transcript__skel-row"></div>
				<div class="transcript__skel-row transcript__skel-row--short"></div>
			</div>
		{/if}

		{#if error}
			<LoadErrorBanner title="Couldn't load this thread" detail={error} />
		{/if}

		{#if !loading && !error}
			<ul class="transcript__list" role="list" bind:this={listEl}>
				{#each tree as node (node.message.message_uuid)}
					{@render msgNode(node)}
				{/each}
			</ul>
		{/if}

		{#if !readOnly}
			<form
				class="transcript-compose"
				data-augmented-ui="tl-clip br-clip border"
				bind:this={composeEl}
				onsubmit={(e) => {
					e.preventDefault();
					submit();
				}}
			>
				{#if replyTarget}
					<div class="reply-target-chip">
						<CornerUpLeft size={11} aria-hidden="true" />
						<span
							>Replying to <strong>{replyTarget.senderLabel}</strong> — {replyTarget.preview}</span
						>
						<button
							type="button"
							class="reply-target-chip__cancel"
							onclick={cancelReply}
							aria-label="Cancel reply target"
						>
							<X size={12} aria-hidden="true" />
						</button>
					</div>
				{/if}

				{#if replyNotice}
					<p class="transcript-compose__notice" role="status">{replyNotice}</p>
				{/if}
				{#if replyError}
					<p class="transcript-compose__error" role="alert">{replyError}</p>
				{/if}

				<div class="transcript-compose__bar">
					<button
						type="button"
						class="transcript-compose__attach"
						class:transcript-compose__attach--active={contextOpen}
						onclick={() => (contextOpen = !contextOpen)}
						aria-expanded={contextOpen}
						aria-label={draftContextItems.length > 0
							? `Attach context, ${draftContextItems.length} attached`
							: 'Attach context'}
						title="Attach context"
					>
						<Paperclip size={14} aria-hidden="true" />
						{#if draftContextItems.length > 0}<span class="transcript-compose__attach-count"
								>{draftContextItems.length}</span
							>{/if}
					</button>

					<textarea
						class="transcript-compose__textarea"
						bind:this={textareaEl}
						bind:value={replyBody}
						onkeydown={handleKeydown}
						aria-label={replyTarget ? `Reply to ${replyTarget.senderLabel}` : `Message this thread`}
						placeholder={replyTarget
							? `Reply to ${replyTarget.senderLabel}`
							: `Message this thread`}
						rows="1"></textarea>

					<button
						type="submit"
						class="transcript-compose__send"
						aria-label={replyPhase === 'sending' ? 'Sending' : 'Send'}
						title={replyPhase === 'sending' ? 'Sending' : 'Send'}
						disabled={!canSend}
					>
						<Send size={15} aria-hidden="true" />
					</button>
				</div>

				{#if contextOpen}
					<div class="ctx-picker">
						{#if draftContextItems.length > 0}
							<ul class="ctx-picker__list" role="list">
								{#each draftContextItems as item, i (item.kind + item.value + i)}
									<li class="ctx-chip ctx-chip--draft">
										{item.kind}: {contextChipLabel(item)}
										<button
											type="button"
											onclick={() => removeContextItem(i)}
											aria-label="Remove {item.kind} {item.value}"
										>
											<X size={11} aria-hidden="true" />
										</button>
									</li>
								{/each}
							</ul>
						{/if}
						<div class="ctx-picker__row">
							<select class="ctx-picker__input" bind:value={draftKind} aria-label="Context kind">
								{#each CONTEXT_KINDS as kind (kind)}
									<option value={kind}>{kind}</option>
								{/each}
							</select>
							<input
								class="ctx-picker__input"
								type="text"
								bind:value={draftValue}
								placeholder="value"
								aria-label="Context value"
							/>
							<input
								class="ctx-picker__input"
								type="text"
								bind:value={draftLabel}
								placeholder="label (optional)"
								aria-label="Context label"
							/>
							<button
								type="button"
								class="ctx-picker__add"
								onclick={addContextItem}
								aria-label="Add context item"
							>
								<Plus size={12} aria-hidden="true" />
							</button>
						</div>
					</div>
				{/if}
			</form>
		{/if}
	{/if}
</div>

<style>
	.transcript {
		display: flex;
		flex-direction: column;
		gap: 10px;
		min-width: 0;
		height: 100%;
	}

	.transcript__empty {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-3);
	}

	.transcript__head {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding-bottom: var(--space-3);
		border-bottom: var(--hairline) solid var(--border-subtle);
	}

	.transcript__head-row {
		display: flex;
		align-items: flex-start;
		gap: var(--space-3);
	}

	.transcript__subject {
		flex: 1;
		min-width: 0;
		margin: 0;
		overflow-wrap: anywhere;
		font-family: var(--font-display);
		font-size: var(--text-lg);
		font-weight: var(--weight-display);
		line-height: var(--leading-tight);
		color: var(--text-strong);
	}

	.transcript__goal {
		margin: 0;
		max-width: 92ch;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-sm);
		line-height: var(--leading-body);
		color: var(--text-2);
		overflow-wrap: anywhere;
	}

	.transcript__meta {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-1) var(--space-3);
		font-family: var(--font-mono, monospace);
		font-size: var(--text-label);
		color: var(--text-3);
	}

	.transcript__note :global(a) {
		color: var(--status-info-fg);
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	.transcript__note :global(a:focus-visible) {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.transcript__skeleton {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.transcript__skel-row {
		height: 56px;
		border-radius: 2px;
		background: linear-gradient(
			90deg,
			var(--surface-2) 0%,
			var(--surface-shine) 50%,
			var(--surface-2) 100%
		);
		background-size: 200% 100%;
	}

	.transcript__skel-row--short {
		height: 32px;
		opacity: 0.7;
	}

	.transcript__skel-row {
		animation: transcript-shimmer 1.6s ease-in-out infinite;
	}

	@keyframes transcript-shimmer {
		0% {
			background-position: 200% 0;
		}
		100% {
			background-position: -200% 0;
		}
	}

	.transcript__list {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0;
	}

	.day-divider {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		margin: var(--space-1) 0;
	}

	.day-divider::before,
	.day-divider::after {
		content: '';
		flex: 1;
		height: 1px;
		background: var(--border-subtle);
	}

	.day-divider__label {
		flex: none;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}

	.transcript-msg {
		position: relative;
		padding: var(--space-3) 112px var(--space-3) var(--space-1);
		background: transparent;
	}

	.transcript--readonly .transcript-msg {
		padding-right: var(--space-1);
	}

	.transcript-msg:not(.transcript-msg--grouped) {
		border-top: var(--hairline) solid var(--border-subtle);
	}

	.day-divider + .transcript-msg {
		border-top: none;
	}

	.transcript-msg--grouped {
		padding-top: 0;
	}

	.transcript-msg--reply {
		border-left: 2px solid var(--border-subtle);
		padding-left: 12px;
	}

	.transcript-msg--highlighted {
		background: var(--surface-2);
	}

	.transcript-msg--highlighted {
		transition: background var(--dur-3, 600ms) var(--ease-snap);
	}

	.transcript-msg__row {
		display: flex;
		align-items: flex-start;
		gap: 8px;
	}

	.msg-gutter {
		flex: none;
		width: var(--control-h);
		display: flex;
		align-items: flex-start;
		justify-content: center;
		padding-top: 2px;
	}

	.msg-gutter__time {
		opacity: 0;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-micro);
		white-space: nowrap;
		color: var(--text-3);
	}

	.transcript-msg:hover .msg-gutter__time,
	.transcript-msg:focus-within .msg-gutter__time {
		opacity: 1;
	}

	.msg-gutter__time {
		transition: opacity var(--dur-1) var(--ease-snap);
	}

	.transcript-msg__body-col {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.transcript-msg__head {
		display: flex;
		align-items: baseline;
		flex-wrap: wrap;
		gap: var(--space-1) var(--space-2);
	}

	.transcript-msg__sender {
		font-family: var(--font-display);
		font-size: var(--text-md);
		font-weight: 700;
		color: var(--text-strong);
		text-decoration: none;
	}

	.transcript-msg__sender:hover,
	.transcript-msg__to-label:hover {
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	.transcript-msg__sender:focus-visible,
	.transcript-msg__to-label:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 1px;
	}

	.transcript-msg__to-label {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption);
		color: var(--text-3);
		text-decoration: none;
	}

	.transcript-msg__uuid {
		margin-left: 4px;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption);
		font-weight: 400;
		color: var(--text-3);
	}

	.transcript-msg__status-text {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
	}

	.transcript-msg__req {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--status-success-fg);
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	.transcript-msg__time {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption);
		color: var(--text-3);
	}

	.transcript-msg__ack-error {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--status-danger-fg);
	}

	.transcript-msg__body {
		margin: 0;
		max-width: 92ch;
		padding: 0;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-sm);
		line-height: 1.5;
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}

	.msg-toolbar {
		position: absolute;
		top: 2px;
		right: 6px;
		display: flex;
		gap: 4px;
		opacity: 0;
	}

	.transcript-msg:hover .msg-toolbar,
	.transcript-msg:focus-within .msg-toolbar {
		opacity: 1;
	}

	.msg-toolbar {
		transition: opacity var(--dur-1) var(--ease-snap);
	}

	.msg-toolbar__btn {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 2px 8px;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		background: var(--surface-2);
		color: var(--text-2);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		cursor: pointer;
	}

	.msg-toolbar__btn:hover {
		color: var(--text-strong);
		border-color: var(--interactive-active-border);
	}

	.msg-toolbar__btn:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 1px;
	}

	.ctx-row {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.ctx-chip {
		--edge: 4px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 2px 7px;
		background: var(--surface-2);
		color: var(--text-2);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		text-decoration: none;
		cursor: pointer;
		border: none;
	}

	a.ctx-chip:hover,
	button.ctx-chip--action:hover {
		--aug-border-bg: var(--interactive-active-border);
		color: var(--text-strong);
	}

	.ctx-chip--label {
		cursor: default;
		opacity: 0.85;
	}

	.ctx-chip:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.ctx-chip--draft {
		display: inline-flex;
		align-items: center;
		gap: 4px;
	}

	.ctx-chip--draft button {
		display: inline-flex;
		border: none;
		background: transparent;
		color: var(--text-3);
		cursor: pointer;
	}

	.ctx-chip--draft button:hover {
		color: var(--status-danger-fg);
	}

	.transcript-compose {
		--edge: var(--edge-sm);
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		position: sticky;
		bottom: 0;
		z-index: 1;
		flex: none;
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 8px 10px;
		background: var(--surface-card);
	}

	.reply-target-chip {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 5px 8px;
		border: 1px solid var(--status-info-border);
		border-radius: 2px;
		background: var(--status-info-bg);
		color: var(--status-info-fg);
		font-family: var(--font-mono, monospace);
		font-size: 11px;
	}

	.reply-target-chip span {
		flex: 1;
		min-width: 0;
		overflow-wrap: anywhere;
	}

	.reply-target-chip__cancel {
		flex: none;
		display: inline-flex;
		border: none;
		background: transparent;
		color: inherit;
		cursor: pointer;
	}

	.transcript-compose__bar {
		display: flex;
		align-items: flex-end;
		gap: 6px;
	}

	.transcript-compose__attach {
		position: relative;
		flex: none;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: var(--control-h);
		height: var(--control-h);
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		background: var(--surface-2);
		color: var(--text-2);
		cursor: pointer;
	}

	.transcript-compose__attach:hover {
		color: var(--text-strong);
		border-color: var(--interactive-active-border);
	}

	.transcript-compose__attach--active {
		color: var(--status-info-fg);
		border-color: var(--status-info-border);
	}

	.transcript-compose__attach-count {
		position: absolute;
		top: -4px;
		right: -4px;
		min-width: 14px;
		height: 14px;
		padding: 0 3px;
		border-radius: 7px;
		background: var(--status-info-bg);
		color: var(--status-info-fg);
		font-family: var(--font-mono, monospace);
		font-size: 9px;
		line-height: 14px;
		text-align: center;
	}

	.transcript-compose__textarea {
		flex: 1;
		min-width: 0;
		resize: vertical;
		min-height: var(--control-h);
		max-height: 160px;
		padding: 6px 8px;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		background: var(--surface-2);
		color: var(--text-1);
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		line-height: 1.5;
	}

	.transcript-compose__textarea:focus-visible {
		outline: 2px solid var(--status-info-border);
		outline-offset: 1px;
	}

	.ctx-picker {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-2);
	}

	.ctx-picker__list {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		list-style: none;
		margin: 0 0 6px;
		padding: 0;
	}

	.ctx-picker__row {
		display: flex;
		gap: 6px;
	}

	.ctx-picker__input {
		flex: 1;
		min-width: 0;
		padding: 4px 6px;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		background: var(--surface-2);
		color: var(--text-1);
		font-family: var(--font-mono, monospace);
		font-size: 11px;
	}

	.ctx-picker__add {
		flex: none;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		background: var(--surface-2);
		color: var(--text-2);
		cursor: pointer;
	}

	.ctx-picker__add:hover {
		color: var(--text-strong);
		border-color: var(--interactive-active-border);
	}

	.transcript-compose__notice {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--status-info-fg);
	}

	.transcript-compose__error {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--status-danger-fg);
	}

	.transcript-compose__send {
		flex: none;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: var(--control-h);
		height: var(--control-h);
		border: none;
		border-radius: 2px;
		background: var(--status-steering-bg);
		color: var(--status-steering-fg);
		cursor: pointer;
		transition: background var(--dur-1) var(--ease-snap);
	}

	.transcript-compose__send:hover:not(:disabled) {
		background: var(--status-steering-border);
	}

	.transcript-compose__send:disabled {
		opacity: 0.55;
		cursor: not-allowed;
	}
</style>
