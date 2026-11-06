<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect } from 'storybook/test';
	import ProjectDetailPanel from './ProjectDetailPanel.svelte';
	import type { FleetProjectWithCounts } from '$lib/fleet/types';

	const PROJECT: FleetProjectWithCounts = {
		project_path: 'projects/temet/temet-proxy',
		session_count: 42,
		request_count: 1380,
		first_seen: '2026-04-10T09:00:00Z',
		last_seen: '2026-07-16T08:30:00Z',
		pinned: true,
		custom_name: null
	};

	const { Story } = defineMeta({
		title: 'Projects/ProjectDetailPanel',
		component: ProjectDetailPanel,
		tags: ['autodocs']
	});
</script>

<Story
	name="Populated"
	args={{ project: PROJECT }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(
			canvas.getByText('projects/temet/temet-proxy', {
				selector: '[data-slot="project-detail-path"]'
			})
		).toBeInTheDocument();

		const vitals = canvasElement.querySelector(
			'[data-slot="project-detail-vitals"]'
		) as HTMLElement;
		await expect(vitals).toBeInTheDocument();
		await expect(
			within(vitals).getByText('42', { selector: '[data-slot="project-detail-session-count"]' })
		).toBeInTheDocument();
		const requestCountText = vitals.querySelector(
			'[data-slot="project-detail-request-count"]'
		)?.textContent;
		await expect(requestCountText === '1,380' || requestCountText === '1380').toBe(true);
		await expect(
			within(vitals).getByText('Pinned', { selector: '[data-slot="project-detail-pinned"]' })
		).toBeInTheDocument();

		await expect(canvas.getByText(/still being designed/i)).toBeInTheDocument();
	}}
>
	{#snippet template(args)}
		<div class="w-[560px] bg-[var(--surface-bg)] p-6">
			<ProjectDetailPanel {...args} />
		</div>
	{/snippet}
</Story>
