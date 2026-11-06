<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import DiagnosticsCard from './DiagnosticsCard.svelte';
	import type { Diagnostic } from '$lib/types';

	function buildDiagnostic(overrides: Partial<Diagnostic> = {}): Diagnostic {
		return {
			category: 'justifying_shortcuts',
			analyzer: 'weasel-v1',
			advice: 'Avoid framing scope reductions as inherently beneficial.',
			matched_text: 'pragmatically, for now',
			match_range: [0, 22],
			other_hits: 0,
			created_at: '2026-05-04T10:00:01Z',
			...overrides
		};
	}

	const { Story } = defineMeta({
		title: 'Components/Detail/DiagnosticsCard',
		component: DiagnosticsCard,
		tags: ['autodocs'],
		parameters: {
			a11y: { config: { rules: [{ id: 'nested-interactive', enabled: false }] } }
		}
	});
</script>

<Story name="MixedSeverity">
	{#snippet template()}
		<DiagnosticsCard
			diagnostics={[
				buildDiagnostic({
					category: 'justifying_shortcuts',
					matched_text: 'pragmatically speaking',
					other_hits: 2
				}),
				buildDiagnostic({
					category: 'deflecting_blame',
					analyzer: 'weasel-v1',
					advice: 'Avoid language attributing problems to external causes.',
					matched_text: 'the pre-existing state of the codebase',
					match_range: [0, 38],
					other_hits: 0
				}),
				buildDiagnostic({
					category: 'avoiding_work',
					analyzer: 'weasel-v1',
					advice: 'Avoid punting effort to a future iteration.',
					matched_text: 'straightforward to revisit later',
					match_range: [0, 32],
					other_hits: 0
				}),
				buildDiagnostic({
					category: 'faking_understanding',
					analyzer: 'weasel-v1',
					advice: 'Avoid claiming convention without evidence.',
					matched_text: 'consistent with standard patterns',
					match_range: [0, 33],
					other_hits: 0
				}),
				buildDiagnostic({
					category: 'overselling',
					matched_text: 'robust and production-ready',
					match_range: [0, 26],
					other_hits: 3
				})
			]}
		/>
	{/snippet}
</Story>
