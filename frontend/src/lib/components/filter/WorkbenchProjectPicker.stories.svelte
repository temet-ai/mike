<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import WorkbenchProjectPicker from './WorkbenchProjectPicker.svelte';
	import type { ProjectSummaryDto } from '$lib/types';

	function project(overrides: Partial<ProjectSummaryDto> & { project_path: string }): ProjectSummaryDto {
		return {
			project_id: 1,
			project_uuid: 'uuid',
			session_count: 4,
			request_count: 40,
			first_seen: '2026-01-01T00:00:00Z',
			last_seen: '2026-09-01T00:00:00Z',
			pinned: false,
			custom_name: null,
			...overrides
		};
	}

	// 12 projects — over PROJECT_PICKER_SEARCH_THRESHOLD (10), so the filter input shows.
	const PROJECTS: ProjectSummaryDto[] = [
		project({ project_id: 1, project_path: '/mnt/data/projects/temet/temet-proxy', custom_name: 'temet-proxy', session_count: 120 }),
		project({ project_id: 2, project_path: '/mnt/data/projects/alpha', custom_name: 'Alpha' }),
		project({ project_id: 3, project_path: '/mnt/data/projects/beta' }),
		project({ project_id: 4, project_path: '/mnt/data/projects/gamma' }),
		project({ project_id: 5, project_path: '/mnt/data/projects/delta' }),
		project({ project_id: 6, project_path: '/mnt/data/projects/epsilon' }),
		project({ project_id: 7, project_path: '/mnt/data/projects/zeta' }),
		project({ project_id: 8, project_path: '/mnt/data/projects/eta' }),
		project({ project_id: 9, project_path: '/mnt/data/projects/theta' }),
		project({ project_id: 10, project_path: '/mnt/data/projects/iota' }),
		project({ project_id: 11, project_path: '/mnt/data/projects/kappa' }),
		project({ project_id: 12, project_path: '/mnt/data/projects/lambda' })
	];

	const { Story } = defineMeta({
		title: 'Routes/Filter/WorkbenchProjectPicker',
		component: WorkbenchProjectPicker,
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	let value = $state<string | null>(null);
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		// The popover's content renders through a portal (outside canvasElement, matching
		// bits-ui's own Popover.Portal), so its options/search input are queried against
		// document.body — same pattern as AddRulePopover.stories.svelte.
		const body = within(document.body);

		const trigger = await canvas.findByRole('button', { name: 'Prompt project: Global' });
		await userEvent.click(trigger);

		const globalOption = await body.findByRole('option', { name: /Global/ });
		await expect(globalOption).toHaveAttribute('aria-selected', 'true');
		await expect(await body.findByText('current')).toBeInTheDocument();

		// Over the search threshold, so the filter input shows and narrows the list.
		const search = await body.findByPlaceholderText('Filter projects…');
		await userEvent.type(search, 'alpha');
		await expect(await body.findByRole('option', { name: /Alpha/ })).toBeInTheDocument();
		await expect(body.queryByRole('option', { name: /Beta/ })).toBeNull();

		await userEvent.click(await body.findByRole('option', { name: /Alpha/ }));
		await expect(value).toBe('/mnt/data/projects/alpha');

		// Re-opening shows the picked project's own name as the trigger label, and re-selects it.
		await expect(
			await canvas.findByRole('button', { name: 'Prompt project: Alpha' })
		).toBeInTheDocument();
		await userEvent.click(await canvas.findByRole('button', { name: 'Prompt project: Alpha' }));
		const alphaOption = await body.findByRole('option', { name: /Alpha/ });
		await expect(alphaOption).toHaveAttribute('aria-selected', 'true');

		// Picking Global again clears the pick.
		await userEvent.click(await body.findByRole('option', { name: /Global/ }));
		await expect(value).toBeNull();
	}}
>
	{#snippet template()}
		<div class="h-[420px] w-[360px] bg-[var(--surface-bg)] p-4">
			<WorkbenchProjectPicker
				projects={PROJECTS}
				loading={false}
				error={null}
				{value}
				onSelect={(next) => (value = next)}
			/>
		</div>
	{/snippet}
</Story>
