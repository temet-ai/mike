<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent } from 'storybook/test';
	import NodeChip from './NodeChip.svelte';

	const { Story } = defineMeta({
		title: 'Shared/Board/NodeChip',
		component: NodeChip,
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	let sessionSelected = $state(false);
</script>

<Story
	name="Kinds"
	play={async ({ canvas }) => {
		const session = canvas.getByRole('button', { name: /inspect main session/i });
		await expect(session).toHaveAttribute('aria-pressed', 'false');
		await userEvent.click(session);
		await expect(session).toHaveAttribute('aria-pressed', 'true');
		await userEvent.click(session);
		await expect(session).toHaveAttribute('aria-pressed', 'false');

		await expect(canvas.getByText('peer session')).toBeVisible();
		await expect(canvas.getByText('rust-developer')).toBeVisible();
		await expect(canvas.getByText('3 subagents')).toBeVisible();
		await expect(canvas.getByText('Bash')).toBeVisible();
		await expect(canvas.getByText('composer')).toBeVisible();
		await expect(canvas.getByText('Anthropic')).toBeVisible();
		await expect(canvas.getByText('operator shell')).toBeVisible();
	}}
>
	{#snippet template()}
		<div
			data-design="funk"
			style="position: relative; width: 900px; height: 220px; background: var(--surface-bg); padding: 16px;"
		>
			<NodeChip
				kind="session"
				name="main session"
				sub={sessionSelected ? 'selected' : 'busy'}
				state={sessionSelected ? 'selected' : 'busy'}
				icon="session"
				x={0}
				y={0}
				onSelect={() => (sessionSelected = !sessionSelected)}
			/>
			<NodeChip
				kind="peer"
				name="peer session"
				sub="quiet"
				state="quiet"
				icon="peer"
				x={200}
				y={0}
			/>
			<NodeChip
				kind="outside"
				name="Anthropic"
				sub="response"
				state="response"
				icon="model"
				x={368}
				y={0}
			/>
			<NodeChip
				kind="agent"
				name="rust-developer"
				sub="recent"
				state="recent"
				icon="agent"
				x={0}
				y={72}
			/>
			<NodeChip
				kind="group"
				name="3 subagents"
				sub="pending"
				state="pending"
				icon="group"
				x={170}
				y={72}
			/>
			<NodeChip kind="tool" name="Bash" sub="error" state="error" icon="tool" x={340} y={72} />
			<NodeChip
				kind="service"
				name="composer"
				sub="hot"
				state="hot"
				icon="composer"
				x={520}
				y={72}
			/>
			<NodeChip
				kind="terminal"
				name="operator shell"
				sub="muted"
				state="quiet"
				icon="terminal"
				muted
				x={0}
				y={130}
			/>
		</div>
	{/snippet}
</Story>
