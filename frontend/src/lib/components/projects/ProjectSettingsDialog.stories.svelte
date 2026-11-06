<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
	import ProjectSettingsDialog from './ProjectSettingsDialog.svelte';
	import type { FleetProjectWithCounts } from '$lib/fleet/types';

	function project(
		path: string,
		uuid: string,
		extra: Partial<FleetProjectWithCounts> = {}
	): FleetProjectWithCounts {
		return {
			project_path: path,
			project_uuid: uuid,
			session_count: 1,
			request_count: 11,
			first_seen: '2026-09-21T15:41:32Z',
			last_seen: '2026-09-21T15:49:19Z',
			pinned: false,
			custom_name: null,
			...extra
		};
	}

	const MAIN = project(
		'/mnt/data/projects/temet/temet-proxy',
		'ec5bcb17-7792-47c2-9f7b-c07b1f417f92',
		{
			session_count: 213,
			custom_name: 'mike'
		}
	);
	const WORKTREE = project(
		'/mnt/data/projects/temet/temet-proxy-wt-wp03',
		'3475c49b-4f89-40af-a966-2d74b249b5df'
	);
	const UNRELATED = project('/mnt/data/projects/omarchy', '9d3b1c55-0000-4000-8000-000000000001');

	const LONG_WORKTREE = project(
		'/mnt/data/projects/ankh-morpork/ai/svc-coord-eval-f0/worktrees/very-long-branch-name',
		'5b1e2f00-0000-4000-8000-000000000010',
		{ custom_name: 'Coordination eval worktree: very-long-branch-name experiment' }
	);
	const LONG_PARENT = project(
		'/mnt/data/projects/ankh-morpork/ai/svc-coord-eval-f0',
		'5b1e2f00-0000-4000-8000-000000000011',
		{ session_count: 48, custom_name: 'svc-coord-eval-f0' }
	);
	const LONG_SIBLINGS = [
		'/mnt/data/projects/ankh-morpork/ai/svc-coord-eval-f0/worktrees/another-extremely-long-feature-branch-name-for-review',
		'/mnt/data/projects/ankh-morpork/ai/svc-coord-eval-f1',
		'/mnt/data/projects/ankh-morpork/ai/svc-coord-eval-f2/worktrees/rebase-onto-main-after-the-schema-split',
		'/home/vimes/.local/share/checkouts/ankh-morpork/platform/infrastructure-terraform-modules',
		'/mnt/data/projects/temet/temet-proxy-wt-wp07'
	].map((path, i) => project(path, `5b1e2f00-0000-4000-8000-00000000002${i}`));

	const { Story } = defineMeta({
		title: 'Projects/ProjectSettingsDialog',
		component: ProjectSettingsDialog,
		tags: ['autodocs']
	});

	const onMergeProject = fn(async () => {});
	const onCreateProject = fn(async () => {});
	const onRenameProject = fn(async () => {});
	const onClose = fn();
</script>

<Story
	name="Move into an existing project"
	args={{
		project: LONG_WORKTREE,
		projects: [MAIN, WORKTREE, UNRELATED, LONG_WORKTREE, LONG_PARENT, ...LONG_SIBLINGS],
		onRenameProject,
		onMergeProject,
		onCreateProject,
		onClose
	}}
	play={async ({ canvasElement }) => {
		const body = within(canvasElement.ownerDocument.body);
		const dialog = await body.findByRole('dialog');
		await waitFor(() => expect(within(dialog).getByLabelText('Name')).toHaveFocus());
		await expect(
			within(dialog).getByRole('radio', { name: /svc-coord-eval-f0.*Suggested/ })
		).toBeInTheDocument();
		await userEvent.click(within(dialog).getByRole('radio', { name: /mike/ }));
		await expect(within(dialog).getByText(/1 session will move/)).toBeInTheDocument();
		await userEvent.click(within(dialog).getByRole('button', { name: 'Save' }));
		await waitFor(() =>
			expect(onMergeProject).toHaveBeenCalledWith(LONG_WORKTREE.project_path, MAIN.project_uuid)
		);
	}}
/>

<Story
	name="Create a new project"
	args={{
		project: WORKTREE,
		projects: [MAIN, WORKTREE],
		onRenameProject,
		onMergeProject,
		onCreateProject,
		onClose
	}}
	play={async ({ canvasElement }) => {
		const body = within(canvasElement.ownerDocument.body);
		const dialog = await body.findByRole('dialog');
		await waitFor(() => expect(within(dialog).getByLabelText('Name')).toHaveFocus());
		await userEvent.click(within(dialog).getByRole('radio', { name: /Create a new project/ }));
		const save = within(dialog).getByRole('button', { name: 'Save' });
		await expect(save).toBeDisabled();
		await userEvent.type(within(dialog).getByLabelText('New project name'), 'Coordination eval');
		await waitFor(() => expect(within(dialog).getByRole('button', { name: 'Save' })).toBeEnabled());
		await userEvent.click(within(dialog).getByRole('button', { name: 'Save' }));
		await waitFor(() =>
			expect(onCreateProject).toHaveBeenCalledWith(WORKTREE.project_path, 'Coordination eval')
		);
	}}
/>
