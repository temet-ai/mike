<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, expect } from 'storybook/test';

	const { Story } = defineMeta({
		title: 'Foundations/Section Stack',
		tags: ['autodocs']
	});

	const BANDS = [
		{
			label: 'Session controls',
			meta: '1/9 steerers · 0 graders · 11 rules · 5 analyzers',
			help: false
		},
		{ label: 'Flagged responses', meta: '0 findings', help: false },
		{ label: 'Tool uses', meta: '256 calls', help: false },
		{ label: 'Memory actions', meta: '', help: true },
		{ label: 'Lessons touching this session', meta: '', help: true },
		{ label: 'Mailbox', meta: '2 messages', help: true }
	] as const;
</script>

<script lang="ts">
	import DisclosureMarker from '../shared/DisclosureMarker.svelte';

	let grouped = $state(true);
	let openBand = $state<string | null>(null);

	const toggle = (label: string) => (openBand = openBand === label ? null : label);
</script>

{#snippet bandHeader(label: string, meta: string, help: boolean)}
	<div class="band-hd-wrap">
		<button
			type="button"
			class="band-hd"
			aria-expanded={openBand === label}
			onclick={() => toggle(label)}
		>
			<DisclosureMarker open={openBand === label} />
			<span class="fk-band-slot"></span>
			<span class="machine fk-band-lbl" class:fk-band-lbl--open={openBand === label}>{label}</span>
			{#if meta}<span class="band-hd__meta">{meta}</span>{/if}
		</button>
		{#if help}
			<span class="fk-band-help"><span class="fk-band-help-dot"></span></span>
		{/if}
	</div>
{/snippet}

<Story
	name="SectionStack"
	play={async ({ canvas }) => {
		const verdict = canvas.getByTestId('stack-verdict');
		await expect(verdict).toHaveTextContent('Contract');

		const band = canvas.getByRole('button', { name: /tool uses/i });
		await expect(band).toHaveAttribute('aria-expanded', 'false');
		await userEvent.click(band);
		await expect(band).toHaveAttribute('aria-expanded', 'true');

		await userEvent.click(canvas.getByRole('button', { name: /break the group/i }));
		await expect(verdict).toHaveTextContent('Violation');
		await userEvent.click(canvas.getByRole('button', { name: /group the stack/i }));
		await expect(verdict).toHaveTextContent('Contract');
	}}
>
	{#snippet template()}
		<div class="fk-plate" data-design="funk">
			<p class="fk-plate-title machine">Section stack</p>

			<div class="fk-two-col">
				<div>
					<div
						class="machine fk-verdict"
						class:fk-verdict--good={grouped}
						class:fk-verdict--bad={!grouped}
						data-testid="stack-verdict"
					>
						{grouped ? 'Contract — one frame, hairline seams' : 'Violation — a frame per section'}
					</div>

					{#if grouped}
						<div class="edge mech-group">
							{#each BANDS as band (band.label)}
								<div>{@render bandHeader(band.label, band.meta, band.help)}</div>
							{/each}
						</div>
					{:else}
						<div class="fk-broken">
							{#each BANDS as band (band.label)}
								<div
									class="edge"
									style="--edge: var(--edge-sm); --edge-pad: 0px; --edge-line: var(--red-border);"
								>
									{@render bandHeader(band.label, band.meta, band.help)}
								</div>
							{/each}
						</div>
					{/if}

					<p class="fk-cap">
						{grouped
							? 'the group draws the frame AND every seam'
							: 'doubled lines at every join: upper frame, its inlay, lower frame, its inlay'}
					</p>

					<div class="fk-controls">
						<button type="button" class="fk-btn" onclick={() => (grouped = !grouped)}>
							{grouped ? 'Break the group' : 'Group the stack'}
						</button>
						<span class="fk-readout">same six sections · one frame, or six</span>
					</div>

					<div class="machine fk-subhead">Corners on an interior seam</div>
					<div class="fk-seam">
						<div
							class="edge fk-seam-cell"
							style="--edge: 14px; --edge-br: 0px; --edge-pad: calc(var(--edge) + var(--space-1));"
						>
							<span class="machine fk-seam-lbl">--edge-br: 0</span>
						</div>
						<div
							class="edge fk-seam-cell fk-seam-cell--right"
							style="--edge: 14px; --edge-tl: 0px; --edge-pad: calc(var(--edge) + var(--space-1));"
						>
							<span class="machine fk-seam-lbl">--edge-tl: 0</span>
						</div>
					</div>
					<p class="fk-cap">
						square whichever corner lands on a seam · the pair shares one frame via margin-left:
						-1px
					</p>
				</div>

				<div class="fk-rules">
					<div class="machine fk-subhead fk-subhead--flush">The rules</div>

					<p class="fk-rule">
						A stack of sections is ONE framed object. Each section is a band inside it.
					</p>
					<p class="fk-rule">
						The group owns the seam: <span class="fk-tok">&gt; * + *</span> — never :nth-child, which
						stops matching the moment a band is added or hidden.
					</p>
					<p class="fk-rule">
						Members draw NO frame of their own.
						<span class="fk-dim">
							Press the button and watch each join double from one line to four.
						</span>
					</p>
					<p class="fk-rule">
						The group sets <span class="fk-tok">--edge-pad: 0</span>; bands own their padding.
					</p>
					<p class="fk-rule">
						Every band leads with the same header row. A header that labels rather than toggles is a
						<span class="fk-tok">div</span>, never a button — a section that cannot collapse must
						not advertise the affordance.
					</p>
					<p class="fk-rule">
						A band's tooltip is a SIBLING of the toggle, never nested inside it.
					</p>
					<p class="fk-rule">
						One disclosure marker, and it ROTATES. Open a band above and watch it turn 90 degrees
						and take the accent colour. Swapping the triangle for a second glyph is banned.
					</p>
					<p class="fk-rule">
						Never restate the band classes in scoped component CSS. Scoped rules are unlayered and
						out-specify @layer components, so a local copy wins silently and that one band drifts
						from every other.
					</p>

					<div class="machine fk-subhead">Gap discipline</div>
					<div class="fk-gap-row fk-gap-row--bare">identity header — no frame at all</div>
					<div class="edge mech-group">
						<div class="fk-gap-row">overview band — flush against it at 0</div>
					</div>
					<div class="fk-gap-meter">
						<span class="machine fk-gap-tag">margin-block</span>
						<span class="fk-gap-line"></span>
						<span class="fk-gap-val">16px</span>
					</div>
					<div class="edge mech-group">
						<div class="fk-gap-row">mechanism group — one framed instrument</div>
					</div>
					<div class="fk-gap-meter">
						<span class="machine fk-gap-tag">margin-block</span>
						<span class="fk-gap-line"></span>
						<span class="fk-gap-val">16px</span>
					</div>
					<div class="fk-gap-row fk-gap-row--bare">timeline</div>
					<p class="fk-cap">
						the header is bare chrome · a run butts at ZERO · only a self-contained instrument
						floats clear
					</p>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<style>
	.fk-plate {
		display: flex;
		flex-direction: column;
		padding: var(--space-6);
		background: var(--bg);
		max-width: 1000px;
	}
	.fk-plate-title {
		margin: 0 0 var(--space-4);
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.fk-two-col {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: var(--space-4);
		align-items: start;
	}
	.fk-verdict {
		margin-bottom: var(--space-2);
		font-size: var(--text-caption);
	}
	.fk-verdict--bad {
		color: var(--red);
	}
	.fk-verdict--good {
		color: var(--green);
	}

	.fk-broken {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.fk-band-slot {
		flex: none;
		width: 11px;
		height: 11px;
		border: var(--hairline) solid var(--line-strong);
	}
	.fk-band-lbl {
		font-size: var(--text-caption);
		color: var(--text-2);
		white-space: nowrap;
		transition: color var(--dur-1) var(--ease-snap);
	}
	.fk-band-lbl--open {
		color: var(--cyan);
	}
	.fk-band-help {
		display: flex;
		align-items: center;
		padding: 0 var(--space-3);
	}
	.fk-band-help-dot {
		width: 11px;
		height: 11px;
		border-radius: 50%;
		border: var(--hairline) solid var(--line);
	}

	.fk-controls {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		margin-top: var(--space-3);
	}
	.fk-btn {
		display: inline-flex;
		align-items: center;
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

	.fk-subhead {
		margin: var(--space-5) 0 var(--space-2);
		font-size: var(--text-caption);
		color: var(--text-3);
	}
	.fk-subhead--flush {
		margin-top: 0;
	}

	.fk-seam {
		display: grid;
		grid-template-columns: 1fr 1fr;
	}
	.fk-seam-cell {
		min-height: 54px;
	}
	.fk-seam-cell--right {
		margin-left: -1px;
	}
	.fk-seam-lbl {
		font-size: var(--text-micro);
		color: var(--text-2);
	}

	.fk-rules {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.fk-rule {
		margin: 0;
		padding: 6px 0 6px var(--space-2);
		border-left: 2px solid var(--line);
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		line-height: var(--leading-body);
		color: var(--text-2);
	}
	.fk-tok {
		color: var(--cyan);
	}
	.fk-dim {
		color: var(--text-3);
	}

	.fk-gap-row {
		padding: 7px var(--space-2);
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-2);
	}
	.fk-gap-row--bare {
		color: var(--text-3);
	}
	.fk-gap-meter {
		display: flex;
		align-items: center;
		gap: var(--space-1);
		height: var(--space-4);
		padding: 0 var(--space-2);
	}
	.fk-gap-tag {
		font-size: var(--text-micro);
		color: var(--yellow);
	}
	.fk-gap-line {
		flex: 1;
		height: 1px;
		background: var(--line);
	}
	.fk-gap-val {
		font-family: var(--font-mono);
		font-size: var(--text-micro);
		color: var(--text-3);
	}

	.fk-cap {
		margin: var(--space-2) 0 0;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		line-height: var(--leading-body);
		color: var(--text-3);
	}
</style>
