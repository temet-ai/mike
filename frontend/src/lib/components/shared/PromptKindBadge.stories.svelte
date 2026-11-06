<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import PromptKindBadge from './PromptKindBadge.svelte';

	const { Story } = defineMeta({
		title: 'Shared/PromptKindBadge',
		component: PromptKindBadge,
		tags: ['autodocs']
	});
</script>

<Story
	name="PromptKindBadge"
	play={async ({ canvasElement }) => {
		const operator = canvasElement.querySelector('[data-prompt-kind="operator"]');
		await expect(operator, 'operator badge should be in DOM').toBeTruthy();
		await expect(operator?.textContent?.trim()).toBe('OPERATOR');

		const teammate = canvasElement.querySelector('[data-prompt-kind="teammate-message"]');
		await expect(teammate, 'teammate-message badge should be in DOM').toBeTruthy();
		await expect(teammate?.textContent?.trim()).toBe('TEAMMATE MSG');

		const activity = canvasElement.querySelector('[data-prompt-kind="activity-label"]');
		await expect(activity, 'activity-label badge should be in DOM').toBeTruthy();

		const hook = canvasElement.querySelector('[data-prompt-kind="hook-feedback"]');
		await expect(hook, 'hook-feedback badge should be in DOM').toBeTruthy();

		const loop = canvasElement.querySelector('[data-prompt-kind="loop-wakeup"]');
		await expect(loop, 'loop-wakeup badge should be in DOM').toBeTruthy();
		await expect(loop?.textContent?.trim()).toBe('LOOP WAKEUP');
		await expect(loop?.className).toContain('badge-muted');

		const lineage = canvasElement.querySelector('[data-prompt-kind="agent-lineage"]');
		await expect(lineage?.textContent?.trim()).toBe('AGENT LINEAGE');

		const unclassifiedSlot = canvasElement.querySelector('[data-slot="unclassified"]');
		await expect(unclassifiedSlot?.querySelector('[data-prompt-kind]')).toBeNull();

		const unrecognized = canvasElement.querySelector('[data-prompt-kind="future-kind"]');
		await expect(unrecognized, 'unrecognized-kind badge should be in DOM').toBeTruthy();
		await expect(unrecognized?.textContent?.trim()).toBe('future-kind');
		await expect(unrecognized?.className).toContain('badge-muted');
		await expect(unrecognized?.className).not.toContain('badge-active');
	}}
>
	{#snippet template()}
		<div class="flex flex-col gap-3 bg-[var(--surface-bg)] p-4">
			<p class="font-mono text-micro uppercase tracking-wider text-[var(--text-3)]">
				Glance-row context (TurnCard.svelte) — operator prominent, harness kinds muted
			</p>
			<div
				class="flex flex-wrap items-center gap-2 rounded border border-[var(--border-subtle)] bg-[var(--surface-card)] px-3 py-2"
			>
				<span class="font-mono text-micro text-[var(--text-3)]">#41</span>
				<PromptKindBadge kind="operator" />
				<PromptKindBadge kind="activity-label" />
				<PromptKindBadge kind="hook-feedback" />
				<PromptKindBadge kind="teammate-message" />
				<PromptKindBadge kind="loop-wakeup" />
				<PromptKindBadge kind="agent-lineage" />
				<span data-slot="unclassified" class="inline-flex items-center gap-1">
					<span class="font-mono text-micro text-[var(--text-faint)]">unclassified:</span>
					<PromptKindBadge kind={null} />
				</span>
				<span data-slot="unrecognized" class="inline-flex items-center gap-1">
					<span class="font-mono text-micro text-[var(--text-faint)]">unrecognized:</span>
					<PromptKindBadge kind="future-kind" />
				</span>
			</div>
		</div>
	{/snippet}
</Story>
