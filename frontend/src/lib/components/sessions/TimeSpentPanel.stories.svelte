<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within, userEvent, waitFor } from 'storybook/test';
	import 'augmented-ui/augmented-ui.min.css';
	import TimeSpentPanel from './TimeSpentPanel.svelte';
	import { stubFetchJsonRouter, typed } from '$lib/test-support/storyFetchStubs';
	import { timeSpentFromUrl } from '$lib/components/__fixtures__/timeSpent';
	import type { TimeSpentDto } from '$lib/types';

	const { Story } = defineMeta({
		title: 'Components/Sessions/TimeSpentPanel',
		component: TimeSpentPanel,
		tags: ['autodocs'],
		parameters: {
			layout: 'padded',
			docs: {
				description: {
					component:
						'The "Where the time went" panel (GET /api/time_spent) as it sits on the Sessions list: model time against waiting time, builds while other sessions build, where the waiting went by kind, and the slowest commands. Spec: artifacts/2026-10-12/time-spent-panel/spec.md. Numbers: temet-proxy, last 14 days, main sessions; per-kind answer counts and the command rows are illustrative. On a session page the same panel is rendered with scope "session".'
				}
			}
		}
	});

	const SECTION_KEY = 'sessions:time-spent';
	const PROJECTS = [
		{ value: '/mnt/data/projects/temet/temet-proxy', label: 'temet-proxy' },
		{ value: '/mnt/data/projects/holdfast', label: 'holdfast' }
	];
</script>

<script lang="ts">
	import { setContext } from 'svelte';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import { createCollapseStore } from '$lib/stores/collapseState.svelte';

	setContext(COLLAPSE_STORE_KEY, createCollapseStore());
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			localStorage.clear();
			localStorage.setItem('proxy-inspector:collapsed', JSON.stringify([SECTION_KEY]));
			stubFetchJsonRouter((_method, url) => typed<TimeSpentDto>(timeSpentFromUrl(url)));
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await waitFor(() => expect(canvas.getByText('108 h')).toBeInTheDocument());
		await expect(canvas.getByRole('button', { name: /where the time went/i })).toHaveAttribute(
			'aria-expanded',
			'true'
		);
		await expect(
			canvas.getByText('108 h model · 116 h waiting · last 14 days')
		).toBeInTheDocument();
		await expect(canvas.getByText(/11 times as long/)).toBeInTheDocument();

		const rustTests = canvas.getByRole('button', { name: /^Rust tests/ });
		await expect(rustTests).toHaveAttribute('aria-pressed', 'false');
		await expect(canvas.queryByRole('button', { name: /^MCP tools/ })).not.toBeInTheDocument();

		await expect(canvas.getByText('scripts/sb-test')).toBeInTheDocument();
		await expect(canvas.getByRole('button', { name: 'Show 20 more' })).toBeInTheDocument();
		await userEvent.click(canvas.getByRole('button', { name: 'Show 20 more' }));
		await expect(await canvas.findByText('git log --oneline -20')).toBeInTheDocument();
		await expect(canvas.queryByRole('button', { name: 'Show 20 more' })).not.toBeInTheDocument();

		await userEvent.click(rustTests);
		await expect(rustTests).toHaveAttribute('aria-pressed', 'true');
		await waitFor(() => expect(canvas.queryByText('scripts/sb-test')).not.toBeInTheDocument());
		await expect(canvas.getByText('cargo nextest run --workspace')).toBeInTheDocument();
		await expect(canvas.getByText('Slowest commands · Rust tests')).toBeInTheDocument();

		await userEvent.click(canvas.getByRole('button', { name: 'Show every kind' }));
		await expect(rustTests).toHaveAttribute('aria-pressed', 'false');
		await waitFor(() => expect(canvas.getByText('scripts/sb-test')).toBeInTheDocument());
		localStorage.removeItem('proxy-inspector:collapsed');
	}}
>
	{#snippet template()}
		<div class="tsp-host">
			<TimeSpentPanel scope="list" projects={PROJECTS} />
		</div>
	{/snippet}
</Story>

<style>
	.tsp-host {
		max-width: 1312px;
	}
</style>
