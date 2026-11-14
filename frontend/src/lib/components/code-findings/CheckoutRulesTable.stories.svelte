<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import CheckoutRulesTable from './CheckoutRulesTable.svelte';
	import {
		CODE_FINDINGS_CHECKOUT,
		CODE_PROJECT_RULES
	} from '$lib/test-support/codeFindingsFixtures';

	const { Story } = defineMeta({
		title: 'CodeFindings/CheckoutRulesTable',
		component: CheckoutRulesTable,
		tags: ['autodocs']
	});

	const WATCHED = [CODE_FINDINGS_CHECKOUT, '/home/dev/projects/holdfast', '/home/dev/gg'];
</script>

<Story
	name="Loaded, failed and plain checkouts"
	args={{ watched: WATCHED, projectRules: CODE_PROJECT_RULES }}
	play={async ({ canvas, canvasElement }) => {
		await expect(canvas.getByRole('table', { name: 'Checkouts and project rules' })).toBeVisible();
		await expect(canvas.getByText('loaded')).toBeVisible();
		await expect(canvas.getByText('error')).toBeVisible();
		await expect(canvas.getByText('no project rules')).toBeVisible();
		await expect(canvas.getByText('.mike/rules/no-println.yml')).toBeVisible();
		const error = canvas.getByText(/unknown field 'langauge' at line 3, column 1/);
		await expect(error).toBeVisible();
		await expect(error.scrollWidth).toBeLessThanOrEqual(error.clientWidth + 1);
		await expect(canvasElement.querySelectorAll('[data-slot="checkout-rules-error"]')).toHaveLength(
			1
		);
	}}
>
	{#snippet template(args)}
		<div style="max-width: 900px; padding: 12px;">
			<CheckoutRulesTable {...args} />
		</div>
	{/snippet}
</Story>
