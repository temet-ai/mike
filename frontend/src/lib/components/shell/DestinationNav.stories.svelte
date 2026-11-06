<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect } from 'storybook/test';
	import DestinationNavHarness from './__harness__/DestinationNavHarness.svelte';
	import { shellState } from '$lib/stores/shellState.svelte';
	import 'augmented-ui/augmented-ui.min.css';

	const { Story } = defineMeta({
		title: 'Shell/DestinationNav',
		component: DestinationNavHarness,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' }
	});
</script>

<Story
	name="Default"
	parameters={{
		sveltekit_experimental: {
			state: { page: { url: new URL('http://localhost/settings/steering'), params: {} } }
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const active = canvas.getByRole('link', { name: 'Steering' });
		await expect(active).toHaveAttribute('aria-current', 'page');
		await expect(canvas.getByRole('link', { name: 'Settings' })).not.toHaveAttribute(
			'aria-current'
		);
		await expect(canvasElement.querySelectorAll('[aria-current="page"]')).toHaveLength(1);
		active.focus();
		await expect(active).toHaveFocus();

		// G11: Assistants sits between Workers and Embedding under Settings.
		const settingsLinks = ['Steering', 'Workers', 'Assistants', 'Embedding', 'Flagging'].map(
			(name) => canvas.getByRole('link', { name })
		);
		const labels = settingsLinks.map((el) => el.textContent?.trim());
		expect(labels.indexOf('Assistants')).toBe(labels.indexOf('Workers') + 1);
		expect(labels.indexOf('Embedding')).toBe(labels.indexOf('Assistants') + 1);
		expect(canvas.getByRole('link', { name: 'Assistants' }).getAttribute('href')).toContain(
			'/settings/assistants'
		);
		await userEvent.click(canvas.getByRole('button', { name: 'Collapse navigation column' }));
		await expect(shellState.sessionsCollapsed).toBe(true);
		shellState.sessionsCollapsed = false;
	}}
/>
