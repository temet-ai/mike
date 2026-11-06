<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, waitFor, expect } from 'storybook/test';
	import SystemPromptCard from './SystemPromptCard.svelte';
	import type { SystemBlockInput } from './systemBlockKinds';

	const BILLING_LINE =
		'x-anthropic-billing-header: cc_version=2.0.14; cc_entrypoint=cli; cch=af3b91c2;';
	const IDENTITY_LINE = "You are Claude Code, Anthropic's official CLI for Claude.";

	const INSTRUCTIONS_HEAD = 'INSTRUCTIONS-HEAD-SENTINEL';
	const INSTRUCTIONS_TAIL = 'INSTRUCTIONS-TAIL-SENTINEL';
	const INSTRUCTIONS_BODY = [
		INSTRUCTIONS_HEAD,
		...Array.from(
			{ length: 220 },
			(_, i) =>
				`Section ${i + 1}: You have access to a set of tools you can use to answer questions and perform tasks.`
		),
		INSTRUCTIONS_TAIL
	].join('\n');

	const ALTERED_ORIGINAL_HEAD = 'ALTERED-ORIGINAL-HEAD-SENTINEL';
	const ALTERED_ORIGINAL_TAIL = 'ALTERED-ORIGINAL-TAIL-SENTINEL';
	const ALTERED_BLOCK_ORIGINAL = [
		ALTERED_ORIGINAL_HEAD,
		'Do not call the AgentTool unless the user requested it',
		'Do not use workflows or deep-research unless the user requested it',
		...Array.from(
			{ length: 8 },
			(_, i) => `Filler guidance line ${i + 1} padding this block past the instructions threshold.`
		),
		ALTERED_ORIGINAL_TAIL
	].join('\n');
	const ALTERED_SENT_HEAD = 'ALTERED-SENT-HEAD-SENTINEL';
	const ALTERED_SENT_TAIL = 'ALTERED-SENT-TAIL-SENTINEL';
	const ALTERED_BLOCK_SENT = [
		ALTERED_SENT_HEAD,
		'Dispatch specialist agents when the work matches one — check available agents before doing broad work yourself.',
		'Use workflows or deep-research proactively when the task shape matches their purpose.',
		...Array.from(
			{ length: 8 },
			(_, i) => `Filler guidance line ${i + 1} padding this block past the instructions threshold.`
		),
		ALTERED_SENT_TAIL
	].join('\n');

	const SYSTEM_RAW: unknown[] = [
		{ text: BILLING_LINE, type: 'text' },
		{ text: IDENTITY_LINE, type: 'text' },
		{
			cache_control: { scope: 'global', ttl: '1h', type: 'ephemeral' },
			text: INSTRUCTIONS_BODY,
			type: 'text'
		},
		{ text: ALTERED_BLOCK_ORIGINAL, type: 'text' }
	];

	const MAIN_LOOP_BLOCKS: SystemBlockInput[] = SYSTEM_RAW.map((raw, index) => ({
		text: (raw as { text: string }).text,
		index,
		raw
	}));

	const SENT_BLOCKS: string[] = [
		BILLING_LINE,
		IDENTITY_LINE,
		INSTRUCTIONS_BODY,
		ALTERED_BLOCK_SENT
	];

	const { Story } = defineMeta({
		title: 'Components/Shell/SystemPromptCard',
		component: SystemPromptCard,
		tags: ['autodocs'],
		parameters: { layout: 'padded' }
	});

	function findJsonPre(): HTMLElement | undefined {
		return [...document.body.querySelectorAll('pre')].find((e) =>
			e.textContent?.includes('cache_control')
		) as HTMLElement | undefined;
	}
</script>

<Story
	name="InstructionsExpanded"
	play={async ({ canvasElement }) => {
		const instructionsRow = await waitFor(
			() => {
				const btn = canvasElement.querySelector<HTMLButtonElement>(
					'button[aria-label*="instructions"]'
				);
				if (!btn) throw new Error('instructions button not yet rendered');
				return btn;
			},
			{ timeout: 5000 }
		);
		await userEvent.click(instructionsRow);
		const pre = await waitFor(() => {
			const els = canvasElement.querySelectorAll('pre');
			const el = [...els].find((e) => e.textContent?.includes(INSTRUCTIONS_HEAD));
			if (!el) throw new Error('instructions pre not yet rendered');
			return el;
		});
		await expect(pre.textContent).toContain(INSTRUCTIONS_HEAD);
		await expect(pre.textContent).toContain(INSTRUCTIONS_TAIL);
		await expect(pre.textContent?.length ?? 0).toBeGreaterThanOrEqual(INSTRUCTIONS_BODY.length);

		const topJsonBtn = canvasElement.querySelector<HTMLButtonElement>(
			'[data-slot="system-prompt-card-top-json"]'
		);
		if (!topJsonBtn) throw new Error('top-level raw-json button not rendered');
		await userEvent.click(topJsonBtn);
		const dialog = await waitFor(() => {
			const d = document.body.querySelector('[data-slot="dialog-content"]');
			if (!d) throw new Error('modal not open');
			return d;
		});
		await waitFor(() => {
			if (!dialog.textContent?.includes(IDENTITY_LINE)) {
				throw new Error('rendered view not showing block text');
			}
		});
		const rawTab = [...dialog.querySelectorAll('[role="tab"]')].find(
			(b) => b.textContent?.trim() === 'Raw JSON'
		) as HTMLButtonElement | undefined;
		if (!rawTab) throw new Error('Raw JSON tab not rendered');
		await userEvent.click(rawTab);
		const topPre = await waitFor(() => {
			const el = findJsonPre();
			if (!el) throw new Error('raw json not rendered after tab switch');
			return el;
		});
		await expect(topPre.textContent).toContain('"cache_control"');
		await expect(topPre.textContent).toContain('"scope": "global"');
		await expect(topPre.textContent).toContain('"type": "text"');

		const closeBtn = document.body.querySelector<HTMLButtonElement>('[data-slot="dialog-close"]');
		if (!closeBtn) throw new Error('dialog close button not found');
		await userEvent.click(closeBtn);
		await waitFor(() => {
			if (document.body.querySelector('[data-slot="dialog-content"]')) {
				throw new Error('modal still open after close');
			}
		});
		const blockJsonBtns = canvasElement.querySelectorAll<HTMLButtonElement>(
			'[data-slot="system-prompt-card-block-json"]'
		);
		const instructionsJsonBtn = blockJsonBtns[2];
		if (!instructionsJsonBtn) throw new Error('per-block raw-json buttons not rendered');
		instructionsJsonBtn.click();
		const blockDialog = await waitFor(() => {
			const d = document.body.querySelector('[data-slot="dialog-content"]');
			if (!d?.textContent?.includes(INSTRUCTIONS_HEAD)) {
				throw new Error('per-block rendered view not showing block text');
			}
			return d;
		});
		void blockDialog;

		const blockDialogCloseBtn = document.body.querySelector<HTMLButtonElement>(
			'[data-slot="dialog-close"]'
		);
		if (!blockDialogCloseBtn) throw new Error('block dialog close button not found');
		await userEvent.click(blockDialogCloseBtn);
		await waitFor(() => {
			if (document.body.querySelector('[data-slot="dialog-content"]')) {
				throw new Error('block modal still open after close');
			}
			if (document.body.style.pointerEvents === 'none') {
				throw new Error('body pointer-events still locked after close');
			}
		});

		const headerBadge = canvasElement.querySelector(
			'[data-slot="system-prompt-card-altered-summary"]'
		);
		if (!headerBadge) throw new Error('header altered-summary badge not rendered');
		await expect(headerBadge.textContent).toContain('1 altered');

		const rowBadges = canvasElement.querySelectorAll(
			'[data-slot="system-prompt-card-block-altered"]'
		);
		await expect(rowBadges.length).toBe(1);

		const instructionsButtons = canvasElement.querySelectorAll<HTMLButtonElement>(
			'button[aria-label*="instructions"]'
		);
		await expect(instructionsButtons.length).toBe(2);
		await expect(
			instructionsButtons[0]?.parentElement?.querySelector(
				'[data-slot="system-prompt-card-block-altered"]'
			) ?? null
		).toBeNull();

		const alteredRow = instructionsButtons[1];
		if (!alteredRow) throw new Error('altered instructions row not rendered');
		await userEvent.click(alteredRow);
		await waitFor(() => {
			if (
				!canvasElement.textContent?.includes(ALTERED_ORIGINAL_HEAD) ||
				!canvasElement.textContent?.includes(ALTERED_SENT_HEAD)
			) {
				throw new Error('altered-block diff not showing both original and sent text');
			}
		});
		await expect(canvasElement.textContent).toContain(ALTERED_ORIGINAL_TAIL);
		await expect(canvasElement.textContent).toContain(ALTERED_SENT_TAIL);
	}}
>
	{#snippet template()}
		<div style="max-width: 560px;">
			<SystemPromptCard
				blocks={MAIN_LOOP_BLOCKS}
				systemRaw={SYSTEM_RAW}
				sessionId="story-session-5"
				sentBlocks={SENT_BLOCKS}
			/>
		</div>
	{/snippet}
</Story>
