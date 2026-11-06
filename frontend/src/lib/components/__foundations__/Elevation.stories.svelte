<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor } from 'storybook/test';
	import 'augmented-ui/augmented-ui.min.css';
	import SectionCardHarness from '$lib/components/__harness__/SectionCardHarness.svelte';
	import * as Popover from '$lib/components/ui/popover';
	import * as Dialog from '$lib/components/ui/dialog';
	import * as Sheet from '$lib/components/ui/sheet';
	import { Button } from '$lib/components/ui/button';
	import { createCollapseStore } from '$lib/stores/collapseState.svelte';

	const { Story } = defineMeta({
		title: 'Foundations/Elevation',
		tags: ['autodocs']
	});

	const ROLES = [
		{
			role: 'flat',
			surface: '',
			shadow: '',
			occupants: 'ghost button · tabs list · tabs trigger · separator · badge · table row'
		},
		{
			role: 'card',
			surface: '--surface-card',
			shadow: '--shadow-sm',
			occupants: 'SectionCard · .card-surface'
		},
		{
			role: 'raised',
			surface: '--surface-2',
			shadow: '--shadow-md',
			occupants: 'hovered card · Popover · Tooltip'
		},
		{
			role: 'overlay',
			surface: '--surface-3',
			shadow: '--shadow-lg',
			occupants: 'Dialog · Sheet'
		}
	] as const;

	function tokenText(token: string): string {
		return getComputedStyle(document.documentElement).getPropertyValue(token).trim();
	}

	function computedFor(property: 'backgroundColor' | 'boxShadow', token: string): string {
		const probe = document.createElement('div');
		if (property === 'backgroundColor') probe.style.backgroundColor = `var(${token})`;
		else probe.style.boxShadow = `var(${token})`;
		document.body.appendChild(probe);
		const value = getComputedStyle(probe)[property];
		probe.remove();
		return value;
	}
</script>

<script lang="ts">
	import { onMount } from 'svelte';

	const collapseStore = createCollapseStore();
	let resolved = $state<Record<string, string>>({});

	onMount(() => {
		const tokens = ROLES.flatMap(({ surface, shadow }) => [surface, shadow]).filter(Boolean);
		resolved = Object.fromEntries(tokens.map((token) => [token, tokenText(token)]));
	});
</script>

<Story
	name="Elevation"
	play={async ({ canvas }) => {
		async function openAndMeasure(
			slot: string,
			trigger: RegExp,
			measure: () => Promise<void>
		): Promise<void> {
			await userEvent.click(canvas.getByRole('button', { name: trigger }));
			const content = await waitFor(() => {
				const found = document.querySelector(`[data-slot="${slot}-content"]`);
				expect(found).toBeTruthy();
				return found as HTMLElement;
			});
			await measure();
			content.focus();
			await userEvent.keyboard('{Escape}');
			await waitFor(() => {
				expect(getComputedStyle(document.body).pointerEvents).not.toBe('none');
			});
		}

		for (const { role, surface, shadow } of ROLES) {
			const plate = canvas.getByTestId(`elev-${role}`);
			const painted = getComputedStyle(plate);

			if (shadow) {
				const expected = computedFor('boxShadow', shadow);
				await expect(expected).not.toBe('none');
				await expect(painted.boxShadow).toBe(expected);
			} else {
				await expect(painted.boxShadow).toBe('none');
			}

			if (surface) {
				await expect(painted.backgroundColor).toBe(computedFor('backgroundColor', surface));
			}
		}

		const cardSurface = computedFor('backgroundColor', '--surface-card');
		const raisedSurface = computedFor('backgroundColor', '--surface-2');
		const overlaySurface = computedFor('backgroundColor', '--surface-3');
		await expect(raisedSurface).not.toBe(cardSurface);
		await expect(overlaySurface).not.toBe(cardSurface);
		await expect(overlaySurface).not.toBe(raisedSurface);

		const shadows = ['--shadow-sm', '--shadow-md', '--shadow-lg'].map((token) =>
			computedFor('boxShadow', token)
		);
		await expect(new Set(shadows).size).toBe(3);

		const wrapper = canvas.getByTestId('elev-section-card').firstElementChild as HTMLElement;
		await expect(wrapper).toHaveAttribute('data-elevation', 'card');
		await expect(getComputedStyle(wrapper).boxShadow).toBe(shadows[0]);

		const clipped = wrapper.querySelector('[data-augmented-ui]') as HTMLElement;
		await expect(clipped).toBeTruthy();
		await expect(getComputedStyle(clipped).backgroundColor).toBe(cardSurface);
		await expect(getComputedStyle(clipped).boxShadow).toContain('inset');

		const browserContext = await import('@vitest/browser/context').catch(() => null);
		if (import.meta.env?.STORYBOOK_DISABLE_MOTION === 'true') {
			await expect(
				browserContext,
				'the gate has a real input backend; the raise must be driven by it, not skipped'
			).not.toBeNull();
		}
		if (browserContext) {
			const toggle = canvas.getByRole('button', { name: /diagnostics/i });
			await browserContext.page.elementLocator(toggle).hover();
			await waitFor(async () => {
				await expect(getComputedStyle(wrapper).boxShadow).toBe(shadows[1]);
				await expect(getComputedStyle(clipped).backgroundColor).toBe(raisedSurface);
			});
			await browserContext.page.elementLocator(canvas.getByTestId('elevation-plate')).hover();
		}

		await openAndMeasure('popover', /anchored panel/i, async () => {
			const floating = document.querySelector(
				'[data-bits-floating-content-wrapper]:has(> [data-slot="popover-content"])'
			) as HTMLElement | null;
			await expect(floating).toBeTruthy();
			await expect(getComputedStyle(floating as HTMLElement).boxShadow).toBe(shadows[1]);
			const content = (floating as HTMLElement).querySelector(
				'[data-slot="popover-content"]'
			) as HTMLElement;
			await expect(getComputedStyle(content).backgroundColor).toBe(raisedSurface);
		});

		for (const [name, pattern] of [
			['dialog', /open the dialog/i],
			['sheet', /open the sheet/i]
		] as const) {
			await openAndMeasure(name, pattern, async () => {
				const content = document.querySelector(`[data-slot="${name}-content"]`) as HTMLElement;
				await expect(getComputedStyle(content).backgroundColor).toBe(overlaySurface);
			});
		}
	}}
>
	{#snippet template()}
		<div class="fk-plate-page" data-testid="elevation-plate">
			<p class="fk-plate-title machine">Elevation</p>

			{#each ROLES as { role, surface, shadow, occupants } (role)}
				<div class="fk-role-row">
					<span class="fk-role-name">{role}</span>
					<div class="fk-role-plate" data-elevation={role} data-testid="elev-{role}"></div>
					<div class="fk-role-detail">
						<span class="fk-tag">
							{surface || 'inherits its parent'}
							{surface ? resolved[surface] || '—' : ''}
						</span>
						<span class="fk-tag fk-tag--dim">
							{shadow || 'no shadow'}
							{shadow ? resolved[shadow] || '—' : ''}
						</span>
						<span class="fk-tag fk-tag--dim">{occupants}</span>
					</div>
				</div>
			{/each}

			<p class="fk-section-title machine">On the real primitives</p>

			<div class="fk-demo-row">
				<div class="fk-demo" data-testid="elev-section-card">
					<SectionCardHarness
						{collapseStore}
						title="Diagnostics"
						collapsible
						sectionKey="foundations-elevation"
					>
						{#snippet children()}
							<p class="fk-demo-copy">
								A clipped card at the card layer. Its shadow is on the wrapper around it, because a
								clip-path would have eaten it here, and its surface comes from the token that
								wrapper re-points. Point at the header and both move up a layer together.
							</p>
						{/snippet}
					</SectionCardHarness>
				</div>

				<div class="fk-demo fk-demo--popover">
					<Popover.Root>
						<Popover.Trigger>
							{#snippet child({ props })}
								<Button variant="outline" size="sm" {...props}>Anchored panel</Button>
							{/snippet}
						</Popover.Trigger>
						<Popover.Content>
							<p class="fk-demo-copy">
								Floating chrome at the raised layer, one step lighter than the card below it.
								bits-ui already wraps this panel in an unclipped positioning element, so the shadow
								has somewhere to live and traces the cut corners.
							</p>
						</Popover.Content>
					</Popover.Root>
				</div>
			</div>

			<div class="fk-demo-row">
				<div class="fk-demo">
					<Dialog.Root>
						<Dialog.Trigger>
							{#snippet child({ props })}
								<Button variant="outline" size="sm" {...props}>Open the dialog</Button>
							{/snippet}
						</Dialog.Trigger>
						<Dialog.Content>
							<Dialog.Header>
								<Dialog.Title>Overlay layer</Dialog.Title>
								<Dialog.Description>
									A modal sits two steps above the card layer. It casts no shadow, because its
									content is clipped and positioned fixed, so the surface step is the whole of its
									depth.
								</Dialog.Description>
							</Dialog.Header>
						</Dialog.Content>
					</Dialog.Root>
				</div>

				<div class="fk-demo">
					<Sheet.Root>
						<Sheet.Trigger>
							{#snippet child({ props })}
								<Button variant="outline" size="sm" {...props}>Open the sheet</Button>
							{/snippet}
						</Sheet.Trigger>
						<Sheet.Content>
							<div class="fk-sheet-body">
								<Sheet.Title>Overlay layer</Sheet.Title>
								<p class="fk-demo-copy">
									The sheet takes the same layer as the dialog, by the same route and for the same
									reason.
								</p>
							</div>
						</Sheet.Content>
					</Sheet.Root>
				</div>
			</div>

			<p class="fk-readout">
				A one-pixel light rim would have cost no new value, since every theme declares
				--surface-shine, and it is left out rather than added where it would land on nothing: a role
				sits on a transparent wrapper everywhere except the portaled overlays, and an inset shadow
				there paints behind the child that covers it.
			</p>
		</div>
	{/snippet}
</Story>

<style>
	.fk-plate-page {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding: var(--space-6);
		background: var(--bg);
		max-width: 940px;
	}
	.fk-plate-title {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.fk-section-title {
		margin: var(--space-4) 0 0;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.fk-role-row {
		display: flex;
		align-items: center;
		gap: var(--space-4);
	}
	.fk-role-name {
		flex: none;
		width: 72px;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-1);
	}
	.fk-role-plate {
		flex: none;
		width: 132px;
		height: 40px;
		background: var(--surface-card);
	}
	.fk-role-detail {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}
	.fk-tag {
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-2);
		overflow-wrap: anywhere;
	}
	.fk-tag--dim {
		color: var(--text-3);
	}
	.fk-demo-row {
		display: flex;
		align-items: flex-start;
		gap: var(--space-6);
		padding-top: var(--space-2);
	}
	.fk-demo {
		flex: 1 1 0;
		min-width: 0;
	}
	.fk-demo--popover {
		display: flex;
		justify-content: center;
		padding-block: var(--space-6);
	}
	.fk-demo-copy {
		margin: 0;
		font-size: var(--text-xs);
		line-height: 1.5;
		color: var(--text-2);
	}
	.fk-sheet-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding: var(--space-5);
	}
	.fk-readout {
		margin: var(--space-2) 0 0;
		max-width: 62ch;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		line-height: 1.6;
		color: var(--text-3);
	}
</style>
