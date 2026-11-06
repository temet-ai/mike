<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import ChangeLogTable from './ChangeLogTable.svelte';
	import type { ChangeLogRow } from '$lib/api/configHistory';

	const PROMPT = 'Decide whether the answer holds up. '.repeat(200) + 'LAST-LINE';

	const ROWS: ChangeLogRow[] = [
		{
			id: 3,
			changed_at: '2026-09-24T12:00:03Z',
			xact_id: '1003',
			subject_kind: 'steerer',
			subject_key: '4',
			op: 'soft_delete',
			old_value: { id: 4, name: 'verifier', prompt: PROMPT, deleted_at: null },
			new_value: { id: 4, name: 'verifier', prompt: PROMPT, deleted_at: '2026-09-24T12:00:03Z' },
			redacted: false,
			via: 'api',
			actor: 'mike-mcp',
			request_id: 'req-3',
			build_sha: 'cb761820aa',
			db_user: 'devuser',
			application_name: 'claude-proxy',
			detail: {}
		},
		{
			id: 2,
			changed_at: '2026-09-24T12:00:02Z',
			xact_id: '1002',
			subject_kind: 'build',
			subject_key: 'mike',
			op: 'observe',
			old_value: null,
			new_value: { sha: 'cb761820aa', defaults: { settings: {}, toggles: {} } },
			redacted: false,
			via: 'startup',
			actor: null,
			request_id: null,
			build_sha: 'cb761820aa',
			db_user: 'devuser',
			application_name: 'claude-proxy',
			detail: { defaults_changed: ['setting:dossier.batch_sessions'] }
		}
	];

	const { Story } = defineMeta({
		title: 'Settings/ChangeLogTable',
		component: ChangeLogTable,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	args={{ rows: ROWS }}
	play={async ({ canvasElement }) => {
		await expect(canvasElement.textContent).toContain('LAST-LINE');
		await expect(canvasElement.textContent).toContain('verifier');
		await expect(canvasElement.textContent).toContain('soft_delete');
		await expect(canvasElement.textContent).toContain('recorded when mike started');
	}}
/>
