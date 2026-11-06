<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import SettingsSection from './SettingsSection.svelte';

	const { Story } = defineMeta({
		title: 'Routes/Settings/detail/SettingsSection',
		component: SettingsSection,
		tags: ['autodocs'],
		parameters: {
			docs: {
				description: {
					component:
						'One settings group as a single card: a framed head bar carrying the title ' +
						'sits zero-gap on top of the framed body holding the rows. Same shape as ' +
						'ProjectSessionGroup on /sessions — tl-clip head over a bl-clip/br-clip body, ' +
						'flat --border-default on both.'
				}
			}
		},
		argTypes: {
			description: { control: false },
			action: { control: false },
			children: { control: false }
		}
	});
</script>

<Story
	name="Full"
	tags={['interactive']}
	args={{
		title: 'Session features',
		id: 'session-features',
		badge: 'Applies to new sessions',
		badgeVariant: 'session'
	}}
	play={async ({ canvasElement }) => {
		const head = canvasElement.querySelector('.cfg-sec-head');
		const body = canvasElement.querySelector('.cfg-sec-body');
		await expect(head, 'the head bar renders').not.toBeNull();
		await expect(body, 'the body card renders').not.toBeNull();

		const headRect = head!.getBoundingClientRect();
		const bodyRect = body!.getBoundingClientRect();
		await expect(
			Math.abs(headRect.bottom - bodyRect.top),
			'head bar and body must touch — a gap means the title floats outside its card'
		).toBeLessThanOrEqual(1);
		await expect(
			Math.abs(headRect.left - bodyRect.left),
			'head bar and body must share a left edge'
		).toBeLessThanOrEqual(1);

		const heading = canvasElement.querySelector('h2.cfg-sec-title');
		await expect(heading?.textContent?.trim()).toBe('Session features');
		await expect(heading?.querySelector('a')?.getAttribute('href')).toBe('#session-features');
		await expect(body!.getAttribute('aria-labelledby')).toBe(heading!.id);

		await expect(canvasElement.querySelector('.cfg-sec-badge--session')?.textContent).toContain(
			'Applies to new sessions'
		);
		const lede = canvasElement.querySelector('.cfg-sec-desc');
		await expect(body!.contains(lede), 'the lede lives inside the card').toBe(true);
	}}
>
	{#snippet template(args)}
		<div style="max-width:640px; padding:24px; background:var(--surface-bg);">
			<SettingsSection
				title={args.title}
				id={args.id}
				level={args.level}
				badge={args.badge}
				badgeVariant={args.badgeVariant}
			>
				{#snippet description()}
					A flip takes effect for sessions that start afterward. In-flight sessions keep their
					frozen setting.
				{/snippet}
				{#snippet action()}
					<a href="/settings/steering" style="font-size:11px; color:var(--text-3);">
						Steering settings
					</a>
				{/snippet}
				{#each ['reminders', 'recall_injector', 'file_read_compression'] as row (row)}
					<div
						style="padding:16px 20px; border-bottom:1px solid var(--border-subtle); font-size:13px; color:var(--text-1);"
					>
						{row}
					</div>
				{/each}
			</SettingsSection>
		</div>
	{/snippet}
</Story>
