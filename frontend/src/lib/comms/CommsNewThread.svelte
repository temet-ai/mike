<script lang="ts">
	import { Plus, X } from '@lucide/svelte';
	import { shortSessionId } from '$lib/format';
	import { roleTokens } from '$lib/fleet/roleTokens';
	import { ApiError } from '$lib/api';
	import * as Field from '$lib/components/ui/field';
	import { isTeamCommsUnavailable, postOpenThread } from './teamComms';
	import type { PeerSummary } from './types';

	const CUSTOM_RECIPIENT = '__custom__';

	interface Props {
		sessionId: string | null;
		project: string | null;
		peers: PeerSummary[];
		legacyPhase?: 'idle' | 'sending';
		legacyError?: string | null;
		onLegacySend?: (to: string, subject: string, body: string, tags: string[]) => void;
		onOpened: (threadUuid: string) => void;
	}

	const {
		sessionId,
		project,
		peers,
		legacyPhase = 'idle',
		legacyError = null,
		onLegacySend,
		onOpened
	}: Props = $props();

	let open = $state(false);
	let subject = $state('');
	let tags = $state('');
	let body = $state('');
	let notifyList = $state<string[]>([]);
	let notifyChoice = $state('');
	let notifyCustom = $state('');

	let sendPhase = $state<'idle' | 'sending'>('idle');
	let sendError = $state<string | null>(null);
	let degradeNotice = $state<string | null>(null);

	let subjectInputEl: HTMLInputElement | HTMLTextAreaElement | null = $state(null);
	let toggleButtonEl: HTMLButtonElement | undefined = $state();

	const canSubmit = $derived(
		sendPhase !== 'sending' && subject.trim().length > 0 && body.trim().length > 0
	);
	const identityByPeer = $derived(new Map(peers.map((p) => [p.sessionId, p.identity])));

	function parseTags(raw: string): string[] {
		return raw
			.split(',')
			.map((t) => t.trim())
			.filter((t) => t.length > 0);
	}

	function addNotify(): void {
		const candidate = (notifyChoice === CUSTOM_RECIPIENT ? notifyCustom : notifyChoice).trim();
		if (candidate.length === 0 || notifyList.includes(candidate)) return;
		notifyList = [...notifyList, candidate];
		notifyChoice = '';
		notifyCustom = '';
	}

	function removeNotify(sid: string): void {
		notifyList = notifyList.filter((s) => s !== sid);
	}

	function notifyLabel(sid: string): string {
		const identity = identityByPeer.get(sid);
		if (!identity) return shortSessionId(sid);
		const tokens = roleTokens(identity.role);
		return `${identity.label ?? tokens.label} · ${shortSessionId(sid)}`;
	}

	function resetForm(): void {
		subject = '';
		tags = '';
		body = '';
		notifyList = [];
		notifyChoice = '';
		notifyCustom = '';
	}

	async function submit(): Promise<void> {
		if (!canSubmit) return;
		const trimmedSubject = subject.trim();
		const trimmedBody = body.trim();
		const tagList = parseTags(tags);
		sendPhase = 'sending';
		sendError = null;
		degradeNotice = null;
		try {
			const res = await postOpenThread({
				subject: trimmedSubject,
				body: trimmedBody,
				from_session_id: sessionId,
				project_path: project,
				tags: tagList,
				notify: notifyList
			});
			resetForm();
			open = false;
			sendPhase = 'idle';
			onOpened(res.thread_uuid);
			return;
		} catch (err) {
			if (!isTeamCommsUnavailable(err)) {
				sendError = err instanceof ApiError ? err.message : String(err);
				sendPhase = 'idle';
				return;
			}
		}

		if (onLegacySend && notifyList.length > 0) {
			for (const to of notifyList) {
				onLegacySend(to, trimmedSubject, trimmedBody, tagList);
			}
			degradeNotice =
				notifyList.length === 1
					? "Open threads aren't live yet — sent directly to the notified session instead."
					: `Open threads aren't live yet — sent directly to each of the ${notifyList.length} notified sessions instead.`;
			resetForm();
			open = false;
		} else if (onLegacySend) {
			degradeNotice =
				'Open threads aren\'t live yet for this session. Add at least one session under "Notify sessions" to send this for now.';
		} else {
			degradeNotice =
				"Open threads aren't live here yet — try again shortly, or start this thread from a session's own comms page.";
		}
		sendPhase = 'idle';
	}

	function handleKeydown(e: KeyboardEvent): void {
		if (e.key === 'Enter' && !e.shiftKey) {
			e.preventDefault();
			void submit();
		}
	}

	function closeForm(): void {
		open = false;
		toggleButtonEl?.focus();
	}

	function handleFormKeydown(e: KeyboardEvent): void {
		if (e.key === 'Escape') {
			e.preventDefault();
			closeForm();
		}
	}

	$effect(() => {
		if (open) subjectInputEl?.focus();
	});
</script>

<div class="new-thread">
	<button
		type="button"
		class="new-thread__toggle"
		bind:this={toggleButtonEl}
		onclick={() => (open = !open)}
		aria-expanded={open}
	>
		<Plus size={13} aria-hidden="true" />
		New thread
	</button>

	{#if open}
		<!-- svelte-ignore a11y_no_noninteractive_element_interactions -- onkeydown only catches
		     Escape bubbling up from the form's own focusable controls (inputs, textarea, buttons);
		     the <form> is never itself a focus or click target. -->
		<form
			class="new-thread__form"
			data-augmented-ui="tl-clip br-clip border"
			onsubmit={(e) => {
				e.preventDefault();
				void submit();
			}}
			onkeydown={handleFormKeydown}
		>
			<Field.Root
				label="Subject"
				hint="(required)"
				bind:value={subject}
				bind:ref={subjectInputEl}
				placeholder="what this thread is about"
				required
				aria-required="true"
			/>
			<Field.Root
				label="Message"
				hint="(required)"
				multiline
				rows={3}
				bind:value={body}
				onkeydown={handleKeydown}
				placeholder="press Enter to start the thread, Shift+Enter for a new line"
				required
				aria-required="true"
			/>
			<Field.Root
				label="Tags"
				hint="(optional, comma-separated)"
				bind:value={tags}
				placeholder="blocker, db"
			/>

			<details class="notify-picker">
				<summary class="notify-picker__toggle">
					Notify sessions {notifyList.length > 0
						? `(${notifyList.length})`
						: '(optional — no recipient needed)'}
				</summary>
				<p class="notify-picker__blurb">
					A thread works with no recipient — any session can find and reply to it later. Notify
					specific sessions here only if you want to alert them right away.
				</p>

				{#if notifyList.length > 0}
					<ul class="notify-picker__list" role="list">
						{#each notifyList as sid (sid)}
							<li class="notify-chip">
								{notifyLabel(sid)}
								<button
									type="button"
									onclick={() => removeNotify(sid)}
									aria-label="Remove {notifyLabel(sid)} from notify list"
								>
									<X size={11} aria-hidden="true" />
								</button>
							</li>
						{/each}
					</ul>
				{/if}

				<div class="notify-picker__row">
					<select
						class="notify-picker__input"
						bind:value={notifyChoice}
						aria-label="Session to notify"
					>
						<option value="">Select a peer</option>
						{#each peers as peer (peer.sessionId)}
							{@const tokens = roleTokens(peer.identity.role)}
							<option value={peer.sessionId}
								>{peer.identity.label ?? tokens.label} · {shortSessionId(peer.sessionId)}</option
							>
						{/each}
						<option value={CUSTOM_RECIPIENT}>Other (enter manually)</option>
					</select>
					{#if notifyChoice === CUSTOM_RECIPIENT}
						<input
							class="notify-picker__input"
							type="text"
							aria-label="Session uuid, uuid prefix, or role:<name>"
							bind:value={notifyCustom}
							placeholder="session uuid, uuid prefix, or role:<name>"
						/>
					{/if}
					<button
						type="button"
						class="notify-picker__add"
						onclick={addNotify}
						aria-label="Add session to notify list"
					>
						<Plus size={12} aria-hidden="true" />
					</button>
				</div>
			</details>

			{#if degradeNotice}
				<p class="new-thread__notice" role="status">{degradeNotice}</p>
			{/if}
			{#if sendError}
				<p class="new-thread__error" role="alert">{sendError}</p>
			{/if}
			{#if legacyError}
				<p class="new-thread__error" role="alert">{legacyError}</p>
			{/if}
			<div class="new-thread__actions">
				<button
					type="button"
					class="new-thread__cancel"
					onclick={() => {
						resetForm();
						closeForm();
					}}
				>
					Cancel
				</button>
				<button
					type="submit"
					class="new-thread__submit"
					data-augmented-ui="tl-clip br-clip border"
					disabled={!canSubmit}
				>
					{sendPhase === 'sending' || legacyPhase === 'sending' ? 'Starting…' : 'Start thread'}
				</button>
			</div>
		</form>
	{/if}
</div>

<style>
	.new-thread {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.new-thread__toggle {
		display: inline-flex;
		align-self: flex-start;
		align-items: center;
		gap: 5px;
		padding: 4px 10px;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		background: var(--surface-2);
		color: var(--text-2);
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		cursor: pointer;
	}

	.new-thread__toggle:hover {
		color: var(--text-strong);
		border-color: var(--interactive-active-border);
	}

	.new-thread__toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.new-thread__form {
		--edge: 6px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 10px 12px;
		background: var(--surface-card);
	}

	.notify-picker {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-2);
	}

	.notify-picker__toggle {
		cursor: pointer;
		color: var(--text-3);
		list-style: none;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		font-size: 10px;
		font-weight: 600;
	}

	.notify-picker__toggle::-webkit-details-marker {
		display: none;
	}

	.notify-picker__toggle:hover {
		color: var(--text-2);
	}

	.notify-picker__blurb {
		margin: 4px 0 0;
		text-transform: none;
		letter-spacing: normal;
		font-size: 11px;
		font-weight: 400;
		color: var(--text-3);
	}

	.notify-picker__list {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		list-style: none;
		margin: 6px 0 0;
		padding: 0;
	}

	.notify-chip {
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
		color: var(--text-1);
		font-size: 11px;
	}

	.notify-chip button {
		display: inline-flex;
		border: none;
		background: transparent;
		color: var(--text-3);
		cursor: pointer;
	}

	.notify-chip button:hover {
		color: var(--status-danger-fg);
	}

	.notify-chip button:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 1px;
	}

	.notify-picker__row {
		display: flex;
		gap: 6px;
		margin-top: 6px;
	}

	.notify-picker__input {
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

	.notify-picker__add {
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

	.notify-picker__add:hover {
		color: var(--text-strong);
		border-color: var(--interactive-active-border);
	}

	.notify-picker__add:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.new-thread__notice {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--status-info-fg);
	}

	.new-thread__error {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--status-danger-fg);
	}

	.new-thread__actions {
		display: flex;
		gap: 8px;
	}

	.new-thread__cancel {
		padding: 5px 14px;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		background: var(--surface-2);
		color: var(--text-2);
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		cursor: pointer;
	}

	.new-thread__cancel:hover {
		color: var(--text-strong);
		border-color: var(--interactive-active-border);
	}

	.new-thread__cancel:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.new-thread__submit {
		--edge: var(--edge-sm);
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
		letter-spacing: var(--track-machine, 0.08em);
		cursor: pointer;
	}

	.new-thread__submit:hover:not(:disabled) {
		background: var(--status-steering-border);
	}

	.new-thread__submit:disabled {
		opacity: 0.55;
		cursor: not-allowed;
	}
</style>
