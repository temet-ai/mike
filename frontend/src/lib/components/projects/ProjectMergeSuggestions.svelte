<script lang="ts">
	import { Copy } from '@lucide/svelte';
	import type { ProjectMergeSuggestion } from '$lib/api';
	import { Button } from '$lib/components/ui/button';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';

	interface Props {
		suggestions: readonly ProjectMergeSuggestion[];
		onMerge: (suggestion: ProjectMergeSuggestion) => void;
	}

	const { suggestions, onMerge }: Props = $props();

	function likelihood(probability: number): string {
		return `${Math.round(probability * 100)}% likely the same project`;
	}
</script>

{#if suggestions.length > 0}
	<div data-slot="project-merge-suggestions">
		<SectionCard
			title="Possible duplicates"
			titleIcon={Copy}
			titleTooltip="Projects that look like the same folder or copies of one another. Nothing merges until you confirm."
		>
			<ul class="pms-list">
				{#each suggestions as suggestion (`${suggestion.source_path}\n${suggestion.target_uuid}`)}
					<li class="pms-row" data-slot="project-merge-suggestion">
						<div class="pms-pair">
							<span class="pms-path" data-slot="project-merge-suggestion-source">
								{suggestion.source_path}
							</span>
							<span class="pms-path pms-path--target" data-slot="project-merge-suggestion-target">
								{suggestion.target_path}
							</span>
						</div>
						<span class="pms-reason" data-slot="project-merge-suggestion-reason">
							{likelihood(suggestion.probability)}
						</span>
						<Button
							variant="ghost"
							onclick={() => onMerge(suggestion)}
							aria-label={`Merge ${suggestion.source_path} into ${suggestion.target_path}`}
							data-slot="project-merge-suggestion-merge"
						>
							Merge
						</Button>
					</li>
				{/each}
			</ul>
		</SectionCard>
	</div>
{/if}

<style>
	.pms-list {
		display: flex;
		flex-direction: column;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.pms-row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-3);
		padding: var(--space-2) 0;
	}
	.pms-row + .pms-row {
		border-top: 1px solid var(--border-subtle);
	}
	.pms-pair {
		display: flex;
		min-width: 0;
		flex: 1 1 20rem;
		flex-direction: column;
		gap: var(--space-1);
	}
	.pms-path {
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--text-1);
		overflow-wrap: anywhere;
	}
	.pms-path--target {
		color: var(--text-2);
	}
	.pms-reason {
		font-size: var(--text-sm);
		color: var(--text-2);
	}
</style>
