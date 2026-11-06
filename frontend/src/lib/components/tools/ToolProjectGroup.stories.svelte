<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, expect, waitFor } from 'storybook/test';
	import ToolProjectGroupHarness from './__harness__/ToolProjectGroupHarness.svelte';
	import type { ToolProjectGroupRow } from './ToolProjectGroup.svelte';
	import type { ToolUsesFeedRow } from '$lib/types';

	function buildRow(overrides: Partial<ToolUsesFeedRow> & { use_id: number }): ToolUsesFeedRow {
		return {
			use_id: overrides.use_id,
			wire_tool_use_id: `tool_use_${overrides.use_id}`,
			request_id: overrides.request_id ?? 1000 + overrides.use_id,
			session_id: overrides.session_id ?? '01J9X4QF7H8K2N3P5R6T7V8W9A',
			project: overrides.project !== undefined ? overrides.project : '/Users/dev/temet-proxy',
			tool_name: overrides.tool_name ?? 'Bash',
			created_at: overrides.created_at ?? '2026-06-17T10:00:00Z',
			ord: overrides.ord ?? overrides.use_id,
			input: overrides.input ?? { command: 'ls -la' },
			file_path: overrides.file_path ?? null,
			is_error: overrides.is_error,
			result_id: overrides.result_id,
			error_content: overrides.error_content,
			error_content_bytes: overrides.error_content_bytes,
			error_content_sha256: overrides.error_content_sha256
		};
	}

	const ROWS: ToolUsesFeedRow[] = [
		buildRow({
			use_id: 1,
			tool_name: 'Bash',
			input: { command: 'cargo nextest run --all' },
			created_at: '2026-06-17T10:05:00Z'
		}),
		buildRow({
			use_id: 2,
			tool_name: 'Edit',
			file_path: '/Users/dev/temet-proxy/frontend/app.css',
			input: {
				file_path: '/Users/dev/temet-proxy/frontend/app.css',
				old_string: 'old',
				new_string: 'new'
			},
			is_error: true,
			result_id: 501,
			error_content: 'String to replace not found in file.',
			error_content_bytes: 34,
			error_content_sha256: '3f9ac8d1e2b4567890abcdef1234567890abcdef1234567890abcdef123456',
			created_at: '2026-06-17T10:00:00Z'
		})
	];

	const GROUP: ToolProjectGroupRow = {
		key: 'temet-proxy',
		project: '/Users/dev/temet-proxy',
		isUngrouped: false,
		name: 'temet-proxy',
		path: '/Users/dev/temet-proxy',
		rows: ROWS,
		failedCount: 1,
		colorClass: 'tpg-chip--1'
	};

	const { Story } = defineMeta({
		title: 'Components/Tools/ToolProjectGroup',
		component: ToolProjectGroupHarness,
		tags: ['autodocs'],
		parameters: {
			layout: 'padded',
			docs: {
				description: {
					component:
						'Project-grouped tool-call card for /tools: header (collapse, name/path, loaded-count chrome) and a row list with disclosure-driven input/error panes. Chrome shared with ProjectSessionGroup (chip ramp, clip shell, collapse shape-morph, sidebar:project: collapse-key scheme).'
				}
			}
		}
	});
</script>

<Story
	name="Populated"
	tags={['data-driven', 'interactive']}
	args={{ group: GROUP, now: Date.parse('2026-06-17T10:10:00Z') }}
	play={async ({ canvasElement }) => {
		const head = canvasElement.querySelector('.tpg-head') as HTMLElement;
		const toggle = canvasElement.querySelector('.tpg-toggle') as HTMLElement;
		await expect(head).toBeTruthy();
		await expect(head.textContent).toContain('temet-proxy');

		const counts = canvasElement.querySelector('.tpg-counts') as HTMLElement;
		await expect(counts.textContent).toBe('2 calls, 1 failed');

		await expect(canvasElement.querySelectorAll('.tpg-row').length).toBe(2);
		const failedBadge = canvasElement.querySelector('.tpg-failed');
		await expect(failedBadge).toBeTruthy();
		await expect(failedBadge?.textContent).toBe('Failed');

		await userEvent.click(toggle);
		await waitFor(() => expect(toggle).toHaveAttribute('aria-expanded', 'false'));
		await waitFor(() => expect(canvasElement.querySelector('.tpg-table')).toBeNull());
		await userEvent.click(toggle);
		await waitFor(() => expect(toggle).toHaveAttribute('aria-expanded', 'true'));
		await expect(canvasElement.querySelector('.tpg-table')).toBeTruthy();

		const rows = canvasElement.querySelectorAll('.tpg-row');
		const discloseButtons = canvasElement.querySelectorAll('.tpg-row__disclose');
		await userEvent.click(discloseButtons[1] as HTMLButtonElement);
		const errorPre = await waitFor(() => {
			const el = canvasElement.querySelector('.tpg-pre--error');
			if (!el) throw new Error('error pre not yet rendered');
			return el as HTMLElement;
		});
		await expect(errorPre.textContent).toBe('String to replace not found in file.');
		const labels = Array.from(canvasElement.querySelectorAll('.tpg-section__label')).map(
			(el) => el.textContent
		);
		await expect(labels).toEqual(['Error', 'Input']);
		const errorHead = canvasElement.querySelector('.tpg-section__head') as HTMLElement;
		await expect(errorHead.textContent).toContain('34 B');
		const hashChip = errorHead.querySelector('.tpg-meta--hash') as HTMLElement;
		await expect(hashChip).toHaveTextContent('sha256');

		await userEvent.click(discloseButtons[0] as HTMLButtonElement);
		await waitFor(() => {
			if (canvasElement.querySelectorAll('.tpg-pre').length < 3) {
				throw new Error('clean row input not yet rendered');
			}
		});
		const cleanRowBody = rows[0]?.querySelector('.tpg-row__body') as HTMLElement;
		await expect(cleanRowBody.querySelector('.tpg-section__label')).toBeNull();
		await expect(cleanRowBody.querySelector('.tpg-pre')?.textContent).toContain('cargo nextest');
	}}
>
	{#snippet template(args)}
		<ToolProjectGroupHarness {...args} />
	{/snippet}
</Story>
