<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect } from 'storybook/test';
	import SessionAnalysisBand from './SessionAnalysisBand.svelte';
	import type { Finding } from '$lib/types';

	const SESSION_ID = 'a1b2c3d4-e5f6-7890-abcd-ef1234567890';

	const FINDINGS: Finding[] = [
		{
			request_id: 42,
			session_id: SESSION_ID,
			category: 'justifying_shortcuts',
			analyzer: 'justifying_shortcuts_analyzer',
			advice:
				'The assistant described a partial implementation as "should work" without having run ' +
				'it, compiled it, or committed it. Completion claims require observable evidence in the ' +
				'same turn: a captured test run (exit code 0), a committed diff, or a live screenshot. ' +
				'Without that evidence, the claim is verification theater and should be challenged. ' +
				'Recommended action: ask the assistant to run the validation gate and paste the output.',
			matched_text:
				"I've updated the component to use the correct CSS variable. It should work now " +
				'and the layout shift should be resolved. The fix applies the `box-shadow: inset 2px 0 0 0 ' +
				'var(--interactive-active-border)` approach from the style guide.',
			match_range: [0, 217],
			other_hits: 2,
			created_at: '2026-06-26T14:32:01Z',
			request: {
				model: 'claude-sonnet-4-5',
				status: 'Complete',
				path: '/v1/messages',
				project: '/projects/temet-proxy',
				human_prompt: 'Fix the layout shift on the active row border',
				created_at: '2026-06-26T14:31:55Z'
			}
		},
		{
			request_id: 67,
			session_id: SESSION_ID,
			category: 'avoiding_work',
			analyzer: 'avoiding_work_analyzer',
			advice:
				'The assistant proposed creating a separate configuration file as a prerequisite ' +
				'instead of making the change directly. This is an avoidance pattern: the configuration ' +
				'is trivially inline and a separate file adds indirection without benefit. ' +
				'The tell is "I\'ll need to set up X first" where X is not actually required. ' +
				'Call out the prerequisite as unnecessary and ask for the direct solution.',
			matched_text:
				"To implement this properly I'll need to first create a central configuration " +
				'module that both components can import from. Once that is set up we can wire the ' +
				'feature through it. The configuration module will also let us support future ' +
				'variants without touching each component individually.',
			match_range: [0, 304],
			other_hits: 0,
			created_at: '2026-06-26T15:10:45Z',
			request: {
				model: 'claude-sonnet-4-5',
				status: 'Complete',
				path: '/v1/messages',
				project: '/projects/temet-proxy',
				human_prompt: 'Wire the session ID into the analysis band href',
				created_at: '2026-06-26T15:10:30Z'
			}
		},
		{
			request_id: 89,
			session_id: SESSION_ID,
			category: 'faking_understanding',
			analyzer: 'faking_understanding_analyzer',
			advice:
				'The response claimed understanding of the architecture without citing the relevant ' +
				'file, function, or code path. Phrases like "based on the existing pattern" or ' +
				'"following the established convention" without naming the pattern are a signal that ' +
				'the assistant is extrapolating rather than reading. Ask the assistant to quote the ' +
				'specific code path it is following and verify it exists.',
			matched_text:
				'Based on the existing pattern used elsewhere in the codebase, the correct approach ' +
				'here would be to follow the same convention as the other components. This is consistent ' +
				'with what is already established in the project.',
			match_range: [0, 213],
			other_hits: 1,
			created_at: '2026-06-26T16:05:30Z',
			request: {
				model: 'claude-sonnet-4-5',
				status: 'Complete',
				path: '/v1/messages',
				project: '/projects/temet-proxy',
				human_prompt: 'Add the enrolment fetch effect',
				created_at: '2026-06-26T16:05:10Z'
			}
		}
	];

	const { Story } = defineMeta({
		component: SessionAnalysisBand,
		title: 'Shell/SessionAnalysisBand'
	});
</script>

<Story
	name="WithFindings"
	args={{
		findings: FINDINGS,
		loading: false,
		error: null,
		analysisHref: `/analysis?session=${encodeURIComponent(SESSION_ID)}`,
		open: true
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const toggle = canvas.getByRole('button', { name: /flagged responses/i });
		await expect(toggle).toBeInTheDocument();
		await expect(toggle).toHaveAttribute('aria-expanded', 'true');

		const body = canvasElement.querySelector('#sab-body');
		await expect(body).toBeInTheDocument();

		await userEvent.click(toggle);
		await expect(toggle).toHaveAttribute('aria-expanded', 'false');
		const summary = canvas.getByText(/3 findings/i);
		await expect(summary).toBeInTheDocument();
		await expect(canvasElement.querySelector('#sab-body')).not.toBeInTheDocument();

		await userEvent.click(toggle);
		await expect(toggle).toHaveAttribute('aria-expanded', 'true');

		const matchedText = canvas.getByText(/It should work now/i);
		await expect(matchedText).toBeInTheDocument();

		const advice = canvas.getByText(/verification theater/i);
		await expect(advice).toBeInTheDocument();

		const badge = canvasElement.querySelector('.sab-category-badge');
		await expect(badge).toHaveTextContent(/justifying shortcuts/i);

		const checkName = canvasElement.querySelector('.sab-analyzer');
		await expect(checkName).toHaveTextContent(/justifying shortcuts/i);
		await expect(checkName?.textContent).not.toMatch(/_analyzer/i);

		const reqLink = canvas.getByRole('link', { name: /Go to request #42/i });
		await expect(reqLink).toBeInTheDocument();
		await expect(reqLink).toHaveAttribute(
			'href',
			`/sessions/${encodeURIComponent(SESSION_ID)}/requests/42`
		);

		const analysisLink = canvas.getByRole('link', { name: /view all flagged responses/i });
		await expect(analysisLink).toBeInTheDocument();
		await expect(analysisLink).toHaveAttribute(
			'href',
			`/analysis?session=${encodeURIComponent(SESSION_ID)}`
		);

		const otherHits = canvas.getByText(/\+2 other similar hits/i);
		await expect(otherHits).toBeInTheDocument();
	}}
/>
