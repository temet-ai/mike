<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import { CircleCheck, CircleDashed, CircleX, EyeOff, Info, TriangleAlert } from '@lucide/svelte';
	import StatusMark from './status-mark.svelte';

	const { Story } = defineMeta({
		title: 'UI/StatusMark',
		component: StatusMark,
		tags: ['autodocs']
	});

	const MARKS = [
		{ tone: 'success', icon: CircleCheck, label: 'fixed' },
		{ tone: 'danger', icon: CircleX, label: 'error' },
		{ tone: 'warning', icon: TriangleAlert, label: 'warning' },
		{ tone: 'info', icon: Info, label: 'hint' },
		{ tone: 'strong', icon: EyeOff, label: 'ignored' },
		{ tone: 'quiet', icon: CircleDashed, label: 'open' }
	] as const;
</script>

<Story
	name="Tones"
	play={async ({ canvasElement }) => {
		const marks = Array.from(
			canvasElement.querySelectorAll<HTMLElement>('[data-slot="status-mark"]')
		);
		await expect(marks).toHaveLength(6);
		const lefts = marks.map(
			(mark) =>
				mark.querySelector<HTMLElement>('.status-mark__label')!.getBoundingClientRect().left -
				mark.parentElement!.getBoundingClientRect().left
		);
		for (const left of lefts) await expect(left).toBeCloseTo(lefts[0]!, 1);
		for (const mark of marks) {
			await expect(mark.querySelector('svg')).toHaveAttribute('aria-hidden', 'true');
		}
	}}
>
	{#snippet template()}
		<div class="flex flex-col gap-2">
			{#each MARKS as mark (mark.tone)}
				<div class="w-48">
					<StatusMark icon={mark.icon} label={mark.label} tone={mark.tone} />
				</div>
			{/each}
		</div>
	{/snippet}
</Story>
