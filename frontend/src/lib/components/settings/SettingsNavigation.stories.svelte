<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import SettingsNavigation from './SettingsNavigation.svelte';
	const { Story } = defineMeta({
		title: 'Settings/SettingsNavigation',
		component: SettingsNavigation
	});
</script>

<Story
	name="Default"
	args={{ current: '/settings/workers' }}
	play={async ({ canvasElement }) => {
		const navigation = within(canvasElement).getByRole('navigation', { name: 'Settings sections' });
		await expect(within(navigation).getByRole('link', { name: 'Workers' })).toHaveAttribute(
			'aria-current',
			'page'
		);
		await expect(within(navigation).getByRole('link', { name: 'Integrations' })).toHaveAttribute(
			'href',
			'/settings/integrations'
		);
		await expect(within(navigation).getByRole('link', { name: 'Assistants' })).toHaveAttribute(
			'href',
			'/settings/assistants'
		);
	}}
/>
