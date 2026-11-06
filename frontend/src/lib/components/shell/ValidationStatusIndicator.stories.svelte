<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, waitFor } from 'storybook/test';
	import ValidationStatusIndicator from './ValidationStatusIndicator.svelte';
	import type { ValidationState } from './ValidationStatusIndicator.svelte';

	let previewState = $state<ValidationState>('pending');
	const pendingSince = new Date(Date.now() - 12_000).toISOString();

	const { Story } = defineMeta({
		title: 'Shell/ValidationStatusIndicator',
		component: ValidationStatusIndicator,
		tags: ['autodocs'],
		argTypes: { state: { control: false } }
	});
</script>

<Story
	name="Default"
	args={{ state: 'pending', since: pendingSince, detail: null }}
	play={async ({ canvasElement }) => {
		await expect(canvasElement.textContent).toContain('this answer is being validated');
		await expect(canvasElement.querySelector('.vsi-badge')).toBeNull();
		await expect(canvasElement.querySelectorAll('.vsi-dot').length).toBe(3);

		previewState = 'pass';
		await waitFor(() => expect(canvasElement.querySelector('.vsi-badge--pass')).toBeTruthy());
		await expect(canvasElement.textContent).toContain('validated');
		await expect(canvasElement.querySelector('.vsi-pending-line')).toBeNull();

		previewState = 'correction';
		await waitFor(() => expect(canvasElement.querySelector('.vsi-badge--correction')).toBeTruthy());
		await expect(canvasElement.textContent).toContain('correction');
		await expect(canvasElement.textContent).toContain(
			'verifier · claimed done without running the tests'
		);

		previewState = null;
		await waitFor(() =>
			expect(canvasElement.querySelector('[data-slot="validation-status-indicator"]')).toBeNull()
		);
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface-bg);">
			<ValidationStatusIndicator
				state={previewState}
				since={pendingSince}
				detail={previewState === 'correction'
					? 'verifier · claimed done without running the tests'
					: null}
			/>
		</div>
	{/snippet}
</Story>
