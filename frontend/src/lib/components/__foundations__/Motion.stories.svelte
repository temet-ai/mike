<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, expect, waitFor } from 'storybook/test';
	import 'augmented-ui/augmented-ui.min.css';
	import SectionCardHarness from '$lib/components/__harness__/SectionCardHarness.svelte';
	import * as Tabs from '$lib/components/ui/tabs';
	import * as Popover from '$lib/components/ui/popover';
	import { Button } from '$lib/components/ui/button';
	import { createCollapseStore } from '$lib/stores/collapseState.svelte';
	import { motionDuration } from '$lib/components/ui/motion';
	import SearchField from '$lib/components/shared/SearchField.svelte';

	const { Story } = defineMeta({
		title: 'Foundations/Motion',
		tags: ['autodocs']
	});

	const LANES = [
		{ dur: '--dur-1', ease: '--ease-snap' },
		{ dur: '--dur-2', ease: '--ease-snap' },
		{ dur: '--dur-3', ease: '--ease-io' }
	] as const;

	function keyframeProperties(name: string): string[] {
		const found = new Set<string>();
		for (const sheet of Array.from(document.styleSheets)) {
			let rules: CSSRuleList;
			try {
				rules = sheet.cssRules;
			} catch {
				continue;
			}
			for (const rule of Array.from(rules)) {
				if (!(rule instanceof CSSKeyframesRule) || rule.name !== name) continue;
				for (const frame of Array.from(rule.cssRules)) {
					const style = (frame as CSSKeyframeRule).style;
					for (let i = 0; i < style.length; i += 1) found.add(style.item(i));
				}
			}
		}
		return [...found];
	}

	const MOTIONS = [
		{ name: 'hover', dur: '--dur-1', ease: '--ease-snap', trigger: 'pointer or focus arrives' },
		{ name: 'press', dur: '--dur-1', ease: '--ease-snap', trigger: 'the control is held down' },
		{ name: 'select', dur: '--dur-1', ease: '--ease-io', trigger: 'selection moves here' },
		{ name: 'show', dur: '--dur-1', ease: '--ease-snap', trigger: 'a control becomes usable' },
		{ name: 'disclose', dur: '--dur-2', ease: '--ease-snap', trigger: 'a disclosure opens' },
		{
			name: 'disclose-cut',
			dur: '--dur-2',
			ease: '--ease-io',
			trigger: 'a group folds, its corner follows'
		},
		{ name: 'enter', dur: '--dur-2', ease: '--ease-snap', trigger: 'an overlay opens' },
		{ name: 'arrive', dur: '--dur-3', ease: '--ease-io', trigger: 'a new record lands' },
		{ name: 'wait', dur: '--dur-3', ease: '--ease-io', trigger: 'something is being waited on' }
	] as const;
</script>

<script lang="ts">
	import { onMount } from 'svelte';

	const collapseStore = createCollapseStore();

	let run = $state(false);
	let replays = $state(0);
	let resolved = $state<Record<string, string>>({});
	let timer: ReturnType<typeof setTimeout> | undefined;

	let delivered = $state<number[]>([]);
	let query = $state('');
	let folded = $state(false);

	onMount(() => {
		const probe = document.createElement('div');
		probe.setAttribute('data-design', 'funk');
		document.body.appendChild(probe);
		const cs = getComputedStyle(probe);
		resolved = Object.fromEntries(
			[...new Set(LANES.map((l) => l.dur))].map((d) => [d, cs.getPropertyValue(d).trim()])
		);
		probe.remove();

		timer = setTimeout(() => (run = true), 300);
		return () => clearTimeout(timer);
	});

	function replay() {
		clearTimeout(timer);
		run = false;
		replays += 1;
		timer = setTimeout(() => (run = true), 60);
	}

	function deliver() {
		delivered = [...delivered, delivered.length + 1];
	}
</script>

<Story
	name="Motion"
	play={async ({ canvas }) => {
		const lane = canvas.getByTestId('lane---dur-1');
		const plate = canvas.getByTestId('motion-plate');
		const rootStyle = getComputedStyle(document.documentElement);
		const scopedStyle = getComputedStyle(plate);
		const durations: number[] = [];

		for (const { dur, ease } of LANES) {
			const rootDuration = rootStyle.getPropertyValue(dur).trim();
			const rootEasing = rootStyle.getPropertyValue(ease).trim();
			await expect(rootDuration).not.toBe('');
			await expect(rootEasing).not.toBe('');
			await expect(scopedStyle.getPropertyValue(dur).trim()).toBe(rootDuration);
			await expect(scopedStyle.getPropertyValue(ease).trim()).toBe(rootEasing);
			const milliseconds = parseFloat(rootDuration) * (rootDuration.endsWith('ms') ? 1 : 1000);
			durations.push(milliseconds);
		}
		await expect(durations[0]).toBeGreaterThan(0);
		await expect(durations[1]).toBeGreaterThan(durations[0]!);
		await expect(durations[2]).toBeGreaterThan(durations[1]!);

		await waitFor(() => expect(lane).toHaveAttribute('data-run', 'true'));
		await expect(plate).toHaveAttribute('data-replays', '0');

		await userEvent.click(canvas.getByRole('button', { name: /replay/i }));
		await expect(plate).toHaveAttribute('data-replays', '1');
		await waitFor(() => expect(lane).toHaveAttribute('data-run', 'true'));

		const pressable = canvas.getByRole('button', { name: /hover and press me/i });
		await expect(pressable.getAttribute('data-motion')).toContain('hover');
		await expect(pressable.getAttribute('data-motion')).toContain('press');

		const firstTab = canvas.getByRole('tab', { name: /request/i });
		await expect(firstTab).toHaveAttribute('data-motion', 'select');

		const secondTab = canvas.getByRole('tab', { name: /response/i });
		await expect(firstTab).toHaveAttribute('data-state', 'active');
		await userEvent.click(secondTab);
		await waitFor(async () => {
			await expect(secondTab).toHaveAttribute('data-state', 'active');
		});

		const detached = document.createElement('div');
		const rootDur2 = rootStyle.getPropertyValue('--dur-2').trim();
		const rootDur2Ms = parseFloat(rootDur2) * (rootDur2.endsWith('ms') ? 1 : 1000);
		await expect(motionDuration(detached, '--dur-2')).toBe(rootDur2Ms);
		await expect(motionDuration(detached, '--dur-2')).toBeGreaterThan(0);

		const requested: Array<{ duration: number; rows: boolean }> = [];
		const nativeAnimate = Element.prototype.animate;
		Element.prototype.animate = function (keyframes, options) {
			const duration = typeof options === 'number' ? options : (options?.duration ?? 0);
			const frames = Array.isArray(keyframes) ? keyframes : [];
			requested.push({
				duration: typeof duration === 'number' ? duration : 0,
				rows: frames.some((frame) => frame !== null && 'gridTemplateRows' in frame)
			});
			return nativeAnimate.call(this, keyframes, options);
		};

		try {
			const header = canvas.getByRole('button', { name: /captured headers/i });
			if (header.getAttribute('aria-expanded') === 'true') {
				await userEvent.click(header);
				await waitFor(async () => {
					await expect(header).toHaveAttribute('aria-expanded', 'false');
				});
			}
			requested.length = 0;
			await userEvent.click(header);
			await waitFor(async () => {
				await expect(header).toHaveAttribute('aria-expanded', 'true');
			});

			await waitFor(async () => {
				const rowAnimations = requested.filter((call) => call.rows && call.duration > 0);
				await expect(
					rowAnimations.length,
					'opening a disclosure must animate grid-template-rows over a non-zero ' +
						`duration; saw ${JSON.stringify(requested)}`
				).toBeGreaterThan(0);
				await expect(rowAnimations[0]?.duration).toBe(rootDur2Ms);
			});
		} finally {
			Element.prototype.animate = nativeAnimate;
		}

		await userEvent.click(canvas.getByRole('button', { name: /open panel/i }));
		const overlay = await waitFor(() => {
			const found = document.querySelector('[data-slot="popover-content"]');
			expect(found).toBeTruthy();
			return found as HTMLElement;
		});
		await expect(getComputedStyle(overlay).animationName).toBe('funk-popover-in');

		const field = canvas.getByRole('searchbox', { name: /search the gallery/i });
		const clear = field.parentElement!.querySelector<HTMLButtonElement>('button')!;
		await expect(clear).toHaveAttribute('aria-label', 'Clear search');
		await expect(clear).toHaveAttribute('data-motion', 'show');
		await expect(getComputedStyle(clear).visibility).toBe('hidden');
		await userEvent.keyboard('{Escape}');
		await userEvent.click(field);
		await userEvent.type(field, 'agent');
		await waitFor(() => expect(clear).toHaveAttribute('data-shown'));
		await expect(getComputedStyle(clear).transitionProperty).toContain('visibility');
		await waitFor(() => expect(getComputedStyle(clear).visibility).toBe('visible'));
		await userEvent.click(clear);
		await expect(field).toHaveValue('');

		const cut = canvas.getByTestId('cut-head');
		await expect(getComputedStyle(cut).transitionProperty).toContain('--motion-cut');
		await expect(getComputedStyle(cut).getPropertyValue('--motion-cut').trim()).toBe('14px');
		await userEvent.click(canvas.getByRole('button', { name: /fold the group/i }));
		await waitFor(() =>
			expect(getComputedStyle(cut).getPropertyValue('--motion-cut').trim()).toBe('4px')
		);

		const list = canvas.getByTestId('arrive-list');
		await expect(list.children.length).toBe(0);
		await userEvent.click(canvas.getByRole('button', { name: /deliver a record/i }));
		await waitFor(async () => {
			await expect(list.children.length).toBe(1);
		});
		await expect(list.children[0]).toHaveAttribute('data-motion', 'arrive');

		let waitsChecked = 0;
		for (const element of [...plate.querySelectorAll('*'), plate, overlay]) {
			const { animationIterationCount, animationName } = getComputedStyle(element);
			if (animationName === 'none') continue;
			const isWait = (element.getAttribute('data-motion') ?? '').includes('wait');
			if (!isWait) {
				await expect(
					animationIterationCount,
					`${animationName} loops; only the wait motion may repeat`
				).not.toContain('infinite');
				continue;
			}
			waitsChecked += 1;

			const seconds = parseFloat(getComputedStyle(element).animationDuration);
			const period = Math.round(seconds * 1000);
			await expect(
				period % durations[2]!,
				`${animationName} runs ${period}ms, which is not a whole multiple of --dur-3`
			).toBe(0);
			await expect(period / durations[2]!).toBeGreaterThanOrEqual(2);

			const moved = keyframeProperties(animationName);
			await expect(moved.length, `${animationName} animates nothing`).toBeGreaterThan(0);
			await expect(
				moved.some((prop) => prop === 'transform' || prop.startsWith('background-position')),
				`${animationName} must move something directional; it animates ${moved.join(', ')}`
			).toBe(true);
			for (const banned of ['opacity', 'scale']) {
				await expect(
					moved,
					`${animationName} oscillates ${banned}; a wait marches, it does not pulse`
				).not.toContain(banned);
			}
		}
		await expect(
			waitsChecked,
			'the wait demo must be running; a skipped check is not a passing one'
		).toBeGreaterThanOrEqual(3);
	}}
>
	{#snippet template()}
		<div class="fk-plate" data-design="funk" data-testid="motion-plate" data-replays={replays}>
			<p class="fk-plate-title machine">Motion</p>

			{#each LANES as lane (lane.dur)}
				<div class="fk-lane-row">
					<span class="fk-lane-tag">
						{lane.dur}
						{resolved[lane.dur] || '—'} · {lane.ease.replace('--ease-', '')}
					</span>
					<div class="fk-lane" data-testid="lane-{lane.dur}" data-run={run}>
						<span
							class="fk-dot"
							style="transition: left var({lane.dur}) var({lane.ease}); left: {run
								? 'calc(100% - 12px)'
								: '2px'};"
						></span>
					</div>
				</div>
			{/each}

			<div class="fk-controls">
				<button type="button" class="fk-btn" onclick={replay}>Replay</button>
				<span class="fk-readout">every animation is a state change</span>
			</div>

			<p class="fk-section-title machine">The motions</p>

			<div class="fk-model">
				{#each MOTIONS as motion (motion.name)}
					<div class="fk-model-row">
						<span class="fk-model-name">{motion.name}</span>
						<span class="fk-model-lane">
							{motion.dur}
							{resolved[motion.dur] || '—'} · {motion.ease.replace('--ease-', '')}
						</span>
						<span class="fk-model-trigger">{motion.trigger}</span>
					</div>
				{/each}
			</div>

			<p class="fk-section-title machine">On the real primitives</p>

			<div class="fk-demo-grid">
				<div class="fk-demo">
					<p class="fk-demo-label">hover · press</p>
					<Button variant="outline">Hover and press me</Button>
				</div>

				<div class="fk-demo">
					<p class="fk-demo-label">enter</p>
					<Popover.Root>
						<Popover.Trigger>
							{#snippet child({ props })}
								<Button variant="outline" {...props}>Open panel</Button>
							{/snippet}
						</Popover.Trigger>
						<Popover.Content>
							<p class="fk-demo-copy">
								The panel arrives on --dur-2 and leaves on --dur-1, because leaving should never
								make the operator wait.
							</p>
						</Popover.Content>
					</Popover.Root>
				</div>

				<div class="fk-demo fk-demo--wide">
					<p class="fk-demo-label">select</p>
					<Tabs.Root value="request">
						<Tabs.List>
							<Tabs.Trigger value="request">Request</Tabs.Trigger>
							<Tabs.Trigger value="response">Response</Tabs.Trigger>
						</Tabs.List>
						<Tabs.Content value="request">
							<p class="fk-demo-copy">
								The label repaints and the rail under the active tab fades in on the same lane, so
								activation reads as one movement.
							</p>
						</Tabs.Content>
						<Tabs.Content value="response">
							<p class="fk-demo-copy">
								Selection is carried by colour and the indicator rail, never by a size or weight
								change that would shift the tabs beside it.
							</p>
						</Tabs.Content>
					</Tabs.Root>
				</div>

				<div class="fk-demo">
					<p class="fk-demo-label">show</p>
					<SearchField bind:value={query} label="Search the gallery" placeholder="Type to show" />
					<p class="fk-demo-copy">
						The clear control fades in once there is something to clear, in the place it will
						occupy, so nothing beside it moves.
					</p>
				</div>

				<div class="fk-demo">
					<p class="fk-demo-label">disclose-cut</p>
					<button
						type="button"
						class="fk-cut-head"
						data-testid="cut-head"
						data-augmented-ui="tl-clip border"
						data-motion="disclose-cut"
						data-collapsed={folded}
						aria-expanded={!folded}
						onclick={() => (folded = !folded)}
					>
						Fold the group
					</button>
					<p class="fk-demo-copy">
						A group's corner cut is a registered length, so it turns with the fold instead of
						jumping. The rows below disclose on the lane above.
					</p>
				</div>

				<div class="fk-demo fk-demo--wide">
					<p class="fk-demo-label">disclose</p>
					<SectionCardHarness
						{collapseStore}
						title="Captured headers"
						collapsible
						sectionKey="foundations-motion"
					>
						{#snippet children()}
							<p class="fk-demo-copy">
								The body's row grows from nothing to exactly this content's height, so what sits
								below moves once and smoothly. The marker in the header turns over the same span.
							</p>
						{/snippet}
					</SectionCardHarness>
				</div>

				<div class="fk-demo fk-demo--wide">
					<p class="fk-demo-label">wait</p>
					<div class="fk-wait-row">
						<span class="fk-wait-bar" data-motion="wait" aria-hidden="true"></span>
						<span class="fk-wait-spin" data-motion="wait wait-spin" aria-hidden="true"></span>
						<span class="fk-wait-dots" aria-hidden="true">
							<span data-motion="wait wait-march">·</span><span data-motion="wait wait-march"
								>·</span
							><span data-motion="wait wait-march">·</span>
						</span>
					</div>
					<p class="fk-demo-copy">
						A sweep, a rotation and a march. The one motion that repeats, and the only one allowed
						to: a wait has no settled state until it ends, and it ends when the element goes away.
						None of the three touches opacity or scale, which is what separates a wait from a pulse.
					</p>
				</div>

				<div class="fk-demo fk-demo--wide">
					<p class="fk-demo-label">arrive</p>
					<button type="button" class="fk-btn" onclick={deliver}>Deliver a record</button>
					<ul class="fk-arrive-list" data-testid="arrive-list">
						{#each delivered as record (record)}
							<li class="fk-arrive-item" data-motion="arrive">
								Record {record} reached the list while you were reading this.
							</li>
						{/each}
					</ul>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<style>
	.fk-plate {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-6);
		background: var(--bg);
		max-width: 940px;
	}
	.fk-plate-title {
		margin: 0 0 var(--space-2);
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.fk-section-title {
		margin: var(--space-5) 0 var(--space-2);
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.fk-lane-row {
		display: flex;
		align-items: center;
		gap: var(--space-3);
	}
	.fk-lane-tag {
		flex: none;
		width: 150px;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-2);
	}
	.fk-lane {
		position: relative;
		flex: 1;
		height: 14px;
		background: var(--surface-2);
		border: var(--hairline) solid var(--line);
	}
	.fk-dot {
		position: absolute;
		top: 2px;
		width: 8px;
		height: 8px;
		background: var(--cyan);
		box-shadow: var(--glow-cyan);
	}
	.fk-controls {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		margin-top: var(--space-2);
	}
	.fk-btn {
		display: inline-flex;
		align-items: center;
		align-self: flex-start;
		height: var(--control-h);
		padding: 0 var(--space-3);
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		color: var(--text-1);
		background: var(--surface-2);
		border: var(--hairline) solid var(--line);
		cursor: pointer;
		transition:
			background-color var(--dur-1) var(--ease-snap),
			border-color var(--dur-1) var(--ease-snap);
	}
	.fk-btn:hover {
		background: var(--surface-3);
		border-color: var(--line-strong);
	}
	.fk-btn:focus-visible {
		outline: 2px solid var(--focus);
		outline-offset: 2px;
	}
	.fk-readout {
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
	}
	.fk-model {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}
	.fk-model-row {
		display: flex;
		align-items: baseline;
		gap: var(--space-3);
		font-family: var(--font-mono);
		font-size: var(--text-caption);
	}
	.fk-model-name {
		flex: none;
		width: 90px;
		color: var(--text-1);
	}
	.fk-model-lane {
		flex: none;
		width: 190px;
		color: var(--text-2);
	}
	.fk-model-trigger {
		color: var(--text-3);
	}
	.fk-demo-grid {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: var(--space-5);
		align-items: start;
	}
	.fk-demo {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-width: 0;
	}
	.fk-demo--wide {
		grid-column: 1 / -1;
	}
	.fk-demo-label {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
	}
	.fk-demo-copy {
		margin: 0;
		font-size: var(--text-xs);
		line-height: 1.5;
		color: var(--text-2);
	}
	.fk-wait-row {
		display: flex;
		align-items: center;
		gap: var(--space-4);
	}
	.fk-wait-bar {
		width: 180px;
		height: 12px;
		background: linear-gradient(
			90deg,
			var(--surface-2) 0%,
			var(--surface-3) 50%,
			var(--surface-2) 100%
		);
		background-size: 200% 100%;
	}
	.fk-wait-spin {
		width: 12px;
		height: 12px;
		border: var(--hairline) solid var(--line);
		border-top-color: var(--cyan);
	}
	.fk-wait-dots {
		display: inline-flex;
		gap: var(--space-1);
		font-family: var(--font-mono);
		color: var(--text-2);
	}
	.fk-cut-head {
		--motion-cut: 14px;
		--aug-tl: var(--motion-cut);
		--aug-border-all: var(--hairline);
		--aug-border-bg: var(--line-strong);
		align-self: flex-start;
		padding: var(--space-1) var(--space-3) var(--space-1) var(--space-5);
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		color: var(--text-1);
		background: var(--surface-2);
		border: none;
		cursor: pointer;
	}
	.fk-cut-head[data-collapsed='true'] {
		--motion-cut: 4px;
	}
	.fk-cut-head:focus-visible {
		outline: 2px solid var(--focus);
		outline-offset: 2px;
	}
	.fk-arrive-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.fk-arrive-item {
		padding: var(--space-2) var(--space-3);
		font-size: var(--text-xs);
		color: var(--text-2);
		background: var(--surface-2);
		border: var(--hairline) solid var(--line);
	}
</style>
