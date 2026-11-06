<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor } from 'storybook/test';
	import SpendByCallerChart from './SpendByCallerChart.svelte';
	import { buildFixtureByDayCaller, buildFixtureCallers } from './llmSpendFixtures';

	function tooltipContent(): HTMLElement | null {
		return document.body.querySelector('[data-slot="tooltip-content"]') as HTMLElement | null;
	}

	async function openTooltip(trigger: Element): Promise<HTMLElement> {
		await userEvent.hover(trigger);
		return await waitFor(() => {
			const content = tooltipContent();
			if (!content) throw new Error('tooltip content not yet rendered');
			return content;
		});
	}

	const { Story } = defineMeta({
		title: 'LlmSpend/SpendByCallerChart',
		component: SpendByCallerChart,
		tags: ['autodocs'],
		parameters: { layout: 'padded' }
	});
</script>

<Story
	name="Default"
	args={{ callers: buildFixtureCallers(), byDayCaller: buildFixtureByDayCaller() }}
	play={async ({ canvasElement }) => {
		// Legend is grouped by role: a "Steerers" caption precedes the multi-member steerer
		// group, and members show their short name since the caption already says "Steerers".
		const caption = Array.from(canvasElement.querySelectorAll('.sc-legend-caption')).find(
			(el) => el.textContent?.trim() === 'Steerers'
		);
		await expect(caption).toBeTruthy();
		const legendButtons = Array.from(
			canvasElement.querySelectorAll('.legend-filter__entry')
		) as HTMLButtonElement[];
		await expect(legendButtons.some((b) => b.textContent?.includes('verifier'))).toBe(true);
		await expect(legendButtons.some((b) => b.textContent?.includes('Turn judge'))).toBe(true);
		await expect(legendButtons.some((b) => b.textContent?.includes('Steerer · verifier'))).toBe(
			false
		);

		const cols = canvasElement.querySelectorAll('.sc-col');
		await expect(cols.length).toBe(7);
		const lastCol = cols[cols.length - 1] as HTMLElement;

		// The tooltip groups its entries the same way and adds a group subtotal line.
		const firstOpen = await openTooltip(lastCol);
		const groupLabels = Array.from(firstOpen.querySelectorAll('.sc-tip-group-label')).map((el) =>
			el.textContent?.trim()
		);
		await expect(groupLabels).toContain('Steerers');
		await expect(groupLabels).toContain('Turn judge');
		await expect(firstOpen.textContent).toContain('verifier');
		await expect(firstOpen.textContent).not.toContain('Steerer · verifier');

		const verifierToggle = legendButtons.find((b) => b.textContent?.includes('verifier'));
		await expect(verifierToggle).toBeTruthy();
		await userEvent.click(verifierToggle as HTMLButtonElement);
		await waitFor(() => expect(verifierToggle?.getAttribute('aria-pressed')).toBe('false'));

		await userEvent.unhover(lastCol);
		const reopened = await openTooltip(lastCol);
		await expect(reopened.textContent).not.toContain('verifier');
		await expect(reopened.textContent).toContain('Turn judge');
	}}
/>
