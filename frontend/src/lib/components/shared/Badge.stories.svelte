<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import Badge from './Badge.svelte';

	const { Story } = defineMeta({
		title: 'Shared/Badge',
		component: Badge,
		tags: ['autodocs'],
		parameters: {
			docs: {
				description: {
					component:
						'The base badge/pill/chip/tag primitive. Every appearance is a closed ' +
						'`variant` (see Badge.variants.ts) backed by the existing `.status-pill*` / ' +
						'`.badge-nx`+`.badge-*` / `.tag`+`.tag--*` app.css classes, or (for the ' +
						'identity variants) CSS owned by this component. StatusPill, ' +
						'PromptKindBadge, ClassChip and Tag are thin wrappers over this component ' +
						'and add no styling of their own.'
				}
			}
		}
	});
</script>

<Story
	name="Sequence"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const complete = canvas.getByText('complete');
		expect(complete.getAttribute('data-augmented-ui')).toBe('tl-clip br-clip border');
		expect(complete.className).toContain('status-pill-complete');

		const active = canvas.getByText('accepted');
		expect(active.getAttribute('data-augmented-ui')).toBe('tl-clip br-clip border');
		expect(active.className).toContain('badge-active');

		const muted = canvas.getByText('unclassified');
		expect(muted.className).toContain('badge-muted');

		const tag = canvas.getByText('hook');
		expect(tag.getAttribute('data-augmented-ui')).toBeNull();
		expect(tag.className).toContain('tag--green');

		const identity = canvas.getByText('main-agent');
		expect(identity.getAttribute('data-augmented-ui')).toBeNull();
		expect(identity.tagName).toBe('SPAN');

		const link = canvas.getByText('rust-developer');
		expect(link.tagName).toBe('A');
		expect(link.getAttribute('href')).toBe('/prompts/abc123');
		expect(link.className).toContain('class-chip-active');
	}}
>
	<div
		style="display: flex; flex-wrap: wrap; align-items: center; gap: 12px; padding: 16px; background: var(--surface-bg);"
	>
		<Badge variant="pill-complete">complete</Badge>
		<Badge variant="pill-error">failed</Badge>
		<Badge variant="chip-active">accepted</Badge>
		<Badge variant="chip-warning">corrected</Badge>
		<Badge variant="chip-muted">unclassified</Badge>
		<Badge variant="tag-green">hook</Badge>
		<Badge variant="tag-blue">boot</Badge>
		<Badge variant="identity">main-agent</Badge>
		<Badge variant="identity-active" href="/prompts/abc123">rust-developer</Badge>
	</div>
</Story>
