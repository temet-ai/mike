<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent } from 'storybook/test';
	import BoardColumn from './BoardColumn.svelte';
	import NodeChip from './NodeChip.svelte';

	const { Story } = defineMeta({
		title: 'Shared/Board/BoardColumn',
		component: BoardColumn,
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	let agentsOpen = $state(true);
</script>

<Story
	name="FourColumns"
	play={async ({ canvas }) => {
		const control = canvas.getByRole('button', { name: /collapse agents/i });
		await expect(control).toHaveAttribute('aria-expanded', 'true');
		await userEvent.click(control);
		await expect(control).toHaveAttribute('aria-expanded', 'false');
		await expect(canvas.getByRole('button', { name: /expand agents/i })).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div data-design="funk" style="display: flex; background: var(--surface-bg);">
			<div style="width: 176px;">
				<BoardColumn label="Session" count={1} state="busy" height={220}>
					<NodeChip
						kind="session"
						name="main session"
						sub="busy"
						state="busy"
						icon="session"
						x={16}
						y={48}
					/>
				</BoardColumn>
			</div>
			<div style="width: 340px;">
				<BoardColumn
					label="Agents"
					count={3}
					summary="1 active"
					state="busy"
					control={agentsOpen ? 'collapse' : 'expand'}
					onToggle={() => (agentsOpen = !agentsOpen)}
					height={220}
				>
					{#if agentsOpen}
						<NodeChip
							kind="agent"
							name="rust-developer"
							sub="busy"
							state="busy"
							icon="agent"
							x={16}
							y={48}
						/>
					{/if}
				</BoardColumn>
			</div>
			<div style="width: 260px;">
				<BoardColumn label="Mike · Proxy · the Boundary" count={16} state="quiet" height={220}>
					<NodeChip
						kind="service"
						name="composer"
						sub="quiet"
						state="quiet"
						icon="composer"
						x={16}
						y={48}
					/>
				</BoardColumn>
			</div>
			<div style="width: 220px;">
				<BoardColumn label="Outside the Boundary" count={2} state="recent" height={220}>
					<NodeChip
						kind="outside"
						name="Anthropic"
						sub="response"
						state="response"
						icon="model"
						x={16}
						y={48}
					/>
				</BoardColumn>
			</div>
		</div>
	{/snippet}
</Story>
