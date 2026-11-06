<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, waitFor } from 'storybook/test';
	import AnalysisToolsCard from './AnalysisToolsCard.svelte';
	import 'augmented-ui/augmented-ui.min.css';
	import type { Finding, ToolUseDto, ToolUsesResponse } from '$lib/types';

	const SESSION_ID = 'a1b2c3d4-e5f6-7890-abcd-ef1234567890';
	const STATIC_NOW = new Date('2026-07-02T12:00:00Z').getTime();

	const FINDINGS: Finding[] = [
		{
			request_id: 42,
			session_id: SESSION_ID,
			category: 'justifying_shortcuts',
			analyzer: 'justifying_shortcuts_analyzer',
			advice:
				'The assistant described a partial implementation as "should work" without having run ' +
				'it, compiled it, or committed it. Completion claims require observable evidence in the ' +
				'same turn.',
			matched_text:
				"I've updated the component to use the correct CSS variable. It should work now.",
			match_range: [0, 60],
			other_hits: 0,
			created_at: '2026-06-26T14:32:01Z',
			request: {
				model: 'claude-sonnet-4-5',
				status: 'Complete',
				path: '/v1/messages',
				project: '/projects/temet-proxy',
				human_prompt: 'Fix the layout shift on the active row border',
				created_at: '2026-06-26T14:31:55Z'
			}
		}
	];

	const TOOL_USES: ToolUseDto[] = [
		{
			request_id: 42,
			session_id: SESSION_ID,
			created_at: '2026-06-26T14:31:56Z',
			tool_id: 'toolu_01AnalysisTools',
			tool_name: 'Edit',
			input: {
				file_path: 'src/lib/components/shell/SessionOverview.svelte',
				old_string: 'a',
				new_string: 'b'
			},
			file_path: 'src/lib/components/shell/SessionOverview.svelte',
			ord: 0
		}
	];

	function stubToolUsesFetch(): () => void {
		const orig = globalThis.fetch;
		globalThis.fetch = (async (input: RequestInfo | URL) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (url.includes('/tool_uses')) {
				const body: ToolUsesResponse = {
					session_id: 'story-session',
					offset: 0,
					limit: 1000,
					rows: TOOL_USES,
					total_count: TOOL_USES.length
				};
				return new Response(JSON.stringify(body), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404, statusText: 'Not Found' });
		}) as typeof fetch;
		return () => {
			globalThis.fetch = orig;
		};
	}

	const { Story } = defineMeta({
		title: 'Shell/AnalysisToolsCard',
		component: AnalysisToolsCard,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			const restore = stubToolUsesFetch();
			return { restore };
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const card = canvasElement.querySelector('.atc-card');
		await expect(card).not.toBeNull();

		const analysisToggle = canvas.getByRole('button', { name: /flagged responses/i });
		await analysisToggle.click();
		await expect(canvas.getByText(/should work now/)).toBeInTheDocument();

		await waitFor(() => expect(canvas.getByText('Tool uses')).toBeInTheDocument());
		const toolUsesHeading = canvas.getByRole('button', { name: /tool uses/i });
		const toolUsesSection = toolUsesHeading.closest('section');
		await expect(toolUsesSection?.getAttribute('data-augmented-ui')).toBeNull();
	}}
>
	{#snippet template()}
		<div style="max-width: 760px; background: var(--surface-bg); padding: 16px;">
			<AnalysisToolsCard
				sessionId={SESSION_ID}
				now={STATIC_NOW}
				findings={FINDINGS}
				findingsLoading={false}
				findingsError={null}
				analysisHref={`/analysis?session=${SESSION_ID}`}
			/>
		</div>
	{/snippet}
</Story>
