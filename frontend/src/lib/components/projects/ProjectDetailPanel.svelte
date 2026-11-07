<script lang="ts">
	import type { FleetProjectWithCounts } from '$lib/fleet/types';
	import { formatRelative, formatCount, formatClockAbsolute } from '$lib/time';
	import { Info } from '@lucide/svelte';
	import ProjectIdentityHeading from './ProjectIdentityHeading.svelte';

	interface Props {
		project: FleetProjectWithCounts;
		onRenameProject?: (projectPath: string, customName: string | null) => Promise<void>;
		nested?: boolean;
		hideIdentity?: boolean;
	}

	const { project, onRenameProject, nested = false, hideIdentity = false }: Props = $props();
</script>

<div class="flex flex-col gap-0" data-slot="project-detail-panel">
	{#if !nested && !hideIdentity}
		<div class="pdp-identity">
			<ProjectIdentityHeading {project} {onRenameProject} />
		</div>
	{/if}

	<div
		class="pdp-vitals funk-sec"
		class:edge={!nested}
		class:pdp-vitals--nested={nested}
		data-accent="teal"
		data-slot="project-detail-vitals"
	>
		<div class="funk-sec-hd">
			<span class="funk-sec-tag-backing">
				<span class="funk-sec-tag" data-augmented-ui="tr-clip bl-clip border">Vitals</span>
			</span>
		</div>

		<div class="funk-sec-body flex flex-col gap-3">
			<dl class="grid grid-cols-2 gap-x-6 gap-y-3 text-xs sm:grid-cols-4">
				<div>
					<dt class="text-label uppercase tracking-wider text-[var(--text-3)]">Sessions</dt>
					<dd class="font-mono text-[var(--text-1)]" data-slot="project-detail-session-count">
						{formatCount(project.session_count)}
					</dd>
				</div>
				<div>
					<dt class="text-label uppercase tracking-wider text-[var(--text-3)]">Requests</dt>
					<dd class="font-mono text-[var(--text-1)]" data-slot="project-detail-request-count">
						{formatCount(project.request_count)}
					</dd>
				</div>
				<div>
					<dt class="text-label uppercase tracking-wider text-[var(--text-3)]">First seen</dt>
					<dd
						class="font-mono text-[var(--text-1)]"
						title={formatClockAbsolute(project.first_seen)}
						data-slot="project-detail-first-seen"
					>
						{formatRelative(project.first_seen)}
					</dd>
				</div>
				<div>
					<dt class="text-label uppercase tracking-wider text-[var(--text-3)]">Last seen</dt>
					<dd
						class="font-mono text-[var(--text-1)]"
						title={formatClockAbsolute(project.last_seen)}
						data-slot="project-detail-last-seen"
					>
						{formatRelative(project.last_seen)}
					</dd>
				</div>
			</dl>
			<div class="flex items-center gap-2 text-label text-[var(--text-2)]">
				<span class="text-label uppercase tracking-wider text-[var(--text-3)]">Pinned</span>
				{#if project.pinned}
					<span class="badge-nx badge-active" data-slot="project-detail-pinned">Pinned</span>
				{:else}
					<span data-slot="project-detail-pinned">—</span>
				{/if}
			</div>

			<div class="pdp-note flex items-start gap-2" data-slot="project-detail-placeholder-note">
				<Info class="mt-0.5 size-4 shrink-0 text-[var(--text-3)]" aria-hidden="true" />
				<p class="text-label text-[var(--text-2)] italic">
					This page is a placeholder — full Overview, Intelligence, Lessons, and Agents sections for
					this project are still being designed.
				</p>
			</div>
		</div>
	</div>
</div>

<style>
	.pdp-identity {
		padding: 0 var(--space-4, 16px) var(--space-3, 12px);
	}

	.pdp-note {
		margin-top: 2px;
	}

	.pdp-vitals--nested {
		padding: 0;
		background: none;
	}
</style>
