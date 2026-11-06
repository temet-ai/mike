<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, userEvent, waitFor } from 'storybook/test';
	import ProjectNotebook from './ProjectNotebook.svelte';
	import { stubFetchJsonRouter, typed, type TypedBody } from '$lib/test-support/storyFetchStubs';
	import { __resetApiCacheForTests } from '$lib/stores/cache/apiCache.svelte';
	import type { ProjectNotesResponse, ProjectSessionNoteDto } from '$lib/comms/sessionNotes';
	import 'augmented-ui/augmented-ui.min.css';

	const PROJECT_PATH = 'projects/temet/temet-proxy';

	function makeNote(
		partial: Partial<ProjectSessionNoteDto> & { note_uuid: string; session_uuid: string }
	): ProjectSessionNoteDto {
		return {
			session_name: null,
			session_role: null,
			kind: 'note',
			title: 'untitled',
			body: '',
			status: null,
			tags: [],
			data: {},
			created_at: '2026-09-19T10:00:00Z',
			updated_at: '2026-09-19T10:00:00Z',
			...partial
		};
	}

	const ALL_NOTES: ProjectSessionNoteDto[] = [
		makeNote({
			note_uuid: 'b1111111-0000-4000-8000-000000000001',
			session_uuid: '872d7da7-a414-4432-bef2-630c65ef66b5',
			session_name: 'observer-1',
			session_role: 'observer',
			kind: 'finding',
			title: 'Coordinator seat missed a heartbeat window under load',
			body: 'Two consecutive heartbeat cycles were skipped while the coordinator was mid-verification; the fleet did not treat this as rot because reply_channel_open stayed true throughout.',
			tags: ['fleet', 'heartbeat'],
			created_at: '2026-09-19T09:40:00Z'
		}),
		makeNote({
			note_uuid: 'b1111111-0000-4000-8000-000000000002',
			session_uuid: '9770e3f6-0000-4000-8000-000000000005',
			session_name: 'rust-dev-2',
			session_role: 'rust-developer',
			kind: 'task',
			title: 'Add index on lower(display_name) for name resolution',
			status: 'in_progress',
			created_at: '2026-09-19T08:10:00Z'
		}),
		makeNote({
			note_uuid: 'b1111111-0000-4000-8000-000000000003',
			session_uuid: '872d7da7-a414-4432-bef2-630c65ef66b5',
			session_name: 'observer-1',
			session_role: 'observer',
			kind: 'finding',
			title: 'Verification theater on job #482 — claimed done, files untouched',
			body: 'The worker reported the migration file as written, but git status showed no change in the target path.',
			tags: ['verification'],
			created_at: '2026-09-19T07:05:00Z'
		})
	];

	function router(method: string, url: string): TypedBody {
		const u = new URL(url, 'http://localhost');
		const role = u.searchParams.get('role');
		const kind = u.searchParams.get('kind');
		const filtered = ALL_NOTES.filter(
			(n) => (!role || n.session_role === role) && (!kind || n.kind === kind)
		);
		return typed<ProjectNotesResponse>({
			project_id: 1,
			project_path: PROJECT_PATH,
			total_count: filtered.length,
			offset: 0,
			limit: 200,
			notes: filtered
		});
	}

	const { Story } = defineMeta({
		title: 'Fleet/ProjectNotebook',
		component: ProjectNotebook,
		tags: ['autodocs']
	});
</script>

<Story
	name="FilteredByRole"
	args={{ projectPath: PROJECT_PATH }}
	loaders={[
		async () => {
			__resetApiCacheForTests();
			stubFetchJsonRouter(router);
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await waitFor(() => expect(canvas.getByText(/^3 notes/)).toBeInTheDocument());
		await expect(
			canvas.getByText('Coordinator seat missed a heartbeat window under load')
		).toBeInTheDocument();
		await expect(
			canvas.getByText('Add index on lower(display_name) for name resolution')
		).toBeInTheDocument();

		const roleInput = canvas.getByLabelText('Role');
		await userEvent.type(roleInput, 'observer');

		await waitFor(
			() => expect(canvas.getByText('2 notes matching these filters')).toBeInTheDocument(),
			{ timeout: 2000 }
		);
		await expect(
			canvas.queryByText('Add index on lower(display_name) for name resolution')
		).toBeNull();
		await expect(
			canvas.getByText('Verification theater on job #482 — claimed done, files untouched')
		).toBeInTheDocument();

		await userEvent.click(canvas.getByRole('button', { name: 'Clear filters' }));
		await waitFor(() => expect(canvas.getByText(/^3 notes/)).toBeInTheDocument(), {
			timeout: 2000
		});
	}}
/>
