<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor } from 'storybook/test';
	import CallerTable from './CallerTable.svelte';
	import { buildFixtureCallers } from './llmSpendFixtures';

	const CALLERS = buildFixtureCallers();
	const TOTAL = CALLERS.reduce((sum, c) => sum + c.cost_usd, 0);

	function rowLabels(canvasElement: HTMLElement): string[] {
		return Array.from(canvasElement.querySelectorAll('.ct-row .ct-td--label')).map(
			(el) => el.textContent?.trim() ?? ''
		);
	}

	const { Story } = defineMeta({
		title: 'LlmSpend/CallerTable',
		component: CallerTable,
		tags: ['autodocs'],
		parameters: { layout: 'padded' }
	});
</script>

<Story
	name="Default"
	args={{ callers: CALLERS, totalCostUsd: TOTAL }}
	play={async ({ canvasElement }) => {
		try {
			localStorage.removeItem('mike:llm-spend:caller-table:collapsed-groups');
		} catch {
			/* best effort */
		}

		// Groups, not flat callers, lead the table — the Steerers group (cost 45.00) outranks
		// the single-caller Turn judge group (cost 5.10), so no role interleaving at the top.
		await expect(rowLabels(canvasElement)[0]).toBe('Steerers');
		await expect(canvasElement.textContent).toContain('112,000');
		await expect(canvasElement.textContent).toContain('$0.044');
		await expect(canvasElement.textContent).toContain('77.4%');

		// Groups start expanded: the steerer members render as short names, not the
		// "Steerer · X" / "Steerer (unattributed)" labels the header already covers.
		await expect(canvasElement.textContent).toContain('verifier');
		await expect(canvasElement.textContent).not.toContain('Steerer · verifier');
		await expect(canvasElement.textContent).toContain('Unattributed');
		await expect(canvasElement.textContent).not.toContain('Steerer (unattributed)');

		const verifierLink = Array.from(canvasElement.querySelectorAll('.ct-caller-link')).find(
			(a) => a.textContent?.trim() === 'verifier'
		) as HTMLAnchorElement;
		await expect(verifierLink.getAttribute('href')).toBe('/steerers/6');

		const unattributedRow = Array.from(canvasElement.querySelectorAll('.ct-row')).find((row) =>
			row.textContent?.includes('Unattributed')
		);
		await expect(unattributedRow?.querySelector('.ct-caller-link')).toBeNull();
		await expect(unattributedRow?.querySelector('.ct-caller-plain')).toBeTruthy();

		// A single-member non-steerer group (Turn judge) collapses to one plain row, keeping its link.
		const turnJudgeRow = Array.from(canvasElement.querySelectorAll('.ct-row')).find((row) =>
			row.textContent?.includes('Turn judge')
		);
		await expect(turnJudgeRow?.querySelector('.ct-group-toggle')).toBeNull();
		const turnJudgeLink = turnJudgeRow?.querySelector('.ct-caller-link') as
			| HTMLAnchorElement
			| undefined;
		await expect(turnJudgeLink?.getAttribute('href')).toBe('/sessions/labelling');

		// Collapsing the Steerers group hides its member rows without disturbing other groups.
		const steererToggle = Array.from(canvasElement.querySelectorAll('.ct-group-toggle')).find(
			(b) => b.textContent?.trim() === 'Steerers'
		) as HTMLButtonElement;
		await expect(steererToggle.getAttribute('aria-expanded')).toBe('true');
		await userEvent.click(steererToggle);
		await waitFor(() => expect(steererToggle.getAttribute('aria-expanded')).toBe('false'));
		await expect(canvasElement.textContent).not.toContain('verifier');
		await expect(canvasElement.textContent).toContain('Turn judge');
		await userEvent.click(steererToggle);
		await waitFor(() => expect(canvasElement.textContent).toContain('verifier'));

		// Sorting by Calls reorders GROUPS by their group total (Turn judge's 2200 calls beats
		// the Steerers group's combined 1070) — groups never interleave.
		const callsHeader = Array.from(canvasElement.querySelectorAll('.ct-sort-btn')).find(
			(b) => b.textContent?.trim() === 'Calls'
		) as HTMLButtonElement;
		await userEvent.click(callsHeader);
		await waitFor(() => expect(rowLabels(canvasElement)[0]).toBe('Turn judge'));

		const failedCell = Array.from(canvasElement.querySelectorAll('.ct-td--danger')).find((el) =>
			el.textContent?.trim() === '12'
		);
		await expect(failedCell).toBeTruthy();
	}}
/>
