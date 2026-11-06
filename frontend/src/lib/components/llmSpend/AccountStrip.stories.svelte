<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor } from 'storybook/test';
	import AccountStrip from './AccountStrip.svelte';
	import {
		buildFixtureToday,
		buildFixtureUsage,
		buildFixtureUsageWithoutSpendFields
	} from './llmSpendFixtures';

	function tooltipContent(): HTMLElement | null {
		return document.body.querySelector('[data-slot="tooltip-text"]') as HTMLElement | null;
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
		title: 'LlmSpend/AccountStrip',
		component: AccountStrip,
		tags: ['autodocs'],
		parameters: { layout: 'padded' }
	});
</script>

<Story
	name="Default"
	args={{
		usage: buildFixtureUsage(),
		today: buildFixtureToday(),
		todayError: null
	}}
	play={async ({ canvasElement }) => {
		await expect(canvasElement.textContent).toMatch(/≈ 1\.6 days/);
		await expect(canvasElement.textContent).toContain('$7.72');
		await expect(canvasElement.textContent).toContain('near ceiling');

		await expect(canvasElement.textContent).toContain('$36.13');
		await expect(canvasElement.textContent).toContain('$20.36');
		await expect(canvasElement.textContent).toContain('$15.77');

		const unrecordedTerm = Array.from(canvasElement.querySelectorAll('.as-term')).find((el) =>
			el.textContent?.includes('Unrecorded')
		) as HTMLElement;
		await expect(unrecordedTerm).toBeTruthy();
		const content = await openTooltip(unrecordedTerm);
		await expect(content.textContent).toContain('no mike call record explains');
	}}
/>

<!-- Operator-requested (2026-09-28): prove the page degrades honestly, never crashes, when a
     live proxy build predates the recorded/unrecorded/days-left/ceiling fields. -->
<Story
	name="MissingSpendFields"
	args={{
		usage: buildFixtureUsageWithoutSpendFields(),
		today: buildFixtureToday(),
		todayError: null
	}}
	play={async ({ canvasElement }) => {
		await expect(canvasElement.textContent).toContain('Not available from this proxy build yet.');
		await expect(canvasElement.textContent).not.toContain('undefined');
		await expect(canvasElement.textContent).not.toContain('NaN');
		await expect(canvasElement.textContent).toContain('$7.72');
		await expect(canvasElement.textContent).toContain('near ceiling');
	}}
/>
