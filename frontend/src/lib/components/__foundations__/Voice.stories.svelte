<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';

	const { Story } = defineMeta({
		title: 'Foundations/Voice',
		tags: ['autodocs']
	});

	const ROWS = [
		{ never: 'Oops! Something went wrong 😢', always: 'WRITE FAILED — ERR 507' },
		{
			never: 'Your session expired — please log in again!',
			always: 'SESSION 0x4A2F EXPIRED · 14:02:37'
		},
		{ never: 'About 3k tokens used', always: '2,847 tokens' },
		{ never: 'N/A', always: '—' }
	] as const;
</script>

<Story name="Voice">
	{#snippet template()}
		<div class="fk-plate" data-design="funk">
			<p class="fk-plate-title machine">Voice</p>

			<div class="fk-cols">
				<div class="machine fk-verdict fk-verdict--bad">Never</div>
				<div class="machine fk-verdict fk-verdict--good">Always</div>

				{#each ROWS as row (row.always)}
					<div class="fk-cell fk-cell--never">{row.never}</div>
					<div class="fk-cell fk-cell--always">{row.always}</div>
				{/each}
			</div>
		</div>
	{/snippet}
</Story>

<style>
	.fk-plate {
		display: flex;
		flex-direction: column;
		padding: var(--space-6);
		background: var(--bg);
		max-width: 760px;
	}
	.fk-plate-title {
		margin: 0 0 var(--space-4);
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.fk-cols {
		display: grid;
		grid-template-columns: 1fr 1fr;
		column-gap: var(--space-5);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
	}
	.fk-verdict {
		margin-bottom: var(--space-2);
		font-size: var(--text-caption);
	}
	.fk-verdict--bad {
		color: var(--red);
	}
	.fk-verdict--good {
		color: var(--green);
	}
	.fk-cell {
		padding: 6px 0;
		border-bottom: var(--hairline) solid var(--line);
	}
	.fk-cell--never {
		color: var(--text-3);
		text-decoration: line-through;
	}
	.fk-cell--always {
		color: var(--text-1);
	}
</style>
