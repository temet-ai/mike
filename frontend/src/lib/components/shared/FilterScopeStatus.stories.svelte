<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import FilterScopeStatus from './FilterScopeStatus.svelte';

	const { Story } = defineMeta({
		title: 'Shared/FilterScopeStatus',
		component: FilterScopeStatus,
		tags: ['autodocs']
	});
</script>

<Story
	name="AppliedWithUnappliedEdit"
	args={{
		scopeLabel: 'project "temet-proxy", tool "Bash"',
		shownCount: 42,
		totalCount: 187,
		itemNoun: 'tool use',
		dirty: true
	}}
	play={async ({ canvasElement }) => {
		const scope = canvasElement.querySelector('[data-slot="filter-scope-status-scope"]');
		expect(scope?.textContent).toContain('project "temet-proxy", tool "Bash"');
		expect(scope?.textContent).toContain('42');
		expect(scope?.textContent).toContain('187');
		expect(scope?.textContent).toContain('tool uses');
		const dirty = canvasElement.querySelector('[data-slot="filter-scope-status-dirty"]');
		expect(dirty?.textContent).toContain('Selection changed');
	}}
>
	{#snippet template(args)}
		<div class="p-4" style="max-width: 480px; background: var(--surface-card);">
			<FilterScopeStatus {...args} />
		</div>
	{/snippet}
</Story>
