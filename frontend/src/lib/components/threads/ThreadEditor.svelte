<!--
@component
Edits one thread through the three routes mike offers: change its details (PATCH), write a new
checkpoint that replaces the current state, or add an entry to the history. The details form sends
only the fields that changed. Mount it with a key per thread so its fields start from that thread.
-->
<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { Field } from '$lib/components/ui/field';
	import { Select } from '$lib/components/ui/select';
	import * as Tabs from '$lib/components/ui/tabs';
	import {
		ENTRY_KINDS,
		THREAD_STATUSES,
		type ThreadCheckpointInput,
		type ThreadDetail,
		type ThreadEntryInput,
		type ThreadMetaPatch
	} from '$lib/api/workThreads';
	import { splitLines } from './threadFormat';

	let {
		detail,
		busy = false,
		error = null,
		onPatch,
		onCheckpoint,
		onEntry,
		onClose
	}: {
		detail: ThreadDetail;
		busy?: boolean;
		error?: string | null;
		onPatch: (patch: ThreadMetaPatch) => void;
		onCheckpoint: (input: ThreadCheckpointInput) => void;
		onEntry: (input: ThreadEntryInput) => void;
		onClose: () => void;
	} = $props();

	function seedMeta() {
		return {
			title: detail.thread.title ?? '',
			goal: detail.thread.goal ?? '',
			status: detail.thread.status ?? 'active',
			branch: detail.thread.branch ?? '',
			worktree: detail.thread.worktree ?? '',
			depends: detail.depends_on.join(', '),
			criteria: detail.acceptance_criteria ?? ''
		};
	}

	function seedCheckpoint() {
		const s = detail.state;
		return {
			tldr: s?.tldr ?? '',
			where: s?.where_we_are ?? '',
			next: s?.next_up ?? '',
			blockers: s?.blockers ?? '',
			questions: s?.open_questions ?? '',
			decisions: s?.decisions ?? '',
			files: (s?.files ?? []).join('\n'),
			contribution: ''
		};
	}

	let tab = $state('details');
	let meta = $state(seedMeta());
	let cp = $state(seedCheckpoint());
	let entry = $state({ kind: 'note', summary: '', body: '' });

	const statusOptions = THREAD_STATUSES.map((value) => ({ value }));
	const kindOptions = ENTRY_KINDS.map((value) => ({ value }));

	function dependencies(): string[] {
		return meta.depends
			.split(/[,\s]+/)
			.map((slug) => slug.trim())
			.filter((slug) => slug.length > 0);
	}

	function save(): void {
		const patch: ThreadMetaPatch = {};
		const base = seedMeta();
		if (meta.title !== base.title) patch.title = meta.title;
		if (meta.goal !== base.goal) patch.goal = meta.goal;
		if (meta.status !== base.status) patch.status = meta.status;
		if (meta.branch !== base.branch) patch.branch = meta.branch;
		if (meta.worktree !== base.worktree) patch.worktree = meta.worktree;
		if (meta.criteria !== base.criteria) patch.acceptance_criteria = meta.criteria;
		if (dependencies().join(',') !== detail.depends_on.join(',')) patch.depends_on = dependencies();
		onPatch(patch);
	}

	function checkpoint(): void {
		onCheckpoint({
			slug: detail.thread.slug,
			tldr: cp.tldr,
			where_we_are: cp.where,
			next_up: cp.next,
			blockers: cp.blockers,
			open_questions: cp.questions,
			decisions: cp.decisions,
			files: splitLines(cp.files),
			contribution: cp.contribution.trim() || undefined
		});
	}

	function addEntry(): void {
		onEntry({
			kind: entry.kind,
			body: entry.body,
			summary: entry.summary.trim() || undefined
		});
	}
</script>

<section class="te" aria-label="Edit thread" data-slot="thread-editor">
	<Tabs.Root bind:value={tab}>
		<Tabs.List aria-label="What to change">
			<Tabs.Trigger value="details">Details</Tabs.Trigger>
			<Tabs.Trigger value="checkpoint">Checkpoint</Tabs.Trigger>
			<Tabs.Trigger value="entry">Add entry</Tabs.Trigger>
		</Tabs.List>

		<Tabs.Content value="details" class="te-form">
			<Field label="Title" bind:value={meta.title} />
			<Field label="Goal" multiline rows={2} bind:value={meta.goal} />
			<Select label="Status" options={statusOptions} bind:value={meta.status} />
			<Field label="Branch" bind:value={meta.branch} />
			<Field label="Worktree" bind:value={meta.worktree} />
			<Field
				label="Depends on"
				hint="thread slugs, separated by commas"
				bind:value={meta.depends}
			/>
			<Field
				label="Acceptance criteria"
				hint="one per line; needed before the status can be review"
				multiline
				rows={4}
				bind:value={meta.criteria}
			/>
			<div class="te-buttons">
				<Button disabled={busy} onclick={save}>Save details</Button>
				<Button variant="ghost" onclick={onClose}>Close</Button>
			</div>
		</Tabs.Content>

		<Tabs.Content value="checkpoint" class="te-form">
			<Field
				label="Summary"
				hint="what a session reads first"
				multiline
				rows={2}
				bind:value={cp.tldr}
			/>
			<Field label="Where we are" multiline rows={3} bind:value={cp.where} />
			<Field label="Next up" multiline rows={3} bind:value={cp.next} />
			<Field label="Blockers" multiline rows={2} bind:value={cp.blockers} />
			<Field label="Open questions" multiline rows={2} bind:value={cp.questions} />
			<Field label="Decisions" multiline rows={3} bind:value={cp.decisions} />
			<Field
				label="Files to read next"
				hint="one per line"
				multiline
				rows={3}
				bind:value={cp.files}
			/>
			<Field label="What this change did" bind:value={cp.contribution} />
			<div class="te-buttons">
				<Button disabled={busy || cp.tldr.trim().length === 0} onclick={checkpoint}
					>Write checkpoint</Button
				>
				<Button variant="ghost" onclick={onClose}>Close</Button>
			</div>
		</Tabs.Content>

		<Tabs.Content value="entry" class="te-form">
			<Select label="Kind" options={kindOptions} bind:value={entry.kind} />
			<Field label="Summary" bind:value={entry.summary} />
			<Field label="Body" multiline rows={6} bind:value={entry.body} />
			<div class="te-buttons">
				<Button disabled={busy || entry.body.trim().length === 0} onclick={addEntry}
					>Add entry</Button
				>
				<Button variant="ghost" onclick={onClose}>Close</Button>
			</div>
		</Tabs.Content>
	</Tabs.Root>
	<p class="te-error" aria-live="polite" data-error={error !== null}>{error ?? ''}</p>
</section>

<style>
	.te {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding: var(--space-4);
		background: var(--surface-card);
		border: 1px solid var(--border-default);
	}
	:global(.te-form) {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding-top: var(--space-3);
	}
	.te-buttons {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}
	.te-error {
		margin: 0;
		min-height: 1lh;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.te-error[data-error='true'] {
		color: var(--status-danger-fg);
	}
</style>
