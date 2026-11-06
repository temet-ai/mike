<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import Button from './button.svelte';

	const { Story } = defineMeta({
		title: 'UI/Button',
		component: Button,
		tags: ['autodocs'],
		argTypes: {
			variant: {
				control: 'select',
				options: ['default', 'outline', 'ghost', 'destructive']
			},
			size: {
				control: 'select',
				options: ['default', 'sm', 'icon']
			}
		}
	});
</script>

<Story
	name="Default"
	args={{ variant: 'default' }}
	play={async ({ canvasElement }) => {
		const action = within(canvasElement).getByRole('button', { name: 'Primary action' });
		await expect(action).toHaveAttribute('data-slot', 'preview-primary-action');
		await expect(action).toHaveAttribute('data-button-root');
		await expect(getComputedStyle(action).getPropertyValue('--aug-border-all').trim()).toBe('1px');
	}}
>
	{#snippet template(args)}
		<Button {...args} data-slot="preview-primary-action">Primary action</Button>
	{/snippet}
</Story>

<Story name="Outline" args={{ variant: 'outline' }}>
	{#snippet template(args)}
		<Button {...args}>Secondary action</Button>
	{/snippet}
</Story>

<Story name="Ghost" args={{ variant: 'ghost' }}>
	{#snippet template(args)}
		<Button {...args}>Nav item</Button>
	{/snippet}
</Story>

<Story name="Destructive" args={{ variant: 'destructive' }}>
	{#snippet template(args)}
		<Button {...args}>Delete session</Button>
	{/snippet}
</Story>

<Story name="SizeSm" args={{ variant: 'default', size: 'sm' }}>
	{#snippet template(args)}
		<Button {...args}>Compact action</Button>
	{/snippet}
</Story>

<Story name="SizeIcon" args={{ variant: 'ghost', size: 'icon' }}>
	{#snippet template(args)}
		<Button {...args} aria-label="Copy request body">
			<svg
				class="size-4"
				xmlns="http://www.w3.org/2000/svg"
				fill="none"
				viewBox="0 0 24 24"
				stroke="currentColor"
				aria-hidden="true"
			>
				<path
					stroke-linecap="round"
					stroke-linejoin="round"
					stroke-width="2"
					d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"
				/>
			</svg>
		</Button>
	{/snippet}
</Story>

<Story name="Disabled" args={{ variant: 'default' }}>
	{#snippet template(args)}
		<Button {...args} disabled>Unavailable action</Button>
	{/snippet}
</Story>

<Story name="AsLink" args={{ variant: 'outline' }}>
	{#snippet template(args)}
		<Button {...args} href="https://example.com/sessions">Open sessions</Button>
	{/snippet}
</Story>

<Story name="DisabledLink" args={{ variant: 'outline' }}>
	{#snippet template(args)}
		<Button {...args} href="https://example.com/sessions" disabled>Open sessions (disabled)</Button>
	{/snippet}
</Story>
