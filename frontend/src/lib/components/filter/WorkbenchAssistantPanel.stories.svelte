<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import { setContext } from 'svelte';
	import WorkbenchAssistantPanel from './WorkbenchAssistantPanel.svelte';
	import { FILTER_RULES_STORE_KEY } from '$lib/context-keys';
	import { createMockFilterRulesStore } from '$lib/storybook/mockFilterRulesStore.svelte';
	import type { FilterRuleDto } from '$lib/types';

	// Seeded so the FIRST proposal below (edit_id: "1") matches an EXISTING rule that already
	// has an operator-saved title — the card must show THAT title, not the model's own
	// `proposal.title` for the reword, which can drift from what the operator named it.
	const EXISTING_RULE: FilterRuleDto = {
		id: '1',
		kind: 'text_substring',
		value: 'Old text',
		action: 'replace',
		text: 'Current text',
		title: 'Removes the pronoun guidance',
		enabled: true,
		source: 'ui',
		updated_at: '2026-04-17T12:00:00Z',
		sort_order: 1,
		trigger_token: null,
		fires_24h: 0,
		fires_7d: 0,
		last_fired_at: null
	};

	const HEALTH = {
		ok: true,
		enabled: true,
		model: 'claude-sonnet-5',
		effort: 'medium',
		may_apply: 'suggest',
		spent_today_usd: 0.03,
		daily_budget_usd: 1,
		auth_mode: 'claude_code_login',
		credentials_available: true
	};

	/** A real ReadableStream carrying hand-written SSE bytes — no module mocking, just fetch. */
	function sseResponse(frames: string): Response {
		const encoder = new TextEncoder();
		const body = new ReadableStream<Uint8Array>({
			start(controller) {
				controller.enqueue(encoder.encode(frames));
				controller.close();
			}
		});
		return new Response(body, { status: 200, headers: { 'content-type': 'text/event-stream' } });
	}

	// One assistant turn that calls propose_edit TWICE — proves the panel renders
	// `message.proposals` (plural) as multiple cards, matching the real service's
	// `ConversationMessage.proposals?: Proposal[]` shape.
	//
	// p2's `after` is deliberately the SERVICE'S OWN buggy `match + text` concatenation for
	// append (`mcp/proposals.ts`'s real shape, live on request 421514) — display-only. Its
	// `rule_text` is the raw tail alone: Apply must save THAT, never `after`, or the created
	// rule duplicates the anchor (G3).
	const TURN_SSE = [
		'event: status\ndata: {"phase":"reading prompt"}\n\n',
		'event: text\ndata: {"delta":"Here are two rewords."}\n\n',
		// The reply is markdown, streamed in deltas that split mid-fence and mid-bold: the
		// panel re-renders per delta and must end on the fully formatted reply.
		'event: text\ndata: {"delta":"\\n\\n**Why:** the `security note` repe"}\n\n',
		'event: text\ndata: {"delta":"ats your CLAUDE.md.\\n\\n```text\\nkeep secrets out of"}\n\n',
		'event: text\ndata: {"delta":" logs\\n```\\n\\nSee [the docs](https://docs.anthropic.com/en/docs)."}\n\n',
		'event: proposal\ndata: {"id":"p1","title":"Rewords the security note","edit_id":"1","action":"replace","match":"Old text","before":"Old text","after":"New text","reach":"Every main session prompt","rule_text":"New text"}\n\n',
		'event: proposal\ndata: {"id":"p2","title":"Adds a clickable-path note","edit_id":null,"action":"append","match":"absolute_file_path","before":"","after":"absolute_file_path — clickable in most terminals.","reach":"Wherever the anchor appears","rule_text":"— clickable in most terminals."}\n\n',
		'event: done\ndata: {"cost_usd":0.012}\n\n'
	].join('');

	/** What a real GET .../conversations/:id would answer once the turn above finished — used
	 *  to prove the panel REHYDRATES from `localStorage` + this endpoint after being unmounted
	 *  and remounted (Read only toggling FilterWorkbench's `{#if sideOpen}`, G6). */
	const PERSISTED_CONVERSATION = {
		id: 'conv-1',
		request_id: 42,
		created_at: '2026-09-23T10:00:00Z',
		updated_at: '2026-09-23T10:00:05Z',
		messages: [
			{
				id: 'm1',
				role: 'user',
				text: 'Anything here that fights my CLAUDE.md?',
				created_at: '2026-09-23T10:00:00Z'
			},
			{
				id: 'm2',
				role: 'assistant',
				text: 'Here are two rewords.',
				proposals: [
					{
						id: 'p1',
						title: 'Rewords the security note',
						edit_id: '1',
						action: 'replace',
						match: 'Old text',
						before: 'Old text',
						after: 'New text',
						reach: 'Every main session prompt',
						rule_text: 'New text'
					}
				],
				created_at: '2026-09-23T10:00:05Z'
			}
		]
	};

	let healthEnabled = true;
	/** The `scope`/`project_path` fields of the most recent POST .../messages body — asserted by
	 *  the play() below to prove the WORKBENCH-driven `workbenchScope`/`workbenchProjectPath`
	 *  props (the "Check against" control moved up to the workbench's own Project picker) are
	 *  what actually goes out on the wire, not just displayed. */
	let lastSentScope: string | undefined;
	let lastSentProjectPath: string | undefined;

	function stubFetch(): void {
		globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (url.includes('/api/assistant/health')) {
				return new Response(JSON.stringify({ ...HEALTH, enabled: healthEnabled }), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.includes(`/api/assistant/conversations/conv-1`) && (init?.method ?? 'GET') === 'GET') {
				return new Response(JSON.stringify(PERSISTED_CONVERSATION), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.includes('/api/assistant/conversations') && init?.method === 'POST' && !url.includes('/messages')) {
				return new Response(JSON.stringify({ id: 'conv-1' }), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.includes('/messages')) {
				const body = init?.body
					? (JSON.parse(init.body as string) as { scope?: string; project_path?: string })
					: {};
				lastSentScope = body.scope;
				lastSentProjectPath = body.project_path;
				return sseResponse(TURN_SSE);
			}
			return new Response('Not Found', { status: 404, statusText: url });
		};
	}

	const { Story } = defineMeta({
		title: 'Routes/Filter/WorkbenchAssistantPanel',
		component: WorkbenchAssistantPanel,
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	import type { AssistantDraftHint } from './WorkbenchAssistantPanel.svelte';

	let quote = $state('');
	let draftHint = $state<AssistantDraftHint | null>(null);
	let previewingProposalId = $state<string | null>(null);
	let appliedRuleId = $state<string | null>(null);
	let mounted = $state(true);

	const store = createMockFilterRulesStore([EXISTING_RULE]);
	// svelte-ignore state_referenced_locally -- one-time context registration for the story
	setContext(FILTER_RULES_STORE_KEY, store);
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			stubFetch();
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(await canvas.findByText('Prompt assistant')).toBeInTheDocument();
		await expect(await canvas.findByText(/claude-sonnet-5 · medium · suggests only/)).toBeInTheDocument();
		await expect(
			await canvas.findByRole('button', { name: 'Does this fit my other projects?' })
		).toBeInTheDocument();
		// No "Check against" control of its own — that choice now lives entirely in the
		// workbench's own Project picker, one level up.
		await expect(canvas.queryByRole('radiogroup', { name: 'Check against' })).toBeNull();

		const textarea = await canvas.findByPlaceholderText(
			'Ask to reword, shorten, or check a part of the prompt…'
		);
		await userEvent.type(textarea, 'Anything here that fights my CLAUDE.md?');
		const sendBtn = await canvas.findByRole('button', { name: 'Send' });
		await userEvent.click(sendBtn);
		// This story's workbenchScope/workbenchProjectPath props (below) are what drive the
		// wire body — proves the workbench-level pick, not a control inside this panel.
		await waitFor(() => expect(lastSentScope).toBe('project'));
		await waitFor(() => expect(lastSentProjectPath).toBe('/mnt/data/projects/other'));

		// The streamed reply renders as markdown, not as raw asterisks and backticks; the user's
		// own message stays plain text.
		await expect(await canvas.findByText('Why:', { selector: 'strong' })).toBeInTheDocument();
		await expect(await canvas.findByText('security note', { selector: 'code' })).toBeInTheDocument();
		await expect(await canvas.findByText('keep secrets out of logs', { selector: 'pre' })).toBeInTheDocument();
		const docsLink = await canvas.findByRole('link', { name: 'the docs' });
		await expect(docsLink).toHaveAttribute('href', 'https://docs.anthropic.com/en/docs');
		await expect(canvas.queryByText(/\*\*Why/)).toBeNull();
		for (const el of canvas.getAllByText('Anything here that fights my CLAUDE.md?')) {
			await expect(el.closest('[data-slot="markdown-text"]')).toBeNull();
		}

		// Neither proposal below carries a project_path — no "This project only" badge shows.
		await expect(canvas.queryByText('This project only')).toBeNull();

		// Two proposal cards from ONE assistant turn — plural `proposals`, not a single `proposal`.
		// p1 (edit_id: "1") matches EXISTING_RULE: shows the operator's SAVED title, never the
		// model's own "Rewords the security note" for that same reword. p2 is a brand-new rule
		// (edit_id: null) with no existing title to prefer, so its own title shows as-is.
		await expect(canvas.queryByText('Rewords the security note')).toBeNull();
		await expect(await canvas.findByText('Removes the pronoun guidance')).toBeInTheDocument();
		await expect(
			await canvas.findByText('Adds a clickable-path note')
		).toBeInTheDocument();
		await expect(await canvas.findByText('New text')).toBeInTheDocument();
		await expect(
			await canvas.findByText('absolute_file_path — clickable in most terminals.')
		).toBeInTheDocument();

		const previewButtons = canvas.getAllByRole('button', { name: 'Preview in prompt' });
		await userEvent.click(previewButtons[0]!);
		await expect(previewingProposalId).toBe('p1');

		// Apply p1 (a REWORD of EXISTING_RULE): the operator's saved title survives — the panel
		// must never send the model's own `proposal.title` back as the rule's title (G5).
		const applyButtons = canvas.getAllByRole('button', { name: 'Apply' });
		await userEvent.click(applyButtons[0]!);
		await waitFor(() => expect(appliedRuleId).toBe('1'));
		await expect(store.rules.find((r) => r.id === '1')?.title).toBe('Removes the pronoun guidance');
		await expect(store.rules.find((r) => r.id === '1')?.text).toBe('New text');

		// Apply p2 (a brand-new append proposal): the created rule's `text` is the raw tail from
		// `rule_text`, never the service's own `match + text` display concatenation in `after` —
		// applying the OLD code's `proposal.after` here would duplicate the anchor (G3).
		await userEvent.click(applyButtons[1]!);
		await waitFor(() => expect(appliedRuleId).not.toBe('1'));
		const createdId = appliedRuleId;
		const created = store.rules.find((r) => r.id === createdId);
		await expect(created?.text).toBe('— clickable in most terminals.');
		await expect(created?.text).not.toContain('absolute_file_path');
		// The NEW rule takes the workbench's picked project — never the proposal's own
		// (unset here) `project_path` — spec: "assistant Apply use the picked project's path".
		await expect(created?.project_path).toBe('/mnt/data/projects/other');
		await expect(await canvas.findAllByText('applied · live on the next request')).toHaveLength(2);

		// Toggling "Read only" off and back on in FilterWorkbench unmounts and remounts this
		// whole panel — the conversation must survive that via `localStorage` + `GET
		// .../conversations/:id`, not just in-memory state (G6).
		const toggleMountBtn = await canvas.findByRole('button', { name: 'Toggle mount' });
		await userEvent.click(toggleMountBtn);
		await waitFor(() => expect(canvas.queryByText('Prompt assistant')).toBeNull());
		await userEvent.click(toggleMountBtn);
		await expect(await canvas.findByText('Prompt assistant')).toBeInTheDocument();
		await expect(await canvas.findByText('Here are two rewords.')).toBeInTheDocument();
		await expect(await canvas.findByText('Removes the pronoun guidance')).toBeInTheDocument();

		// A remount re-checks health on mount, and every `send()` re-checks it again right
		// before sending (G7) — the panel doesn't need a page reload or the 30s poll tick to
		// notice the assistant went from enabled to disabled while it sat open.
		healthEnabled = false;
		const textareaAfterOff = await canvas.findByPlaceholderText(
			'Ask to reword, shorten, or check a part of the prompt…'
		);
		await userEvent.type(textareaAfterOff, 'one more question');
		await userEvent.click(await canvas.findByRole('button', { name: 'Send' }));
		await expect(await canvas.findByText(/Prompt assistant is turned off/)).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="h-[560px] w-[420px] bg-[var(--surface-bg)] flex flex-col gap-2">
			<button type="button" onclick={() => (mounted = !mounted)}>Toggle mount</button>
			{#if mounted}
				<WorkbenchAssistantPanel
					requestId={42}
					{quote}
					onClearQuote={() => (quote = '')}
					{draftHint}
					onDraftHintConsumed={() => (draftHint = null)}
					{previewingProposalId}
					onPreview={(p) => (previewingProposalId = p.id)}
					onStopPreview={() => (previewingProposalId = null)}
					onApplied={(ruleId) => (appliedRuleId = ruleId)}
					onToggleCollapse={() => {}}
					workbenchScope="project"
					workbenchProjectPath="/mnt/data/projects/other"
				/>
			{/if}
		</div>
	{/snippet}
</Story>
