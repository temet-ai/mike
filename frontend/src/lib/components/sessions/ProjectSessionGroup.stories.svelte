<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import ProjectSessionGroupHarness from './__harness__/ProjectSessionGroupHarness.svelte';
	import { type ProjectGroupRow } from './ProjectSessionGroup.svelte';
	import 'augmented-ui/augmented-ui.min.css';
	import { buildSessionSummary, buildDetailMetrics } from '$lib/components/__fixtures__/session';

	const STATIC_NOW = Date.parse('2026-06-21T20:00:00Z');

	const SESSIONS = Array.from({ length: 12 }, (_, i) =>
		buildSessionSummary({
			id: `66666666-0000-0000-0000-0000000000${String(i + 1).padStart(2, '0')}`,
			project: '/Users/dev/big-project',
			title: `Big project session ${i + 1}`,
			last_status: 'Complete',
			request_count: 4 + i,
			first_seen: `2026-06-21T${String(8 + i).padStart(2, '0')}:00:00Z`,
			last_seen: `2026-06-21T${String(8 + i).padStart(2, '0')}:10:00Z`,
			pinned: i === 0,
			pending_inbox_count: i === 1 ? 3 : undefined,
			originating_operator_prompt: `Task number ${i + 1} for the big project.`
		})
	);

	const GROUP: ProjectGroupRow = {
		key: 'big-project',
		project: '/Users/dev/big-project',
		isUngrouped: false,
		name: 'big-project',
		path: '/Users/dev/big-project',
		sessions: SESSIONS,
		colorClass: 'ssn-chip--1'
	};

	const DETAIL_METRICS = new Map(SESSIONS.map((s) => [s.id, buildDetailMetrics()]));

	const { Story } = defineMeta({
		title: 'Components/Sessions/ProjectSessionGroup',
		component: ProjectSessionGroupHarness,
		tags: ['autodocs'],
		parameters: {
			layout: 'padded',
			docs: {
				description: {
					component:
						'Browse-mode project card: header (collapse, name/path, project pin) and a session table with hover-revealed detail bands, a >10-session "show more" footer, and per-project "load older sessions" pagination.'
				}
			}
		}
	});
</script>

<Story
	name="Populated"
	tags={['data-driven', 'interactive']}
	args={{ group: GROUP, now: STATIC_NOW, detailMetrics: DETAIL_METRICS }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const groupBtn = canvasElement.querySelector('.ssn-project-head') as HTMLElement;
		const toggleBtn = canvasElement.querySelector('.ssn-project-toggle') as HTMLElement;
		await expect(groupBtn).toBeTruthy();
		await expect(toggleBtn).toBeTruthy();
		await expect(groupBtn.textContent).toContain('big-project');

		const rowsBefore = canvasElement.querySelectorAll('.ssn-row-wrap');
		await expect(rowsBefore.length).toBe(10);
		await userEvent.click(toggleBtn);
		await waitFor(() => expect(toggleBtn).toHaveAttribute('aria-expanded', 'false'));
		await waitFor(() => expect(canvasElement.querySelectorAll('.ssn-row-wrap').length).toBe(0));

		await new Promise((r) => setTimeout(r, 260));
		await expect(getComputedStyle(groupBtn).getPropertyValue('--aug-tl').trim()).toBe('4px');

		await userEvent.click(toggleBtn);
		await waitFor(() => expect(toggleBtn).toHaveAttribute('aria-expanded', 'true'));
		await expect(canvasElement.querySelectorAll('.ssn-row-wrap').length).toBe(10);

		const clipAtFlip = getComputedStyle(groupBtn).getPropertyValue('--aug-tl').trim();
		await expect(clipAtFlip).not.toBe('14px');
		await new Promise((r) => setTimeout(r, 260));
		await expect(getComputedStyle(groupBtn).getPropertyValue('--aug-tl').trim()).toBe('14px');

		const firstRow = canvasElement.querySelector('.ssn-row-wrap') as HTMLElement;
		await userEvent.hover(firstRow);
		await expect(firstRow.querySelector('.ssn-detail-meta')?.textContent).toContain(
			'claude-opus-4-7'
		);

		const showMoreBtn = canvasElement.querySelector('button.ssn-showmore') as HTMLButtonElement;
		await expect(showMoreBtn).toBeTruthy();
		await expect(showMoreBtn.textContent).toContain('Show 2 more sessions');
		await userEvent.click(showMoreBtn);
		await waitFor(() => expect(canvasElement.querySelectorAll('.ssn-row-wrap').length).toBe(12));
		await expect(showMoreBtn.textContent).toContain('Show fewer');

		const rowPinBtn = firstRow.querySelector(
			'button[aria-label="Unpin session"]'
		) as HTMLButtonElement;
		await expect(rowPinBtn).toBeTruthy();
		await userEvent.click(rowPinBtn);
		await waitFor(() =>
			expect(firstRow.querySelector('button[aria-label="Pin session"]')).toBeTruthy()
		);

		const projectPinBtn = canvas.getByRole('button', { name: 'Pin project' });
		await userEvent.click(projectPinBtn);
		await waitFor(() => expect(canvas.getByRole('button', { name: 'Unpin project' })).toBeTruthy());

		const loadOlderBtn = canvasElement.querySelector('.ssn-load-older') as HTMLButtonElement;
		await expect(
			loadOlderBtn,
			'fully-expanded, non-exhausted project should offer "Load older sessions"'
		).toBeTruthy();
		const rowCountBeforeLoadOlder = canvasElement.querySelectorAll('.ssn-row-wrap').length;
		await userEvent.click(loadOlderBtn);
		await waitFor(() =>
			expect(canvasElement.querySelectorAll('.ssn-row-wrap').length).toBe(
				rowCountBeforeLoadOlder + 1
			)
		);
		await waitFor(() => expect(canvasElement.querySelector('.ssn-load-older')).toBeNull());

		const proofBox = canvasElement.querySelector('.ssn-table-proof') as HTMLElement;
		await expect(proofBox).toBeTruthy();
		await expect(getComputedStyle(proofBox).clipPath).not.toBe('none');
		await expect(getComputedStyle(proofBox).getPropertyValue('--aug-bl').trim()).toBe('12px');
		await expect(getComputedStyle(proofBox).getPropertyValue('--aug-br').trim()).toBe('12px');

		const realTable = canvasElement.querySelector('.ssn-table') as HTMLElement;
		await expect(realTable).toBeTruthy();
		await expect(realTable.getAttribute('data-augmented-ui')).toBe('bl-clip br-clip border');
	}}
>
	{#snippet template(args)}
		<ProjectSessionGroupHarness {...args} />
		<div
			class="ssn-table-proof"
			data-augmented-ui="bl-clip br-clip border"
			style="--edge: 12px; --aug-bl: var(--edge); --aug-br: var(--edge); --aug-border-all: 1px; --aug-border-bg: var(--border-default);"
			aria-hidden="true"
		></div>
	{/snippet}
</Story>

<style>
	.ssn-table-proof {
		margin-top: 24px;
		width: 480px;
		height: 96px;
		background: var(--surface-card);
		box-shadow:
			inset 0 1px 0 var(--surface-shine),
			var(--shadow-sm);
	}
</style>
