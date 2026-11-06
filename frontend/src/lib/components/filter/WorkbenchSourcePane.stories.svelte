<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import WorkbenchSourcePane, { type PromptLens } from './WorkbenchSourcePane.svelte';
	import type { FilterRuleDto, SystemBlock } from '$lib/types';

	function sysBlock(index: number, text: string): SystemBlock {
		return { index, text, cache_control: null, composition_match: null };
	}

	// A multi-paragraph block (heading + list + prose), the shape the coordinator flagged as
	// too coarse at block granularity: one block, several separately-edited paragraphs.
	const BLOCK_0 = [
		"You are Claude Code, Anthropic's official CLI for Claude.",
		'',
		'# Session-specific guidance',
		' - Refuse destructive requests without asking first.',
		' - Reference code as absolute_file_path:line_number.'
	].join('\n');

	const SOURCE_BLOCKS: SystemBlock[] = [
		sysBlock(0, BLOCK_0),
		sysBlock(1, '<system-reminder>\nPast sessions are memory too.\n</system-reminder>')
	];

	function buildRule(overrides: Partial<FilterRuleDto> = {}): FilterRuleDto {
		return {
			id: 'rule-src-1',
			kind: 'text_substring',
			value: 'Refuse destructive requests without asking first.',
			action: 'replace',
			text: 'Confirm proceeding through a quick approval step.',
			title: null,
			enabled: true,
			source: 'ui',
			updated_at: '2026-04-17T12:00:00Z',
			sort_order: 1,
			trigger_token: null,
			fires_24h: 3,
			fires_7d: 20,
			last_fired_at: '2026-09-20T09:00:00Z',
			...overrides
		};
	}

	const RULES: FilterRuleDto[] = [
		buildRule(),
		buildRule({
			id: 'rule-src-2',
			value: 'absolute_file_path:line_number',
			action: 'append',
			text: ' — clickable in most terminals.',
			title: 'Notes the path format is clickable'
		})
	];

	const RESULT_BLOCKS = [
		[
			"You are Claude Code, Anthropic's official CLI for Claude.",
			'',
			'# Session-specific guidance',
			' - Confirm proceeding through a quick approval step.',
			' - Reference code as absolute_file_path:line_number. — clickable in most terminals.'
		].join('\n'),
		'<system-reminder>\nPast sessions are memory too.\n</system-reminder>'
	];

	const FIRING_RULE_IDS = new Set(['rule-src-1', 'rule-src-2']);

	// G9: a rule can land in the SESSION MESSAGE (the last user message), not just a system
	// block — it gets its own diffed section, at the end, with the same margin-title treatment.
	// Deliberately shares no vocabulary with RULES' anchor text — an overlapping session
	// message would let a system-block rule ALSO attribute here, duplicating its margin button
	// (an earlier draft of this fixture hit exactly that: "Refuse destructive requests without
	// asking first." inside the session message duplicated rule-src-1's margin button).
	const SESSION_MESSAGE = 'Can you check the flaky checkout test before I merge this?';
	const RESULT_SESSION_MESSAGE =
		'Can you check the flaky checkout test before I merge this? Run it three times to confirm.';

	const { Story } = defineMeta({
		title: 'Routes/Filter/WorkbenchSourcePane',
		component: WorkbenchSourcePane,
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	let lens = $state<PromptLens>('changes');
	let lastAsk = $state<string | null>(null);
	let lastOpenEdit = $state<string | null>(null);
	let scrollToParagraph = $state<string | null>(null);
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		// Two paragraphs in the SAME block, each edited by a DIFFERENT rule: the heading
		// paragraph stays untouched, the list paragraph carries BOTH edits — margin titles and
		// "ask about this" attach per paragraph, not to the whole 5-line block.
		await expect(await canvas.findByText('# Session-specific guidance')).toBeInTheDocument();
		// Regex, not an exact string: these two list-item sentences share no real vocabulary
		// (only the incidental "-" list marker), so the round-4 similarity gate correctly keeps
		// them as two WHOLE sentences — the list marker stays attached to each rather than being
		// hoisted out as a shared "unchanged" word-level anchor (the old, buggy anchoring this
		// gate replaced).
		await expect(
			await canvas.findByText(/Confirm proceeding through a quick approval step\.$/)
		).toBeInTheDocument();
		await expect(
			await canvas.findByText(/Refuse destructive requests without asking first\.$/)
		).toBeInTheDocument();

		await expect(
			await canvas.findByRole('button', {
				name: 'Rewrites “Refuse destructive requests without asking first.”'
			})
		).toBeInTheDocument();
		const appendMargin = await canvas.findByRole('button', {
			name: 'Notes the path format is clickable'
		});
		await userEvent.click(appendMargin);
		await expect(lastOpenEdit).toBe('rule-src-2');

		// Every paragraph gets its OWN "Ask about this" — clicking one quotes only that paragraph,
		// not the whole 5-line block. Document order: intro sentence, heading, list run, block 1.
		const askButtons = canvas.getAllByRole('button', { name: 'Ask about this' });
		expect(askButtons.length).toBeGreaterThanOrEqual(4);
		await userEvent.click(askButtons[0]!);
		await expect(lastAsk).toBe("You are Claude Code, Anthropic's official CLI for Claude.");
		await userEvent.click(askButtons[1]!);
		await expect(lastAsk).toBe('# Session-specific guidance');

		// Switch to "As sent" — plain text, no diff colouring, no margin edit buttons.
		lens = 'source';
		await waitFor(() =>
			expect(canvas.queryByText('Confirm proceeding through a quick approval step.')).toBeNull()
		);
		await expect(
			await canvas.findByText(/Refuse destructive requests without asking first/)
		).toBeInTheDocument();
		await waitFor(() => expect(canvas.queryByRole('button', { name: /Rewrites/ })).toBeNull());

		// "Final" shows exactly what gets sent — no strike-through, no green wash.
		lens = 'result';
		await expect(
			await canvas.findByText(/Confirm proceeding through a quick approval step/)
		).toBeInTheDocument();
		await waitFor(() => expect(canvas.queryByText(/Refuse destructive requests/)).toBeNull());
		lens = 'changes';

		// "Show in prompt" (a paragraph key from outside) scrolls to AND highlights that paragraph.
		scrollToParagraph = '0:1';
		await waitFor(() => {
			const target = canvasElement.querySelector('[data-para-key="0:1"]');
			expect(target?.className).toContain('wb-para--highlight');
		});

		// G17: block pills read as WORDS (the section heading, or the block's own classification)
		// — never a bare "#1"/"#2" number.
		const pills = Array.from(canvasElement.querySelectorAll('.wb-block-pill')).map(
			(el) => el.textContent
		);
		expect(pills).toContain('Session-specific guidance');
		expect(pills.some((t) => /^#\d+$/.test(t ?? ''))).toBe(false);

		// G9: a rule landing in the SESSION MESSAGE (the last user message, not a system block)
		// gets its own diffed section, at the end — same word-level diff and margin treatment.
		await expect(await canvas.findByText('Session message')).toBeInTheDocument();
		await expect(
			await canvas.findByText(/Run it three times to confirm\.$/)
		).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="h-[460px] w-[820px] bg-[var(--surface-bg)]">
			<WorkbenchSourcePane
				blocks={SOURCE_BLOCKS}
				orderedRules={RULES}
				resultBlocks={RESULT_BLOCKS}
				previewBlocks={null}
				firingRuleIds={FIRING_RULE_IDS}
				sessionMessage={SESSION_MESSAGE}
				resultSessionMessage={RESULT_SESSION_MESSAGE}
				previewSessionMessage={null}
				{lens}
				scrollToBlock={null}
				{scrollToParagraph}
				onAskAbout={(q) => (lastAsk = q)}
				onOpenEdit={(id) => (lastOpenEdit = id)}
			/>
		</div>
	{/snippet}
</Story>
