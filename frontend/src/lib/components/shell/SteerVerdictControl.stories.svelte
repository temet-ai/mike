<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import SteerVerdictControl from './SteerVerdictControl.svelte';
	import { stubFetchJsonRouter, typed, type TypedBody } from '$lib/test-support/storyFetchStubs';
	import type { SteerEvaluationLabelDto } from '$lib/types';

	let posted: string | null = null;

	function router(method: string, url: string): TypedBody {
		if (method === 'POST' && url.includes('/label')) {
			posted = url;
			return typed<SteerEvaluationLabelDto>({ label_id: 5, cancelled_wakes: [9] });
		}
		return typed<Record<string, never>>({});
	}

	const { Story } = defineMeta({
		title: 'Shell/SteerVerdictControl',
		component: SteerVerdictControl,
		tags: ['autodocs']
	});
</script>

<Story
	name="SteerVerdictControl"
	loaders={[
		async () => {
			posted = null;
			stubFetchJsonRouter(router);
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await userEvent.click(canvas.getByRole('button', { name: 'Wrong' }));
		await waitFor(() =>
			expect(canvasElement.querySelector('[data-steer-verdict="wrong"]')).not.toBeNull()
		);
		await expect(posted).toContain(
			'/api/steering/evaluations/0b6f3c1e-0000-4000-8000-000000000001/label'
		);
		await expect(canvasElement.textContent).toContain('A pending wake for this fire is cancelled.');
	}}
>
	{#snippet template()}
		<div class="bg-[var(--surface-bg)] p-4">
			<SteerVerdictControl evaluationUuid="0b6f3c1e-0000-4000-8000-000000000001" />
		</div>
	{/snippet}
</Story>
