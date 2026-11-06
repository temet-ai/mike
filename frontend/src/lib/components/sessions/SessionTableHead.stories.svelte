<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect } from 'storybook/test';
	import SessionTableHead from './SessionTableHead.svelte';

	const { Story } = defineMeta({
		title: 'Components/Sessions/SessionTableHead',
		component: SessionTableHead,
		tags: ['autodocs'],
		parameters: {
			layout: 'padded',
			docs: {
				description: {
					component:
						'Static 6-column header row (Session / Req / Started / Last active / Session id) shared by every browse-mode project card and search-mode result card so they can never drift.'
				}
			}
		}
	});
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText('Session')).toBeInTheDocument();
		await expect(canvas.getByText('Req')).toBeInTheDocument();
		await expect(canvas.getByText('Started')).toBeInTheDocument();
		await expect(canvas.getByText('Last active')).toBeInTheDocument();
		await expect(canvas.getByText('Session id')).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="w-[720px] bg-[var(--surface-card)]">
			<SessionTableHead />
		</div>
	{/snippet}
</Story>
