<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import Page from './+page.svelte';
	import { PROJECT_DOCS } from '$lib/help/externalDocs';
	import { DOCUMENT_CONTENT } from '$lib/help/documents';

	const { Story } = defineMeta({
		title: 'Routes/Help/InstalledDocument',
		component: Page,
		tags: ['autodocs']
	});

	function documentData() {
		return {
			document: PROJECT_DOCS.find((entry) => entry.href === '/help/docs/installed-setup')!,
			content: DOCUMENT_CONTENT['installed-setup']!
		};
	}
</script>

<Story
	name="InstalledGuide"
	args={{ data: documentData() }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByRole('heading', { name: 'Installed setup' })).toBeVisible();
		await expect(canvas.getByRole('navigation', { name: 'Installed documentation' })).toBeVisible();
		await expect(canvas.getByRole('link', { name: 'Configuration and import' })).toHaveAttribute(
			'href',
			'/help/docs/configuration-and-import'
		);
		await expect(canvas.getByRole('link', { name: 'Getting started' })).toHaveAttribute(
			'href',
			'/help/guides/getting-started'
		);
	}}
/>
