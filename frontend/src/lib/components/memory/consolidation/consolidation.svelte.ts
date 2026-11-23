import { SvelteDate, SvelteSet } from 'svelte/reactivity';
import { ApiError } from '$lib/api';
import type { CapabilityRow } from '$lib/api/capabilities';
import {
	applyConsolidationDecision,
	getConsolidationCapability,
	getConsolidationDecisions,
	getConsolidationMeasures,
	getConsolidationSubjects,
	settleConsolidationConflict,
	undoConsolidationDecision,
	type DecisionRow,
	type MeasureSeriesRow,
	type SubjectSummary,
	type Verdict
} from '$lib/api/knowledgeConsolidation';
import { CHANGE_VERDICTS, refusalFor } from './labels';

export type MeasureWindow = '7' | '30' | 'all';
export type ChangeFilter = 'changes' | 'every' | Verdict;
export type BandName = 'capability' | 'conflicts' | 'measures' | 'changes' | 'subjects';

export const CHANGES_PAGE = 20;
export const SUBJECTS_PAGE = 50;
const POLL_MS = 30_000;
const DAY_MS = 86_400_000;

export interface BandRefresh {
	at: number | null;
	failedAt: number | null;
	error: string | null;
}

function message(err: unknown): string {
	return err instanceof ApiError || err instanceof Error ? err.message : String(err);
}

function blankRefresh(): BandRefresh {
	return { at: null, failedAt: null, error: null };
}

function mergeInPlace<T extends { id: number }>(
	held: T[],
	fresh: T[]
): { held: T[]; unseen: number } {
	const byId = new Map(fresh.map((row) => [row.id, row]));
	const heldIds = new SvelteSet(held.map((row) => row.id));
	return {
		held: held.map((row) => byId.get(row.id) ?? row),
		unseen: fresh.filter((row) => !heldIds.has(row.id)).length
	};
}

/**
 * Fetch, poll and actions for one project's Consolidation section. Components read the plain
 * `$state` fields and derive from them; nothing here is a `$derived` field.
 */
export class ConsolidationStore {
	project = $state('');
	capability = $state<CapabilityRow | null>(null);
	capabilityError = $state<string | null>(null);
	capabilityLoaded = $state(false);

	measures = $state<MeasureSeriesRow[]>([]);
	measuresError = $state<string | null>(null);
	measuresLoaded = $state(false);
	window = $state<MeasureWindow>('30');
	/** Run the measures are pointed at, epoch ms; shared by all eight charts. */
	pointAt = $state<number | null>(null);

	conflicts = $state<DecisionRow[]>([]);
	conflictsError = $state<string | null>(null);
	conflictsLoaded = $state(false);
	newConflicts = $state(0);
	/** Conflict ids a press settled, with the carrier label that was kept (null: settled elsewhere). */
	settled = $state<Record<number, string | null>>({});

	changes = $state<DecisionRow[]>([]);
	changesError = $state<string | null>(null);
	changesLoaded = $state(false);
	changesFilter = $state<ChangeFilter>('changes');
	changesNext = $state<number | null>(null);
	newChanges = $state(0);
	loadingOlder = $state(false);

	subjects = $state<SubjectSummary[]>([]);
	subjectsError = $state<string | null>(null);
	subjectsLoaded = $state(false);
	subjectsMore = $state(false);
	newSubjects = $state(0);
	loadingMoreSubjects = $state(false);

	refresh = $state<Record<BandName, BandRefresh>>({
		capability: blankRefresh(),
		conflicts: blankRefresh(),
		measures: blankRefresh(),
		changes: blankRefresh(),
		subjects: blankRefresh()
	});

	inFlight = $state<number | null>(null);
	rowErrors = $state<Record<number, string>>({});

	private timer: ReturnType<typeof setInterval> | null = null;
	private generation = 0;

	// ---- loading -------------------------------------------------------------------------

	async start(project: string, poll = true): Promise<void> {
		this.project = project;
		await this.reloadAll();
		this.stopPolling();
		if (!poll) return;
		this.timer = setInterval(() => {
			if (typeof document === 'undefined' || document.visibilityState === 'visible') {
				void this.poll();
			}
		}, POLL_MS);
	}

	dispose(): void {
		this.stopPolling();
		this.generation += 1;
	}

	private stopPolling(): void {
		if (this.timer !== null) clearInterval(this.timer);
		this.timer = null;
	}

	private get bandsOff(): boolean {
		return this.capability?.state === 'off';
	}

	/** User action: every band takes its rows and order afresh. */
	async reloadAll(): Promise<void> {
		this.generation += 1;
		this.pointAt = null;
		this.newConflicts = 0;
		this.newChanges = 0;
		this.newSubjects = 0;
		this.settled = {};
		this.rowErrors = {};
		await Promise.all([
			this.loadCapability(),
			this.loadMeasures(),
			this.loadConflicts(),
			this.loadChanges(),
			this.loadSubjects()
		]);
	}

	private stamp(band: BandName, error: string | null, kept: boolean): void {
		const now = Date.now();
		this.refresh[band] =
			error === null
				? { at: now, failedAt: null, error: null }
				: { at: this.refresh[band].at, failedAt: kept ? now : null, error };
	}

	async loadCapability(): Promise<void> {
		try {
			this.capability = await getConsolidationCapability();
			this.capabilityError = null;
			this.stamp('capability', null, true);
		} catch (err) {
			if (this.capabilityLoaded) this.stamp('capability', message(err), true);
			else this.capabilityError = message(err);
		} finally {
			this.capabilityLoaded = true;
		}
	}

	private sinceIso(): string | undefined {
		if (this.window === 'all') return undefined;
		return new SvelteDate(Date.now() - Number(this.window) * DAY_MS).toISOString();
	}

	async loadMeasures(): Promise<void> {
		const generation = this.generation;
		try {
			const rows = await getConsolidationMeasures(this.project, {
				since: this.sinceIso(),
				limit: 500
			});
			if (generation !== this.generation) return;
			this.measures = rows;
			this.measuresError = null;
			this.stamp('measures', null, true);
		} catch (err) {
			if (this.measuresLoaded) this.stamp('measures', message(err), true);
			else this.measuresError = message(err);
		} finally {
			this.measuresLoaded = true;
		}
	}

	async setWindow(window: MeasureWindow): Promise<void> {
		this.window = window;
		this.pointAt = null;
		await this.loadMeasures();
	}

	async loadConflicts(): Promise<void> {
		const generation = this.generation;
		try {
			const page = await getConsolidationDecisions({
				project: this.project,
				verdicts: ['conflict_open'],
				open: true,
				limit: 500
			});
			if (generation !== this.generation) return;
			this.conflicts = page.decisions;
			this.newConflicts = 0;
			this.settled = {};
			this.conflictsError = null;
			this.stamp('conflicts', null, true);
		} catch (err) {
			if (this.conflictsLoaded) this.stamp('conflicts', message(err), true);
			else this.conflictsError = message(err);
		} finally {
			this.conflictsLoaded = true;
		}
	}

	private changeVerdicts(): Verdict[] | undefined {
		if (this.changesFilter === 'every') return undefined;
		if (this.changesFilter === 'changes') return CHANGE_VERDICTS;
		return [this.changesFilter];
	}

	async loadChanges(): Promise<void> {
		const generation = this.generation;
		try {
			const page = await getConsolidationDecisions({
				project: this.project,
				verdicts: this.changeVerdicts(),
				limit: CHANGES_PAGE
			});
			if (generation !== this.generation) return;
			this.changes = page.decisions;
			this.changesNext = page.next_before;
			this.newChanges = 0;
			this.changesError = null;
			this.stamp('changes', null, true);
		} catch (err) {
			if (this.changesLoaded) this.stamp('changes', message(err), true);
			else this.changesError = message(err);
		} finally {
			this.changesLoaded = true;
		}
	}

	async setChangesFilter(filter: ChangeFilter): Promise<void> {
		this.changesFilter = filter;
		await this.loadChanges();
	}

	async loadOlderChanges(): Promise<void> {
		if (this.changesNext === null || this.loadingOlder) return;
		this.loadingOlder = true;
		try {
			const page = await getConsolidationDecisions({
				project: this.project,
				verdicts: this.changeVerdicts(),
				before: this.changesNext,
				limit: CHANGES_PAGE
			});
			const held = new SvelteSet(this.changes.map((c) => c.id));
			this.changes = [...this.changes, ...page.decisions.filter((c) => !held.has(c.id))];
			this.changesNext = page.next_before;
			this.changesError = null;
		} catch (err) {
			this.changesError = message(err);
		} finally {
			this.loadingOlder = false;
		}
	}

	async loadSubjects(): Promise<void> {
		const generation = this.generation;
		try {
			const rows = await getConsolidationSubjects(this.project, { limit: SUBJECTS_PAGE });
			if (generation !== this.generation) return;
			this.subjects = rows;
			this.subjectsMore = rows.length === SUBJECTS_PAGE;
			this.newSubjects = 0;
			this.subjectsError = null;
			this.stamp('subjects', null, true);
		} catch (err) {
			if (this.subjectsLoaded) this.stamp('subjects', message(err), true);
			else this.subjectsError = message(err);
		} finally {
			this.subjectsLoaded = true;
		}
	}

	async loadMoreSubjects(): Promise<void> {
		if (!this.subjectsMore || this.loadingMoreSubjects) return;
		this.loadingMoreSubjects = true;
		try {
			const rows = await getConsolidationSubjects(this.project, {
				limit: SUBJECTS_PAGE,
				offset: this.subjects.length
			});
			const held = new SvelteSet(this.subjects.map((s) => s.id));
			this.subjects = [...this.subjects, ...rows.filter((s) => !held.has(s.id))];
			this.subjectsMore = rows.length === SUBJECTS_PAGE;
			this.subjectsError = null;
		} catch (err) {
			this.subjectsError = message(err);
		} finally {
			this.loadingMoreSubjects = false;
		}
	}

	async showNew(band: 'conflicts' | 'changes' | 'subjects'): Promise<void> {
		if (band === 'conflicts') await this.loadConflicts();
		else if (band === 'changes') await this.loadChanges();
		else await this.loadSubjects();
	}

	// ---- polling -------------------------------------------------------------------------

	/** Replaces measures and capability; only updates held rows and counts unseen ones. */
	async poll(): Promise<void> {
		await this.pollCapability();
		if (this.bandsOff) return;
		await Promise.all([
			this.pollMeasures(),
			this.pollConflicts(),
			this.pollChanges(),
			this.pollSubjects()
		]);
	}

	private async pollCapability(): Promise<void> {
		try {
			this.capability = await getConsolidationCapability();
			this.stamp('capability', null, true);
		} catch (err) {
			this.stamp('capability', message(err), true);
		}
	}

	private async pollMeasures(): Promise<void> {
		try {
			this.measures = await getConsolidationMeasures(this.project, {
				since: this.sinceIso(),
				limit: 500
			});
			this.stamp('measures', null, true);
		} catch (err) {
			this.stamp('measures', message(err), true);
		}
	}

	private async pollConflicts(): Promise<void> {
		try {
			const page = await getConsolidationDecisions({
				project: this.project,
				verdicts: ['conflict_open'],
				open: true,
				limit: 500
			});
			const merged = mergeInPlace(this.conflicts, page.decisions);
			this.conflicts = merged.held;
			this.newConflicts = merged.unseen;
			const live = new SvelteSet(page.decisions.map((d) => d.id));
			for (const row of this.conflicts) {
				if (!live.has(row.id) && !(row.id in this.settled)) {
					this.settled = { ...this.settled, [row.id]: null };
				}
			}
			this.stamp('conflicts', null, true);
		} catch (err) {
			this.stamp('conflicts', message(err), true);
		}
	}

	private async pollChanges(): Promise<void> {
		try {
			const page = await getConsolidationDecisions({
				project: this.project,
				verdicts: this.changeVerdicts(),
				limit: CHANGES_PAGE
			});
			const merged = mergeInPlace(this.changes, page.decisions);
			this.changes = merged.held;
			this.newChanges = merged.unseen;
			this.stamp('changes', null, true);
		} catch (err) {
			this.stamp('changes', message(err), true);
		}
	}

	private async pollSubjects(): Promise<void> {
		try {
			const rows = await getConsolidationSubjects(this.project, {
				limit: Math.min(500, Math.max(SUBJECTS_PAGE, this.subjects.length))
			});
			const merged = mergeInPlace(this.subjects, rows);
			this.subjects = merged.held;
			this.newSubjects = merged.unseen;
			this.stamp('subjects', null, true);
		} catch (err) {
			this.stamp('subjects', message(err), true);
		}
	}

	// ---- actions -------------------------------------------------------------------------

	private clearError(id: number): void {
		if (!(id in this.rowErrors)) return;
		const next = { ...this.rowErrors };
		delete next[id];
		this.rowErrors = next;
	}

	private replaceDecision(updated: DecisionRow): void {
		const swap = (rows: DecisionRow[]): DecisionRow[] =>
			rows.map((r) => (r.id === updated.id ? updated : r));
		this.changes = swap(this.changes);
		this.conflicts = swap(this.conflicts);
	}

	async undo(row: DecisionRow): Promise<void> {
		await this.act(row, 'undo', async () =>
			this.replaceDecision(await undoConsolidationDecision(row.id))
		);
	}

	async apply(row: DecisionRow): Promise<void> {
		await this.act(row, 'apply', async () =>
			this.replaceDecision(await applyConsolidationDecision(row.id))
		);
	}

	async keep(conflict: DecisionRow, statementId: number): Promise<void> {
		await this.act(conflict, 'keep', async () => {
			await settleConsolidationConflict(conflict.id, statementId);
			const kept =
				statementId === conflict.statement_id
					? (conflict.carrier?.label ?? conflict.carrier_reference)
					: (conflict.counterpart?.label ?? conflict.counterpart_reference ?? '');
			this.settled = { ...this.settled, [conflict.id]: kept };
		});
	}

	private async act(
		row: DecisionRow,
		kind: 'undo' | 'apply' | 'keep',
		run: () => Promise<void>
	): Promise<void> {
		if (this.inFlight !== null) return;
		this.inFlight = row.id;
		this.clearError(row.id);
		try {
			await run();
			void this.poll();
		} catch (err) {
			this.rowErrors = { ...this.rowErrors, [row.id]: refusalFor(kind, message(err)) };
		} finally {
			this.inFlight = null;
		}
	}
}
