<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import WakeStateChip from './WakeStateChip.svelte';

	const { Story } = defineMeta({
		title: 'Shell/WakeStateChip',
		component: WakeStateChip,
		tags: ['autodocs']
	});

	const ROWS = [
		{ delivery_channel: 'wake' as const, wake_state: 'pending', deliver_after: '2026-09-25T14:02:31Z' },
		{ delivery_channel: 'wake' as const, wake_state: 'pushed' },
		{ delivery_channel: 'append' as const, delivered_request_id: 431907 },
		{ delivery_channel: 'wake' as const, reverted_at: '2026-09-25T14:05:00Z', revert_reason: 'expired_ttl' }
	];
</script>

<Story
	name="WakeStateChip"
	play={async ({ canvasElement }) => {
		const states = [...canvasElement.querySelectorAll('[data-wake-axis="state"]')].map((el) => el.textContent?.trim());
		await expect(states).toEqual([
			'pending until 14:02:31Z',
			'pushed',
			'carried by request 431907',
			'dropped: expired_ttl'
		]);
	}}
>
	{#snippet template()}
		<div class="flex flex-col gap-2 bg-[var(--surface-bg)] p-4">
			{#each ROWS as fields, i (i)}
				<WakeStateChip {fields} />
			{/each}
		</div>
	{/snippet}
</Story>
