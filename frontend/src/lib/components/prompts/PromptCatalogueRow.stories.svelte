<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import PromptCatalogueRow from './PromptCatalogueRow.svelte';
	import type { PromptCatalogueRowViewModel } from '$lib/prompts/catalogueRow';

	const rows: PromptCatalogueRowViewModel[] = [
		{
			key: 'subagent:Rust Development Expert',
			lane: 'subagent',
			persona: 'Rust Development Expert',
			displayName: 'Rust Development Expert',
			isHumanizedReason: false,
			state: 'drifted',
			badgeText: 'drifted',
			badgeClass: 'badge-error',
			observationCount: 19,
			lastSeenLabel: '2h ago',
			firingRuleCount: 3,
			actionLabel: 'Review drift',
			href: '/prompts/subagent/Rust%20Development%20Expert?tab=drift',
			sortAnchor: Date.now() - 2 * 60 * 60 * 1000
		},
		{
			key: 'anomaly:non-three-block',
			lane: null,
			persona: null,
			displayName: 'system was a 2-element array (expected exactly 3 blocks)',
			isHumanizedReason: true,
			state: 'needs-investigation',
			badgeText: 'needs investigation',
			badgeClass: 'badge-warning',
			observationCount: 6,
			lastSeenLabel: '3h ago',
			firingRuleCount: 0,
			actionLabel: 'Investigate',
			href: '/sessions/sess-a3f8e2c1/requests/4790',
			sortAnchor: Date.now() - 3 * 60 * 60 * 1000
		},
		{
			key: 'main:',
			lane: 'main',
			persona: null,
			displayName: 'Claude Code (main agent)',
			isHumanizedReason: false,
			state: 'unregistered',
			badgeText: 'unregistered',
			badgeClass: 'badge-warning',
			observationCount: 1416,
			lastSeenLabel: '12m ago',
			firingRuleCount: 0,
			actionLabel: 'Register this',
			href: '/prompts/new',
			sortAnchor: Date.now() - 12 * 60 * 1000
		},
		{
			key: 'subagent:Frontend Development Expert',
			lane: 'subagent',
			persona: 'Frontend Development Expert',
			displayName: 'Frontend Development Expert',
			isHumanizedReason: false,
			state: 'unregistered',
			badgeText: 'unregistered',
			badgeClass: 'badge-warning',
			observationCount: 8,
			lastSeenLabel: '4h ago',
			firingRuleCount: 0,
			actionLabel: 'Register this',
			href: '/prompts/new?persona=Frontend%20Development%20Expert',
			sortAnchor: Date.now() - 4 * 60 * 60 * 1000
		},
		{
			key: 'subagent:file search specialist for Claude Code',
			lane: 'subagent',
			persona: 'file search specialist for Claude Code',
			displayName: 'file search specialist for Claude Code',
			isHumanizedReason: false,
			state: 'unregistered',
			badgeText: 'unregistered',
			badgeClass: 'badge-warning',
			observationCount: 8,
			lastSeenLabel: '6h ago',
			firingRuleCount: 0,
			actionLabel: 'Register this',
			href: '/prompts/new?persona=file%20search%20specialist%20for%20Claude%20Code',
			sortAnchor: Date.now() - 6 * 60 * 60 * 1000
		},
		{
			key: 'subagent:Verification Specialist',
			lane: 'subagent',
			persona: 'Verification Specialist',
			displayName: 'Verification Specialist',
			isHumanizedReason: false,
			state: 'registered',
			badgeText: 'registered',
			badgeClass: 'badge-active',
			observationCount: 7,
			lastSeenLabel: '40m ago',
			firingRuleCount: 0,
			actionLabel: 'Open',
			href: '/prompts/subagent/Verification%20Specialist',
			sortAnchor: Date.now() - 40 * 60 * 1000
		},
		{
			key: 'subagent:steer-author',
			lane: 'subagent',
			persona: 'steer-author',
			displayName: 'steer-author initial prompt (Phase A)',
			isHumanizedReason: false,
			state: 'registered',
			badgeText: 'registered',
			badgeClass: 'badge-active',
			observationCount: 0,
			lastSeenLabel: null,
			firingRuleCount: 0,
			actionLabel: 'Open',
			href: '/prompts/subagent/steer-author',
			sortAnchor: 0
		}
	];

	const { Story } = defineMeta({
		title: 'Routes/Prompts/PromptCatalogueRow',
		tags: ['autodocs']
	});
</script>

<Story
	name="Catalogue"
	play={async ({ canvas }) => {
		const links = await canvas.findAllByRole('link');
		await expect(links).toHaveLength(rows.length);

		for (const row of rows) {
			const link = links.find((el) => el.getAttribute('href') === row.href);
			if (!link) {
				throw new Error(`no row rendered for href ${row.href} (${row.displayName})`);
			}
			await expect(link).toHaveTextContent(row.badgeText);
			await expect(link).toHaveTextContent(row.actionLabel);
		}
	}}
>
	{#snippet template()}
		<div
			class="flex flex-col gap-2 rounded-lg border border-[var(--border-default)] bg-[var(--surface-card)] p-2"
			data-slot="prompt-catalogue-panel"
		>
			{#each rows as row (row.key)}
				<PromptCatalogueRow {row} />
			{/each}
		</div>
	{/snippet}
</Story>
