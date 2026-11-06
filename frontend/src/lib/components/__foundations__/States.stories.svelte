<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';

	const { Story } = defineMeta({
		title: 'Foundations/States',
		tags: ['autodocs']
	});
</script>

<Story name="ButtonStates">
	{#snippet template()}
		<div class="flex flex-col gap-6 p-8 bg-[var(--surface-bg)]">
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">
				Button Interaction States
			</p>
			<p class="text-caption text-[var(--text-3)]"></p>

			<div class="flex flex-wrap gap-4">
				<div class="flex flex-col items-center gap-2">
					<button
						type="button"
						class="inline-flex items-center justify-center rounded px-4 py-2 text-sm font-medium bg-primary text-primary-foreground transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--ring)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--surface-bg)]"
					>
						Idle
					</button>
					<span class="text-caption text-[var(--text-3)]">Default</span>
				</div>

				<div class="flex flex-col items-center gap-2">
					<button
						type="button"
						class="inline-flex items-center justify-center rounded px-4 py-2 text-sm font-medium bg-primary text-primary-foreground outline-none ring-2 ring-[var(--ring)] ring-offset-2 ring-offset-[var(--surface-bg)]"
					>
						Focus
					</button>
					<span class="text-caption text-[var(--text-3)]">Focus-visible</span>
				</div>

				<div class="flex flex-col items-center gap-2">
					<button
						type="button"
						disabled
						class="inline-flex items-center justify-center rounded px-4 py-2 text-sm font-medium bg-primary text-primary-foreground transition-colors disabled:pointer-events-none disabled:opacity-50"
					>
						Disabled
					</button>
					<span class="text-caption text-[var(--text-3)]">disabled:opacity-50</span>
				</div>

				<div class="flex flex-col items-center gap-2">
					<button
						type="button"
						class="inline-flex items-center justify-center rounded px-4 py-2 text-sm font-medium text-[var(--text-1)] bg-[var(--surface-card)] border border-[var(--border-default)] transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--ring)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--surface-bg)]"
					>
						Ghost
					</button>
					<span class="text-caption text-[var(--text-3)]">Outline/Ghost</span>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="RowStates">
	{#snippet template()}
		<div class="flex flex-col gap-4 p-8 bg-[var(--surface-bg)]">
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">Row Interaction States</p>
			<p class="text-caption text-[var(--text-3)]"></p>

			<div class="card-surface overflow-hidden">
				<table class="w-full">
					<thead>
						<tr class="border-b border-[var(--border-subtle)]">
							<th class="text-left text-label uppercase tracking-kpi text-[var(--text-3)] px-4 py-2"
								>State</th
							>
							<th class="text-left text-label uppercase tracking-kpi text-[var(--text-3)] px-4 py-2"
								>Content</th
							>
							<th class="text-left text-label uppercase tracking-kpi text-[var(--text-3)] px-4 py-2"
								>Class</th
							>
						</tr>
					</thead>
					<tbody>
						<tr class="border-b border-[var(--border-subtle)] transition-colors">
							<td class="px-4 py-3 text-xs text-[var(--text-3)]">Default</td>
							<td class="px-4 py-3 text-sm text-[var(--text-1)]">GET /v1/messages · 200</td>
							<td class="px-4 py-3"><code class="text-caption text-[var(--text-2)]">—</code></td>
						</tr>
						<tr class="border-b border-[var(--border-subtle)] bg-muted/50 transition-colors">
							<td class="px-4 py-3 text-xs text-[var(--text-3)]">Hover (simulated)</td>
							<td class="px-4 py-3 text-sm text-[var(--text-1)]">GET /v1/messages · 200</td>
							<td class="px-4 py-3">
								<code class="text-caption text-[var(--text-2)]">hover:bg-muted/50</code>
							</td>
						</tr>
						<tr
							class="border-b border-[var(--border-subtle)] bg-[var(--surface-2)] transition-colors"
						>
							<td class="px-4 py-3 text-xs text-[var(--text-3)]">Active / Selected</td>
							<td class="px-4 py-3 text-sm text-[var(--text-strong)]">GET /v1/messages · 200</td>
							<td class="px-4 py-3">
								<code class="text-caption text-[var(--text-2)]">bg-[var(--surface-2)]</code>
							</td>
						</tr>
						<tr
							class="border-b border-[var(--border-subtle)] bg-[var(--status-danger-bg)] transition-colors"
						>
							<td class="px-4 py-3 text-xs text-[var(--status-danger-fg)]">Error</td>
							<td class="px-4 py-3 text-sm text-[var(--text-1)]">POST /v1/messages · 500</td>
							<td class="px-4 py-3">
								<code class="text-caption text-[var(--text-2)]">bg-[var(--status-danger-bg)]</code>
							</td>
						</tr>
					</tbody>
				</table>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="Loading">
	{#snippet template()}
		<div class="flex flex-col gap-6 p-8 bg-[var(--surface-bg)]">
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">Loading States</p>
			<p class="text-caption text-[var(--text-3)]"></p>

			<div class="flex flex-col gap-2">
				<p class="text-xs text-[var(--text-3)]">Skeleton — animate-pulse</p>
				<div class="card-surface p-4 flex flex-col gap-3">
					<div class="h-4 w-48 rounded bg-[var(--surface-2)] animate-pulse"></div>
					<div class="h-3 w-full rounded bg-[var(--surface-2)] animate-pulse opacity-70"></div>
					<div class="h-3 w-3/4 rounded bg-[var(--surface-2)] animate-pulse opacity-70"></div>
					<div class="h-3 w-5/6 rounded bg-[var(--surface-2)] animate-pulse opacity-40"></div>
				</div>
				<code class="text-caption text-[var(--text-3)]">bg-[var(--surface-2)] animate-pulse</code>
			</div>

			<div class="flex flex-col gap-2">
				<p class="text-xs text-[var(--text-3)]">Inline Spinner — 16px</p>
				<div class="flex items-center gap-3">
					<div
						class="h-4 w-4 animate-spin rounded-full border-2 border-[var(--border-default)] border-t-[var(--text-2)]"
						role="status"
						aria-label="Loading"
					></div>
					<span class="text-sm text-[var(--text-2)]">Fetching requests…</span>
				</div>
				<code class="text-caption text-[var(--text-3)]">
					border-2 border-[var(--border-default)] border-t-[var(--text-2)] animate-spin
				</code>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="Empty">
	{#snippet template()}
		<div class="p-8 bg-[var(--surface-bg)]">
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)] mb-6">Empty State Pattern</p>

			<div class="card-surface flex flex-col items-center justify-center py-12 gap-3">
				<div
					class="flex h-12 w-12 items-center justify-center rounded-full border-2 border-[var(--border-default)] text-[var(--text-3)]"
					aria-hidden="true"
				>
					<svg
						width="20"
						height="20"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						stroke-width="1.5"
					>
						<circle cx="11" cy="11" r="8" /><path d="m21 21-4.35-4.35" />
					</svg>
				</div>
				<p class="text-sm font-medium text-[var(--text-1)]">No requests found</p>
				<p class="text-xs text-[var(--text-3)]">
					Requests will appear here once traffic is proxied through port 9988.
				</p>
				<button
					type="button"
					class="mt-1 inline-flex items-center rounded border border-[var(--border-default)] px-3 py-1.5 text-xs text-[var(--text-2)] transition-colors hover:bg-[var(--surface-2)] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--ring)]"
				>
					View setup guide
				</button>
			</div>
			<p class="text-caption text-[var(--text-3)] mt-3"></p>
		</div>
	{/snippet}
</Story>

<Story name="Error">
	{#snippet template()}
		<div class="flex flex-col gap-6 p-8 bg-[var(--surface-bg)]">
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">Error States</p>
			<p class="text-caption text-[var(--text-3)]"></p>

			<div>
				<p class="text-xs text-[var(--text-3)] mb-2">Page-level banner</p>
				<div
					class="rounded-md bg-[var(--status-danger-bg)] border border-[var(--status-danger-border)] text-[var(--status-danger-fg)] p-4 text-sm"
					role="alert"
				>
					Failed to fetch request details. The inspect API may be unreachable.
				</div>
				<code class="text-caption text-[var(--text-3)]">
					bg-[var(--status-danger-bg)] border-[var(--status-danger-border)]
					text-[var(--status-danger-fg)]
				</code>
			</div>

			<div>
				<p class="text-xs text-[var(--text-3)] mb-2">Inline field error</p>
				<div class="flex flex-col gap-1">
					<label for="url-input" class="text-xs text-[var(--text-2)]">Upstream URL</label>
					<input
						id="url-input"
						type="text"
						value="invalid-url"
						class="rounded border border-[var(--status-danger-border)] bg-[var(--surface-card)] px-3 py-1.5 text-sm text-[var(--text-1)] outline-none"
						aria-describedby="url-error"
					/>
					<p id="url-error" class="text-xs text-[var(--status-danger-fg)]">
						Must be a valid URL starting with http://
					</p>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="WrongRight">
	{#snippet template()}
		<div class="flex flex-col gap-8 p-8 bg-[var(--surface-bg)]">
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">
				Common Interaction State Mistakes
			</p>

			<div class="flex flex-col gap-2">
				<p class="text-xs font-medium text-[var(--status-danger-fg)]">
					WRONG — raw :hover in custom CSS inside a component
				</p>
				<div class="card-surface p-3">
					<code class="text-xs text-[var(--text-2)]">
						/* custom CSS scope — avoid this for interactive state */ tr:hover &#123; background:
						var(--surface-2); &#125;
					</code>
				</div>
				<p class="text-xs font-medium text-[var(--status-success-fg)]">
					RIGHT — hover: utility prefix
				</p>
				<div class="card-surface p-3">
					<code class="text-xs text-[var(--text-2)]">
						&lt;tr class="transition-colors hover:bg-muted/50 cursor-pointer"&gt;
					</code>
				</div>
				<p class="text-caption text-[var(--text-3)]">
					Why: Tailwind utilities stay co-located with markup; no separate CSS file to maintain.
				</p>
			</div>

			<div class="flex flex-col gap-2">
				<p class="text-xs font-medium text-[var(--status-danger-fg)]">
					WRONG — removes focus ring without replacement
				</p>
				<div class="card-surface p-3">
					<code class="text-xs text-[var(--text-2)]"> &lt;button class="outline-none"&gt; </code>
				</div>
				<p class="text-xs font-medium text-[var(--status-success-fg)]">
					RIGHT — replace with focus-visible ring
				</p>
				<div class="card-surface p-3">
					<code class="text-xs text-[var(--text-2)]">
						&lt;button class="focus-visible:outline-none focus-visible:ring-2
						focus-visible:ring-[var(--ring)] focus-visible:ring-offset-2
						focus-visible:ring-offset-[var(--surface-bg)]"&gt;
					</code>
				</div>
				<p class="text-caption text-[var(--text-3)]">
					Why: keyboard navigation requires visible focus. outline:none without ring-visible strands
					keyboard users. source: patterns.md:200
				</p>
			</div>
		</div>
	{/snippet}
</Story>
