<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import SeatDecisions from './SeatDecisions.svelte';

	const { Story } = defineMeta({
		title: 'Fleet/SeatDecisions',
		component: SeatDecisions,
		tags: ['autodocs'],
		argTypes: { seat: { control: false }, modes: { control: false } }
	});
</script>

<Story
	name="Default"
	tags={['interactive']}
	args={{
		seat: { effort: { work: 620, machinery: 210, waiting: 170 }, mail: { held: 7, woken: 12 } },
		modes: { effort: 'on', wake_check: 'off' }
	}}
	play={async ({ canvasElement }) => {
		const blocks = Array.from(
			canvasElement.querySelectorAll<HTMLElement>('[data-slot="seat-decisions"]')
		);
		await expect(blocks).toHaveLength(2);
		const [live, empty] = blocks;
		await expect(live?.textContent?.replace(/\s+/g, ' ')).toContain(
			'work 62% · machinery 21% · waiting 17%'
		);
		await expect(live?.querySelector('[data-block="mail"]')?.textContent).toContain('off');
		await expect(live?.querySelector('[data-block="mail"]')?.textContent).toContain('12');
		await expect(live?.querySelector('[data-block="effort"]')?.textContent).not.toContain('off');
		await expect(empty?.querySelector('[data-block="effort"]')?.textContent).toContain('—');
		await expect(canvasElement.textContent).toContain('Figures arrive when the run closes.');

		const heightLive = live?.querySelector('[data-block="effort"]')?.getBoundingClientRect().height;
		const heightEmpty = empty
			?.querySelector('[data-block="effort"]')
			?.getBoundingClientRect().height;
		await expect(heightEmpty).toBe(heightLive);
	}}
>
	{#snippet template(args)}
		<div
			style="width: 22rem; display: flex; flex-direction: column; gap: var(--space-4); background: var(--surface-2); padding: var(--space-3);"
		>
			<SeatDecisions {...args} />
			<SeatDecisions seat={null} modes={null} />
			<SeatDecisions seat={null} unavailableReason="Figures arrive when the run closes." />
		</div>
	{/snippet}
</Story>
