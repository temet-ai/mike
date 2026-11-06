<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import Page from './+page.svelte';

	const { Story } = defineMeta({
		title: 'Routes/Help/Guides/FilterRules',
		component: Page,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/help/guides/filter-rules'),
					params: {}
				}
			}
		}
	}}
/>
