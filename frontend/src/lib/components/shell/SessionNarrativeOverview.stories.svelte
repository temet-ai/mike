<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect } from 'storybook/test';
	import SessionNarrativeOverview from './SessionNarrativeOverview.svelte';
	import 'augmented-ui/augmented-ui.min.css';

	const SAMPLE_OVERVIEW =
		'This session moved the mailbox above the timeline and merged the steering ' +
		'controls into a single panel, per the operator-approved mockup.\n\n' +
		'It landed cleanly: the header now carries session identity exactly once, ' +
		'and the mailbox panel is unconditionally rendered regardless of load state.';

	let showOverview = $state(false);

	const { Story } = defineMeta({
		title: 'Shell/SessionNarrativeOverview',
		component: SessionNarrativeOverview,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(
			canvas.getByText('No overview recorded for this session yet.')
		).toBeInTheDocument();
		await expect(canvasElement.querySelectorAll('.sno-ghost-line').length).toBeGreaterThan(0);
		await expect(canvas.getByText(/Reserved: a two-paragraph narrative/)).toBeInTheDocument();

		const toggle = canvas.getByRole('button', { name: /toggle overview content/i });
		await userEvent.click(toggle);
		await expect(canvas.getByText(/moved the mailbox above the timeline/)).toBeInTheDocument();
		await expect(canvas.queryByText('No overview recorded for this session yet.')).toBeNull();

		const card = canvasElement.querySelector('.sno-card');
		await expect(card).not.toBeNull();
		const cardStyle = getComputedStyle(card!);
		await expect(cardStyle.borderRadius).toBe('0px');
		await expect(card!.classList.contains('edge')).toBe(true);
		await expect(cardStyle.clipPath).not.toBe('none');

		const bodyText = canvasElement.querySelector('.sno-text');
		await expect(bodyText).not.toBeNull();
		await expect(getComputedStyle(bodyText!).fontFamily.toLowerCase()).toContain('mono');
		const heading = canvasElement.querySelector('.sno-card-hd');
		await expect(heading).not.toBeNull();
		await expect(getComputedStyle(heading!).fontFamily.toLowerCase()).toContain('syne');
	}}
>
	{#snippet template()}
		<div style="max-width: 420px; background: var(--surface-bg); padding: 16px;">
			<button
				type="button"
				aria-label="toggle overview content"
				style="margin-bottom: 10px;"
				onclick={() => (showOverview = !showOverview)}
			>
				Toggle overview content
			</button>
			<SessionNarrativeOverview overview={showOverview ? SAMPLE_OVERVIEW : ''} />
		</div>
	{/snippet}
</Story>
