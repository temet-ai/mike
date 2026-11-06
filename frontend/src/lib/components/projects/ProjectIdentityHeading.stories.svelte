<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, waitFor } from 'storybook/test';
	import ProjectIdentityHeading from './ProjectIdentityHeading.svelte';
	import type { FleetProjectSummary } from '$lib/fleet/types';

	const PROJECT: FleetProjectSummary = {
		project_path: 'projects/temet/temet-proxy',
		session_count: 42,
		request_count: 1380,
		first_seen: '2026-04-10T09:00:00Z',
		last_seen: '2026-07-16T08:30:00Z',
		pinned: true,
		custom_name: null
	};

	const { Story } = defineMeta({
		title: 'Projects/ProjectIdentityHeading',
		component: ProjectIdentityHeading,
		tags: ['autodocs']
	});
</script>

<Story
	name="Renamed"
	args={{ project: PROJECT, onRenameProject: fn(async () => {}) }}
	play={async ({ args, canvas }) => {
		await expect(
			canvas.getByText('projects/temet/temet-proxy', {
				selector: '[data-slot="project-detail-custom-name"]'
			})
		).toBeInTheDocument();
		await expect(
			canvas.getByText('projects/temet/temet-proxy', {
				selector: '[data-slot="project-detail-path"]'
			})
		).toBeInTheDocument();

		await canvas.getByLabelText('Edit friendly name for this project').click();

		const nameInput = canvas.getByLabelText('Friendly name for this project');
		await expect(nameInput).toHaveFocus();
		await userEvent.type(nameInput, 'mike proxy');
		await canvas.getByLabelText('Save friendly name').click();

		await waitFor(() =>
			expect(args.onRenameProject).toHaveBeenCalledWith('projects/temet/temet-proxy', 'mike proxy')
		);

		await expect(
			await canvas.findByLabelText('Edit friendly name for this project')
		).toBeInTheDocument();
		await expect(
			canvas.getByText('projects/temet/temet-proxy', {
				selector: '[data-slot="project-detail-path"]'
			})
		).toBeInTheDocument();
	}}
/>
