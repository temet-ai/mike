<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import Tag from './Tag.svelte';

	const { Story } = defineMeta({
		title: 'Components/MessageView/Tag',
		component: Tag,
		tags: ['autodocs'],
		argTypes: {
			variant: {
				control: 'select',
				options: [
					'default',
					'group',
					'block',
					'hook',
					'boot',
					'event-info',
					'event-warn',
					'catchall'
				]
			}
		}
	});
</script>

<Story name="Hook" args={{ variant: 'hook' }}>
	{#snippet template(args)}<Tag {...args}>hook</Tag>{/snippet}
</Story>
