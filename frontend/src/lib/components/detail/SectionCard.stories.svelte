<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within, userEvent, waitFor } from 'storybook/test';
	import 'augmented-ui/augmented-ui.min.css';
	import SectionCard from './SectionCard.svelte';
	import { createCollapseStore } from '$lib/stores/collapseState.svelte';
	import SectionCardHarness from '$lib/components/__harness__/SectionCardHarness.svelte';

	const collapseStore = createCollapseStore();

	const { Story } = defineMeta({
		title: 'Components/Detail/SectionCard',
		component: SectionCard,
		tags: ['autodocs']
	});
</script>

<Story
	name="Collapsible_Expanded"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const toggle = canvas.getByRole('button', { name: /diagnostics/i });
		expect(toggle).toBeInTheDocument();
		const defaultSection = toggle.closest('section');
		expect(defaultSection?.getAttribute('data-augmented-ui')).toBe('tl-clip br-clip border');
		expect(defaultSection?.closest('[data-elevation]')?.getAttribute('data-elevation')).toBe(
			'card'
		);
		expect(canvas.getByText('Expanded section content is visible here.')).toBeInTheDocument();

		expect(toggle).toHaveAttribute('aria-expanded', 'true');
		await userEvent.click(toggle);
		expect(toggle).toHaveAttribute('aria-expanded', 'false');
		await waitFor(() =>
			expect(
				canvas.queryByText('Expanded section content is visible here.')
			).not.toBeInTheDocument()
		);
		await userEvent.click(toggle);
		expect(toggle).toHaveAttribute('aria-expanded', 'true');
		expect(canvas.getByText('Expanded section content is visible here.')).toBeInTheDocument();

		const groupedToggle = canvas.getByRole('button', { name: /grouped section/i });
		const groupedSection = groupedToggle.closest('section');
		expect(groupedSection?.getAttribute('data-augmented-ui')).toBeNull();
		expect(groupedSection?.closest('[data-elevation]')?.getAttribute('data-elevation')).toBe(
			'flat'
		);
		expect(canvas.getByText('Grouped content, no frame of its own.')).toBeInTheDocument();

		expect(canvas.queryByRole('button', { name: /locked section/i })).toBeNull();
		expect(canvas.getByText('Locked section')).toBeInTheDocument();
		expect(canvas.queryByText('This body never renders.')).toBeNull();
		expect(
			canvas.getByText('This description stays visible even when locked.')
		).toBeInTheDocument();

		const tooltipToggle = canvas.getByRole('button', { name: /explained section/i });
		const tooltipRow = tooltipToggle.parentElement;
		const infoIcon = tooltipRow?.querySelector('.dsc-title-help') ?? null;
		expect(infoIcon).toBeInTheDocument();
		expect(infoIcon && tooltipToggle.contains(infoIcon)).toBe(false);

		const accentToggle = canvas.getByRole('button', { name: /accented section/i });
		expect(accentToggle.closest('section')).toHaveAttribute('data-accent', 'teal');
		const accentOffSection = canvas.getByText('Accented, locked').closest('section');
		expect(accentOffSection).not.toHaveAttribute('data-accent');
	}}
>
	{#snippet template()}
		<div style="display: flex; flex-direction: column; gap: 16px;">
			<SectionCardHarness
				{collapseStore}
				title="Diagnostics"
				collapsible
				sectionKey="story-diag-expanded"
			>
				{#snippet children()}
					<p class="text-sm text-[var(--text-2)]">Expanded section content is visible here.</p>
				{/snippet}
				{#snippet headerActions()}
					<button
						type="button"
						class="rounded px-2 py-0.5 text-label uppercase tracking-wider text-[var(--text-3)] hover:text-[var(--text-2)]"
						aria-label="Copy section"
					>
						Copy
					</button>
				{/snippet}
			</SectionCardHarness>
			<SectionCardHarness
				{collapseStore}
				title="Grouped section"
				collapsible
				bordered={false}
				sectionKey="story-grouped-expanded"
			>
				{#snippet children()}
					<p class="text-sm text-[var(--text-2)]">Grouped content, no frame of its own.</p>
				{/snippet}
			</SectionCardHarness>
			<SectionCardHarness
				{collapseStore}
				title="Locked section"
				expandable={false}
				description="This description stays visible even when locked."
			>
				{#snippet children()}
					<p class="text-sm text-[var(--text-2)]">This body never renders.</p>
				{/snippet}
			</SectionCardHarness>
			<SectionCardHarness
				{collapseStore}
				title="Explained section"
				collapsible
				sectionKey="story-tooltip-expanded"
				titleTooltip="This is the sibling info-icon tooltip pattern six panels hand-rolled before composing SectionCard."
			>
				{#snippet children()}
					<p class="text-sm text-[var(--text-2)]">Body content for the tooltip example.</p>
				{/snippet}
			</SectionCardHarness>
			<SectionCardHarness
				{collapseStore}
				title="Accented section"
				collapsible
				sectionKey="story-accent-expanded"
				accent="teal"
			>
				{#snippet children()}
					<p class="text-sm text-[var(--text-2)]">Teal edge accent, from WorkerCard.</p>
				{/snippet}
			</SectionCardHarness>
			<SectionCardHarness
				{collapseStore}
				title="Accented, locked"
				accent="danger"
				expandable={false}
				description="Locked sections never show an accent colour — never alarming, per WorkerCard's own convention."
			>
				{#snippet children()}
					<p class="text-sm text-[var(--text-2)]">This body never renders.</p>
				{/snippet}
			</SectionCardHarness>
		</div>
	{/snippet}
</Story>
