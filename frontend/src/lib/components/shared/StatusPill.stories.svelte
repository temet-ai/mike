<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import StatusPill from './StatusPill.svelte';

	const { Story } = defineMeta({
		title: 'Shared/StatusPill',
		component: StatusPill,
		tags: ['autodocs'],
		argTypes: {
			status: {
				control: 'select',
				options: ['Pending', 'Streaming', 'Complete']
			}
		}
	});
</script>

<Story name="Complete" args={{ status: 'Complete' }}>
	{#snippet template(args)}
		<StatusPill {...args} />
	{/snippet}
</Story>

<Story name="Pending" args={{ status: 'Pending' }}>
	{#snippet template(args)}
		<StatusPill {...args} />
	{/snippet}
</Story>

<Story name="Streaming" args={{ status: 'Streaming' }}>
	{#snippet template(args)}
		<StatusPill {...args} />
	{/snippet}
</Story>

<Story name="Error" args={{ status: { Error: 'upstream timeout' } }}>
	{#snippet template(args)}
		<StatusPill {...args} />
	{/snippet}
</Story>
