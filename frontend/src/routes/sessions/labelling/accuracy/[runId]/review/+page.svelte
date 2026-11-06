<script lang="ts">
	import { Switch } from '$lib/components/ui/switch';
	import { tick, untrack } from 'svelte';
	import { SvelteMap } from 'svelte/reactivity';
	import { page } from '$app/state';
	import { replaceState } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { ChevronLeft, ChevronRight, Keyboard, List, ListFilter, SearchX } from '@lucide/svelte';
	import * as Tabs from '$lib/components/ui/tabs';
	import * as Popover from '$lib/components/ui/popover';
	import * as Sheet from '$lib/components/ui/sheet';
	import { Button } from '$lib/components/ui/button';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import FilteredEmptyState from '$lib/components/shared/FilteredEmptyState.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import { getSession, listSessionOperatorTurns } from '$lib/api';
	import type { OperatorTurnDto, SessionSummary } from '$lib/types';
	import { listEvalLabels, postGoldLabels } from '$lib/api/eval';
	import type { EvalLabelDto } from '$lib/api/eval';
	import { getEvalReviewDetail, getOperatorTurnFacts, listEvalRunItems } from '$lib/api/evalRuns';
	import type {
		EvalReviewDetailDto,
		EvalRunItemDto,
		OperatorTurnFactsDto
	} from '$lib/api/evalRuns';
	import EvalAnswerCard from '$lib/components/eval/EvalAnswerCard.svelte';
	import EvalChoiceGroup from '$lib/components/eval/EvalChoiceGroup.svelte';
	import EvalDecisionBar from '$lib/components/eval/EvalDecisionBar.svelte';
	import type { BarQuestion } from '$lib/components/eval/EvalDecisionBar.svelte';
	import EvalSessionContext from '$lib/components/eval/EvalSessionContext.svelte';
	import EvalTurnEvidence from '$lib/components/eval/EvalTurnEvidence.svelte';
	import EvalTurnList from '$lib/components/eval/EvalTurnList.svelte';
	import LabelDefinitions from '$lib/components/eval/LabelDefinitions.svelte';
	import type { RowInfo } from '$lib/components/eval/EvalTurnList.svelte';
	import {
		MODE_TABS,
		QUESTION,
		ROW_LABEL,
		activeFilterCount,
		byTurn,
		candidates,
		citedSpan,
		disagreementCount,
		disputedAxes,
		filtersFromParams,
		firstUndecided,
		formatWhen,
		isTextTarget,
		joinWords,
		judgeOn,
		judgeMissedLast,
		judgeSawLastMessage,
		labelOn,
		labellingWho,
		labellingWhy,
		latestReview,
		loadErrorWords,
		neighbour,
		nextUndecided,
		otherAxisNote,
		paramsFor,
		queueQuery,
		quotedEvidence,
		saveErrorText,
		shortcutFor,
		turnPosition
	} from '$lib/components/eval/evalReview';
	import type {
		Axis,
		BarStatus,
		Candidate,
		FactsState,
		ReviewFilters,
		ReviewMode
	} from '$lib/components/eval/evalReview';
	import { optionsFor, wordFor } from '$lib/components/eval/labelControl';
	import { runName, runWhen, shortModel } from '$lib/components/judges/judges';
	import type { EvalReviewPageData } from './+page';

	let { data }: { data: EvalReviewPageData } = $props();

	interface Loaded {
		turn: number;
		detail: EvalReviewDetailDto;
		history: EvalLabelDto[];
	}

	interface SessionData {
		summary: SessionSummary | null;
		summaryError: string | null;
		turns: OperatorTurnDto[] | null;
		turnsError: string | null;
	}

	const EMPTY_SESSION: SessionData = {
		summary: null,
		summaryError: null,
		turns: null,
		turnsError: null
	};
	const FOLD_KEY = 'judges:session-band';

	const initialParams = page.url.searchParams;
	let requestedTurn: number | null = Number(initialParams.get('turn')) || null;

	let filters = $state<ReviewFilters>(filtersFromParams(initialParams));
	let queue = $state.raw<EvalRunItemDto[]>([]);
	let total = $state(0);
	let reviewed = $state(0);
	let queueState = $state<'loading' | 'ok' | 'error'>('loading');
	let queueError = $state<string | null>(null);

	let selected = $state<number | null>(null);
	let shown = $state.raw<Loaded | null>(null);
	let loadingTurn = $state<number | null>(null);
	let detailError = $state<string | null>(null);
	let failedTurn = $state<number | null>(null);
	let factsFor = $state.raw<{ turn: number; state: FactsState }>({
		turn: -1,
		state: { status: 'loading' }
	});
	let note = $state('');
	let busy = $state(false);
	let status = $state<BarStatus | null>(null);
	let otherLeft = $state<number | null>(null);
	let sheetOpen = $state(false);
	let filtersOpen = $state(false);
	let keysOpen = $state(false);
	let bandOpen = $state(true);
	let panelHeight = $state(0);

	let scroller = $state<HTMLElement | undefined>();
	let bar = $state<ReturnType<typeof EvalDecisionBar> | undefined>();

	const detailCache = new Map<number, Promise<Loaded>>();
	const detailDone = new Map<number, Loaded>();
	const factsCache = new Map<number, Promise<OperatorTurnFactsDto>>();
	const factsDone = new Map<number, OperatorTurnFactsDto>();
	const sessionStarted = new Set<string>();
	const sessions = new SvelteMap<string, SessionData>();
	const rowInfo = new SvelteMap<number, RowInfo>();
	const seenSessions = new Set<string>();
	let queueToken = 0;
	let openToken = 0;
	let factsToken = 0;

	const mode = $derived(filters.mode);
	const otherMode = $derived<ReviewMode>(mode === 'reaction' ? 'delivery' : 'reaction');
	const query = $derived(queueQuery(filters));
	const filterCount = $derived(activeFilterCount(filters));
	const percent = $derived(total > 0 ? Math.round((reviewed / total) * 100) : 0);
	const runTitle = $derived(data.run ? runName(data.run) : null);
	const runShort = $derived(
		data.run ? `Labelling accuracy, ${runWhen(data.run)}` : 'Labelling accuracy'
	);
	const disputedCount = $derived(tabCount(mode));

	const item = $derived(shown?.detail.item ?? null);
	const questions = $derived<Axis[]>(item ? disputedAxes(item, mode) : []);
	const current = $derived(shown?.detail.labels.current ?? []);
	const runGold = $derived(shown?.detail.labels.run_gold ?? []);
	const saved = $derived(shown ? latestReview(shown.detail.labels.reviews) : null);
	const facts = $derived<FactsState>(
		shown && factsFor.turn === shown.turn ? factsFor.state : { status: 'loading' }
	);
	const knownFacts = $derived(facts.status === 'ok' ? facts.facts : null);
	const quote = $derived(questions[0] ? quotedEvidence(runGold, questions[0]) : null);
	const cited = $derived(
		shown
			? citedSpan(quote, [
					{ key: 'reply', text: shown.detail.context?.next_genuine_text ?? null },
					{ key: 'last', text: knownFacts?.stop_response ?? null },
					{ key: 'prompt', text: shown.detail.context?.user_prompt ?? null }
				])
			: null
	);
	const notes = $derived(
		item
			? (['delivery', 'reaction'] as const)
					.filter((a) => !questions.includes(a))
					.map((a) => otherAxisNote(item, current, a))
					.filter((n): n is string => n !== null)
			: []
	);
	const barQuestions = $derived<BarQuestion[]>(
		item
			? questions.map((axis) => {
					const list = candidates(item, current, axis);
					return {
						axis,
						candidates: list,
						others: optionsFor(data.vocabulary, axis).filter(
							(o) => !list.some((c) => c.label === o.label)
						),
						saved: saved?.answers[axis] ?? null
					};
				})
			: []
	);
	const sessionUuid = $derived(
		shown?.detail.context?.session_uuid ?? knownFacts?.session_id ?? null
	);
	const session = $derived(sessionUuid ? (sessions.get(sessionUuid) ?? EMPTY_SESSION) : null);
	const position = $derived(
		session?.turns && shown ? turnPosition(session.turns, shown.turn) : null
	);
	const nextPosition = $derived(
		session?.turns && shown?.detail.context?.next_genuine_turn_id
			? turnPosition(session.turns, shown.detail.context.next_genuine_turn_id)
			: null
	);
	const turnHref = $derived.by(() => {
		if (!sessionUuid) return null;
		const base = resolve(`/sessions/${encodeURIComponent(sessionUuid)}` as `/sessions/${string}`);
		const start = knownFacts?.start_request_id;
		return start ? `${base}?request=${start}` : base;
	});
	const startedAt = $derived(formatWhen(knownFacts?.created_at));
	const answeredBy = $derived.by(() => {
		const verdict = item?.verdict;
		if (!verdict) return null;
		const config = verdict.bundle_id ?? data.run?.bundle_id;
		return `${shortModel(verdict.model)}${config ? `, configuration ${config}` : ''}`;
	});
	const answerCount = $derived(
		Math.max(1, ...barQuestions.map((q) => q.candidates.length + q.others.length))
	);
	const judgeSaw = $derived(
		judgeSawLastMessage(
			knownFacts?.stop_response,
			typeof item?.verdict?.input_text === 'string' ? item.verdict.input_text : null
		)
	);
	const tabDone = $derived(
		queueState === 'ok' &&
			queue.length > 0 &&
			selected === null &&
			queue.every((r) => r.reviewed_at)
	);
	const filteredEmpty = $derived(queueState === 'ok' && queue.length === 0);

	function definition(axis: Axis, label: string): string | null {
		return optionsFor(data.vocabulary, axis).find((o) => o.label === label)?.description ?? null;
	}

	function tabCount(value: ReviewMode): number | null {
		const report = data.run?.report;
		return disagreementCount(value === 'reaction' ? report?.reaction : report?.delivery);
	}

	function labelChoices(axis: Axis) {
		return [
			{ value: '', label: 'any' },
			...optionsFor(data.vocabulary, axis).map((o) => ({ value: o.label, label: wordFor(o.label) }))
		];
	}

	function labelRow(axis: Axis, c: Candidate): EvalLabelDto | null {
		if (c.holders.includes('label')) return labelOn(current, axis) ?? labelOn(runGold, axis);
		if (c.holders.includes('earlier')) return labelOn(runGold, axis);
		return null;
	}

	function cardProps(axis: Axis, c: Candidate) {
		const row = labelRow(axis, c);
		const shipped =
			axis === 'delivery' &&
			barQuestions.some((q) => q.candidates.some((x) => x.label === 'shipped'));
		return {
			labelledBy: row ? labellingWho(row, axis) : null,
			why:
				row && shown
					? labellingWhy({
							labels: shown.history,
							row,
							axis,
							vocabulary: data.vocabulary,
							quoteShown: cited !== null && cited.span === row.evidence?.trim(),
							shippedDisputed: shipped
						})
					: []
		};
	}

	function readFolds(): Record<string, boolean> {
		try {
			const raw = sessionStorage.getItem(FOLD_KEY);
			return raw ? (JSON.parse(raw) as Record<string, boolean>) : {};
		} catch {
			return {};
		}
	}

	function writeFold(uuid: string, open: boolean) {
		try {
			sessionStorage.setItem(FOLD_KEY, JSON.stringify({ ...readFolds(), [uuid]: open }));
		} catch {
			return;
		}
	}

	function applyFold(uuid: string | null) {
		if (!uuid) return;
		const chosen = readFolds()[uuid];
		bandOpen = chosen ?? !seenSessions.has(uuid);
		seenSessions.add(uuid);
	}

	function toggleBand(open: boolean) {
		bandOpen = open;
		if (sessionUuid) writeFold(sessionUuid, open);
	}

	const sessionPromises = new Map<
		string,
		{ summary: Promise<SessionSummary>; turns: Promise<OperatorTurnDto[]> }
	>();

	function fetchSession(uuid: string) {
		const cached = sessionPromises.get(uuid);
		if (cached) return cached;
		const fresh = {
			summary: getSession(uuid, { limit: 1 }).then((d) => d.summary),
			turns: listSessionOperatorTurns(uuid, { order: 'asc', promptKind: 'operator' })
		};
		fresh.summary.catch(() => undefined);
		fresh.turns.catch(() => undefined);
		sessionPromises.set(uuid, fresh);
		return fresh;
	}

	function loadSession(uuid: string) {
		if (sessionStarted.has(uuid)) return;
		sessionStarted.add(uuid);
		sessions.set(uuid, { ...EMPTY_SESSION });
		const update = (patch: Partial<SessionData>) =>
			sessions.set(uuid, { ...(sessions.get(uuid) ?? EMPTY_SESSION), ...patch });
		const { summary, turns } = fetchSession(uuid);
		summary
			.then((value) => update({ summary: value }))
			.catch((e) => update({ summaryError: loadErrorWords(e) }));
		turns
			.then((value) => update({ turns: value }))
			.catch((e) => update({ turnsError: loadErrorWords(e) }));
	}

	function retrySession() {
		if (!sessionUuid) return;
		sessionStarted.delete(sessionUuid);
		sessionPromises.delete(sessionUuid);
		loadSession(sessionUuid);
	}

	function fetchDetail(turn: number): Promise<Loaded> {
		const cached = detailCache.get(turn);
		if (cached) return cached;
		const promise = Promise.all([
			getEvalReviewDetail(data.runId, turn),
			listEvalLabels({ kind: 'turn', turnId: turn })
		]).then(([detail, labels]) => {
			const loaded = { turn, detail, history: labels.labels };
			detailDone.set(turn, loaded);
			if (detail.context?.session_uuid) loadSession(detail.context.session_uuid);
			return loaded;
		});
		promise.catch(() => detailCache.delete(turn));
		detailCache.set(turn, promise);
		return promise;
	}

	function fetchFacts(turn: number): Promise<OperatorTurnFactsDto> {
		const cached = factsCache.get(turn);
		if (cached) return cached;
		const promise = getOperatorTurnFacts(turn).then((f) => {
			factsDone.set(turn, f);
			return f;
		});
		promise.catch(() => factsCache.delete(turn));
		factsCache.set(turn, promise);
		return promise;
	}

	function forget(turn: number) {
		detailCache.delete(turn);
		detailDone.delete(turn);
	}

	async function loadFacts(turn: number) {
		const token = ++factsToken;
		const done = factsDone.get(turn);
		if (done) {
			factsFor = { turn, state: { status: 'ok', facts: done } };
			return;
		}
		factsFor = { turn, state: { status: 'loading' } };
		try {
			const f = await fetchFacts(turn);
			if (token === factsToken) factsFor = { turn, state: { status: 'ok', facts: f } };
		} catch (e) {
			if (token === factsToken)
				factsFor = { turn, state: { status: 'error', message: loadErrorWords(e) } };
		}
	}

	function prefetch(turn: number) {
		const next = nextUndecided(queue, turn);
		if (next === null) return;
		fetchDetail(next).catch(() => undefined);
		fetchFacts(next).catch(() => undefined);
	}

	async function focusQuestion() {
		await tick();
		if ((scroller?.getBoundingClientRect().top ?? 0) < 0) window.scrollTo({ top: 0 });
		scroller?.querySelector<HTMLElement>('[data-question]')?.focus({ preventScroll: true });
	}

	async function open(turn: number, after: BarStatus | null = null, focus = false) {
		const token = ++openToken;
		selected = turn;
		note = '';
		status = null;
		detailError = null;
		failedTurn = null;
		void loadFacts(turn);
		let loaded = detailDone.get(turn) ?? null;
		if (!loaded) {
			loadingTurn = turn;
			try {
				loaded = await fetchDetail(turn);
			} catch (e) {
				if (token !== openToken) return;
				loadingTurn = null;
				shown = null;
				detailError = loadErrorWords(e);
				failedTurn = turn;
				return;
			}
			if (token !== openToken) return;
		}
		applyFold(loaded.detail.context?.session_uuid ?? null);
		shown = loaded;
		loadingTurn = null;
		status = after;
		prefetch(turn);
		if (focus) await focusQuestion();
	}

	function closeTurn(after: BarStatus | null) {
		openToken++;
		selected = null;
		shown = null;
		loadingTurn = null;
		detailError = null;
		status = after;
	}

	async function loadQueue(fresh: boolean) {
		if (!data.run) return;
		const token = ++queueToken;
		if (fresh) queueState = 'loading';
		try {
			const result = await listEvalRunItems(data.runId, query);
			if (token !== queueToken) return;
			total = result.total;
			reviewed = result.reviewed;
			queueError = null;
			if (fresh) {
				queue = byTurn(result.items);
				queueState = 'ok';
				const turn = requestedTurn ?? firstUndecided(queue);
				requestedTurn = null;
				if (turn === null) closeTurn(null);
				else void open(turn);
			} else {
				const latest = new Map(result.items.map((r) => [r.operator_turn_id, r]));
				queue = queue.map((r) => latest.get(r.operator_turn_id) ?? r);
			}
		} catch (e) {
			if (token !== queueToken) return;
			queueError = loadErrorWords(e);
			if (fresh) queueState = 'error';
		}
	}

	$effect(() => {
		void query;
		untrack(() => void loadQueue(true));
	});

	$effect(() => {
		const search = paramsFor(filters, selected).toString();
		untrack(() => {
			try {
				replaceState(`${page.url.pathname}${search ? `?${search}` : ''}`, {});
			} catch {
				return;
			}
		});
	});

	$effect(() => {
		if (!tabDone || !data.run) return;
		const other = otherMode;
		untrack(() => {
			otherLeft = null;
			listEvalRunItems(data.runId, { disagreement: other, include: ['review'], limit: 1 })
				.then((r) => (otherLeft = r.total - r.reviewed))
				.catch(() => (otherLeft = null));
		});
	});

	function setMode(value: ReviewMode) {
		if (value === filters.mode) return;
		filters = { mode: value, gold: '', judge: '', agree: filters.agree };
	}

	function setFilter<K extends keyof ReviewFilters>(key: K, value: ReviewFilters[K]) {
		filters = { ...filters, [key]: value };
	}

	function clearFilters() {
		filters = { mode: filters.mode, gold: '', judge: '', agree: false };
		filtersOpen = false;
	}

	function step(direction: 1 | -1) {
		const next = neighbour(queue, selected, direction);
		if (next !== null && next !== selected) void open(next, null, true);
	}

	const canPrevious = $derived(
		neighbour(queue, selected, -1) !== null && neighbour(queue, selected, -1) !== selected
	);
	const canNext = $derived(
		neighbour(queue, selected, 1) !== null && neighbour(queue, selected, 1) !== selected
	);

	async function decide(answers: Array<{ axis: Axis; label: string }>) {
		if (!shown || busy || loadingTurn !== null) return;
		const turn = shown.turn;
		const words = joinWords(answers.map((a) => wordFor(a.label)));
		busy = true;
		status = { tone: 'muted', text: `Saving ${words}` };
		try {
			await postGoldLabels(
				{ kind: 'turn', turnId: turn },
				answers,
				note.trim() || null,
				'eval_review',
				data.runId
			);
		} catch (e) {
			status = { tone: 'danger', text: saveErrorText(e) };
			busy = false;
			return;
		}
		busy = false;
		note = '';
		forget(turn);
		const stamp = new Date().toISOString();
		queue = queue.map((r) =>
			r.operator_turn_id === turn ? { ...r, reviewed_at: r.reviewed_at ?? stamp } : r
		);
		void loadQueue(false);
		const after: BarStatus = {
			tone: 'success',
			text: `Saved the last turn as ${words}.`,
			goBack: turn
		};
		const next = nextUndecided(queue, turn);
		if (next === null) closeTurn(after);
		else await open(next, after, true);
	}

	function retryFacts() {
		if (!shown) return;
		factsCache.delete(shown.turn);
		void loadFacts(shown.turn);
	}

	function retryDetail() {
		if (failedTurn === null) return;
		forget(failedTurn);
		void open(failedTurn);
	}

	function pickFromList(turn: number) {
		sheetOpen = false;
		void open(turn, null, true);
	}

	async function rowVisible(turn: number) {
		if (rowInfo.has(turn)) return;
		try {
			const loaded = await fetchDetail(turn);
			const uuid = loaded.detail.context?.session_uuid;
			if (!uuid) {
				rowInfo.set(turn, { error: 'no session' });
				return;
			}
			const found = fetchSession(uuid);
			const [summary, turns] = await Promise.all([found.summary, found.turns]);
			rowInfo.set(turn, {
				title: summary.title ?? 'Untitled session',
				position: turnPosition(turns, turn)
			});
		} catch (e) {
			rowInfo.set(turn, { error: loadErrorWords(e) });
		}
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.defaultPrevented || isTextTarget(event.target)) return;
		const action = shortcutFor(event);
		if (!action) return;
		event.preventDefault();
		if (action.kind === 'next') step(1);
		else if (action.kind === 'previous') step(-1);
		else if (loadingTurn === null) bar?.shortcut(action.index);
	}
</script>

<svelte:head><title>Review labels — Sessions — mike</title></svelte:head>
<svelte:window onkeydown={onKeydown} />

<div class="jd-scroll" bind:this={scroller} style:--jd-panel-h={`${panelHeight}px`}>
	<div class="jd">
		<header class="jd-head">
			<div class="jd-title-row">
				<h1 class="jd-h1">Review labels</h1>
				<span class="jd-runs"
					><Button variant="outline" href={resolve('/sessions/labelling/accuracy')}
						>Labelling accuracy</Button
					></span
				>
			</div>
			{#if runTitle}
				<p class="jd-run-words" data-testid="run-words">
					From <a
						class="jd-run-link"
						href={resolve(
							`/sessions/labelling/accuracy/${data.runId}` as `/sessions/labelling/accuracy/${string}`
						)}
						data-testid="run-link">{runTitle}</a
					>
				</p>
			{/if}
			{#if data.run}
				<p class="jd-lede jd-lead" data-testid="intro-count">
					The judge's answer and the turn's label differ on {disputedCount ?? total} turns in this accuracy
					run. One of them is wrong.
				</p>
				<p class="jd-lede">
					For each turn: read what happened, then click the correct label. With one question shown,
					that click saves your answer and the next turn opens. When a turn differs on both
					questions it shows two rows of answers: pick one in each row, and the turn is saved and
					the next one opens only once both rows have an answer.
				</p>
				<p class="jd-lede">
					Your answer becomes the turn's label. It counts in every accuracy run that includes the
					turn, so a turn you review here shows as reviewed there too, and later accuracy runs are
					scored against it.
				</p>
				<LabelDefinitions vocabulary={data.vocabulary} />
			{/if}
		</header>

		{#if !data.run}
			<LoadErrorBanner
				title="This accuracy run could not be loaded"
				detail={data.runError ?? 'not found'}
			/>
		{:else}
			{#if data.vocabularyError}
				<p class="jd-error" role="alert">
					The answer definitions could not be loaded: {data.vocabularyError}
				</p>
			{/if}

			<div class="jd-bar">
				<Tabs.Root value={mode} onValueChange={(v) => setMode(v as ReviewMode)}>
					<Tabs.List aria-label="Which question">
						{#each MODE_TABS as tab (tab.value)}
							<Tooltip text={tab.help}>
								{#snippet child({ props })}
									<Tabs.Trigger {...props} value={tab.value}
										>{tab.label}{#if tabCount(tab.value) !== null}<span class="jd-count"
												>{tabCount(tab.value)}</span
											>{/if}</Tabs.Trigger
									>
								{/snippet}
							</Tooltip>
						{/each}
					</Tabs.List>
				</Tabs.Root>
				<div class="jd-progress" data-testid="review-progress">
					<span
						>{#if queueState === 'loading' && queue.length === 0}Loading turns{:else}<strong
								>{reviewed}</strong
							>
							of <strong>{total}</strong>
							reviewed{/if}</span
					>
					<span class="jd-track" aria-hidden="true"
						><span class="jd-fill" style:width={`${percent}%`}></span></span
					>
				</div>
				<div class="jd-tools">
					<Tooltip text="Previous turn in this list · k">
						{#snippet child({ props })}
							<Button
								{...props}
								variant="outline"
								aria-label="Previous"
								disabled={!canPrevious}
								onclick={() => step(-1)}
								><ChevronLeft aria-hidden="true" /><span class="jd-pager-text">Previous</span
								></Button
							>
						{/snippet}
					</Tooltip>
					<Tooltip text="Next turn in this list · j">
						{#snippet child({ props })}
							<Button
								{...props}
								variant="outline"
								aria-label="Next"
								disabled={!canNext}
								onclick={() => step(1)}
								><span class="jd-pager-text">Next</span><ChevronRight aria-hidden="true" /></Button
							>
						{/snippet}
					</Tooltip>
					<Popover.Root bind:open={keysOpen}>
						<Popover.Trigger>
							{#snippet child({ props })}
								<Button {...props} variant="ghost" size="icon" aria-label="Keyboard shortcuts"
									><Keyboard aria-hidden="true" /></Button
								>
							{/snippet}
						</Popover.Trigger>
						<Popover.Content side="bottom" align="end" class="jd-keys">
							<h2 class="machine jd-keys-title">Keyboard shortcuts</h2>
							<ul class="jd-keys-list">
								<li>
									<kbd>Alt+1</kbd> to <kbd>Alt+{answerCount}</kbd>: choose that answer, in the order
									shown{#if barQuestions.length > 1}, in the first row still without an answer{/if}
								</li>
								<li><kbd>j</kbd> / <kbd>k</kbd>: next / previous turn</li>
								<li>Shortcuts do nothing while you are typing.</li>
							</ul>
						</Popover.Content>
					</Popover.Root>
					<Tooltip text="Every turn in this list and the label you chose">
						{#snippet child({ props })}
							<Button {...props} variant="ghost" onclick={() => (sheetOpen = true)}
								><List aria-hidden="true" />All turns</Button
							>
						{/snippet}
					</Tooltip>
					<Popover.Root bind:open={filtersOpen}>
						<Popover.Trigger>
							{#snippet child({ props })}
								<Button {...props} variant="ghost"
									><ListFilter aria-hidden="true" /><span class="jd-swap"
										><span class:jd-off={filterCount > 0}>Filters</span><span
											class:jd-off={filterCount === 0}>Filters · {filterCount}</span
										></span
									></Button
								>
							{/snippet}
						</Popover.Trigger>
						<Popover.Content side="bottom" align="end" class="jd-filters">
							<EvalChoiceGroup
								label="The label says"
								choices={labelChoices(mode)}
								value={filters.gold}
								onChange={(v) => setFilter('gold', v)}
							/>
							<EvalChoiceGroup
								label="The judge says"
								choices={labelChoices(mode)}
								value={filters.judge}
								onChange={(v) => setFilter('judge', v)}
							/>
							<label class="jd-switch">
								<Switch bind:checked={() => filters.agree, (next) => setFilter('agree', next)} />
								Include turns where they agree
							</label>
							<div class="jd-filters-foot">
								<Button variant="outline" size="sm" onclick={clearFilters}>Clear filters</Button>
							</div>
						</Popover.Content>
					</Popover.Root>
				</div>
			</div>

			{#if queueError}
				<p class="jd-error" role="alert">The list of turns could not be loaded: {queueError}</p>
			{/if}

			{#if filteredEmpty}
				<FilteredEmptyState
					icon={SearchX}
					filtered={filterCount > 0}
					emptyText="No turns in this accuracy run differ on this question."
					filteredText="No turns match these filters."
					onClearFilters={clearFilters}
				/>
			{:else if tabDone}
				<div class="jd-done" data-testid="tab-done">
					<p class="jd-done-title">
						All {total} turns that differ on {ROW_LABEL[mode].toLowerCase()} are reviewed.
					</p>
					<div class="jd-done-actions">
						{#if otherLeft}
							<Button variant="outline" onclick={() => setMode(otherMode)}
								>{ROW_LABEL[otherMode]} · {otherLeft} left</Button
							>
						{/if}
						<Button variant="outline" href={resolve('/sessions/labelling/accuracy')}
							>Labelling accuracy</Button
						>
					</div>
					<p class="jd-quiet">To look at one again, open All turns.</p>
					{#if status}
						<p class="jd-quiet" data-tone={status.tone}>
							<span class="jd-status-text">{status.text}</span>
							{#if status.goBack !== undefined}{@const back = status.goBack}<button
									type="button"
									class="jd-link"
									onclick={() => void open(back, null, true)}>Go back</button
								>{/if}
						</p>
					{/if}
				</div>
			{:else if detailError}
				<p class="jd-error" role="alert">
					This turn could not be loaded: {detailError}.
					<button type="button" class="jd-link" onclick={retryDetail}>Try again</button>
				</p>
			{:else if shown && item}
				<div class="jd-question">
					<p class="jd-meta" data-testid="turn-meta">
						{#if loadingTurn !== null}
							<span>Loading the next turn</span>
						{:else}
							{#if position}<span>Turn {position.index} of {position.total}</span>{/if}
							{#if startedAt}<span>{startedAt}</span>{/if}
						{/if}
					</p>
					{#each questions as axis, i (axis)}
						{#if i === 0}
							<h2 class="jd-h2" data-question tabindex="-1">{QUESTION[axis]}</h2>
						{:else}
							<h2 class="jd-h2">{QUESTION[axis]}</h2>
						{/if}
					{/each}
					{#if questions.length > 1}
						<p class="jd-quiet" data-testid="two-questions">
							Both questions differ on this turn. Answer each row below; the turn saves when both
							have an answer.
						</p>
					{/if}
					{#if judgeMissedLast(judgeSaw)}
						<p class="jd-warn" role="note" data-testid="judge-missed">
							The judge did not see the assistant's last message to you, so its answer on this turn
							rests on incomplete input. Decide from what happened, not from the judge's answer.
						</p>
					{/if}
				</div>

				<div class="jd-work">
					<div class="jd-spine edge mech-group" data-testid="spine">
						{#key shown.turn}
							{#if sessionUuid && session}
								<EvalSessionContext
									{sessionUuid}
									summary={session.summary}
									summaryError={session.summaryError}
									turns={session.turns}
									turnsError={session.turnsError}
									{position}
									open={bandOpen}
									onToggle={toggleBand}
									onRetry={retrySession}
								/>
							{/if}
							<EvalTurnEvidence
								context={shown.detail.context}
								{facts}
								keyAxes={questions}
								{cited}
								turnNumber={position?.index ?? null}
								nextTurnNumber={nextPosition?.index ?? null}
								{turnHref}
								onRetryFacts={retryFacts}
							/>
						{/key}
					</div>
					<aside class="jd-rail" aria-label="The competing answers">
						<div class="edge mech-group jd-answers">
							{#key shown.turn}
								{#each barQuestions as q (q.axis)}
									{#if barQuestions.length > 1}
										<div class="band-hd band-hd--static">
											<h3 class="machine jd-group-title">{ROW_LABEL[q.axis]}</h3>
										</div>
									{/if}
									{#each q.candidates as c (c.label)}
										{@const extra = cardProps(q.axis, c)}
										<EvalAnswerCard
											axis={q.axis}
											label={c.label}
											holders={c.holders}
											definition={definition(q.axis, c.label)}
											labelledBy={extra.labelledBy}
											why={extra.why}
											verdict={item.verdict}
											{answeredBy}
											{judgeSaw}
											notGraded={null}
										/>
									{/each}
									{#if judgeOn(item, q.axis) === null}
										<EvalAnswerCard
											axis={q.axis}
											label={null}
											holders={['judge']}
											definition={null}
											labelledBy={null}
											why={[]}
											verdict={item.verdict}
											{answeredBy}
											judgeSaw={null}
											notGraded={item.outcome}
										/>
									{/if}
								{/each}
							{/key}
						</div>
						{#each notes as n (n)}<p class="jd-note" data-testid="other-axis-note">{n}</p>{/each}
					</aside>
				</div>

				{#key shown.turn}
					<EvalDecisionBar
						bind:this={bar}
						bind:note
						bind:height={panelHeight}
						questions={barQuestions}
						savedAt={saved?.at ?? null}
						busy={busy || loadingTurn !== null}
						{status}
						onDecide={(answers) => void decide(answers)}
						onStatus={(s) => (status = s)}
						onGoBack={(turn) => void open(turn, null, true)}
					/>
				{/key}
			{:else if queueState !== 'error'}
				<p class="jd-quiet" role="status">
					{selected !== null ? 'Loading the turn' : 'Loading turns'}
				</p>
			{/if}
		{/if}
	</div>
</div>

<Sheet.Root bind:open={sheetOpen}>
	<Sheet.Content>
		<div class="jd-sheet">
			<Sheet.Title>{ROW_LABEL[mode]} · {runShort}</Sheet.Title>
			<Sheet.Description>{reviewed} of {total} reviewed</Sheet.Description>
			<EvalTurnList
				items={queue}
				{mode}
				{selected}
				info={rowInfo}
				onVisible={(turn) => void rowVisible(turn)}
				onSelect={pickFromList}
			/>
		</div>
	</Sheet.Content>
</Sheet.Root>

<style>
	.jd-scroll {
		flex: 1 0 auto;
		background: var(--surface-bg);
		container-type: inline-size;
	}
	.jd {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		padding: var(--space-5) var(--space-6) 0;
		max-width: 1600px;
	}
	.jd-head {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.jd-title-row {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-2) var(--space-3);
	}
	.jd-h1 {
		margin: 0;
		font-family: var(--font-display);
		font-size: var(--text-2xl);
		font-weight: var(--weight-display);
		line-height: var(--leading-tight);
		letter-spacing: var(--track-display);
		color: var(--text-strong);
	}
	.jd-run-link {
		color: var(--interactive-active-fg);
		text-decoration: underline;
		text-underline-offset: var(--space-1);
	}
	.jd-run-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.jd-run-words {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.jd-runs {
		margin-left: auto;
	}
	.jd-lede {
		margin: 0;
		max-width: 75ch;
		font-size: var(--text-md);
		line-height: var(--leading-body);
		color: var(--text-2);
	}
	.jd-lead {
		color: var(--text-1);
	}
	.jd-error {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--status-danger-fg);
	}
	.jd-quiet {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.jd-warn {
		margin: 0;
		max-width: 80ch;
		padding: var(--space-2) var(--space-3);
		border: var(--hairline) solid var(--status-warning-border);
		background: var(--status-warning-bg);
		font-size: var(--text-sm);
		color: var(--status-warning-fg);
	}
	.jd-quiet[data-tone='success'] .jd-status-text {
		color: var(--status-success-fg);
	}
	.jd-quiet[data-tone='danger'] .jd-status-text {
		color: var(--status-danger-fg);
	}
	.jd-link {
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		color: var(--interactive-active-fg);
		cursor: pointer;
	}
	.jd-link:hover {
		text-decoration: underline;
	}
	.jd-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.jd-bar {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-end;
		gap: var(--space-2) var(--space-4);
		border-bottom: var(--hairline) solid var(--border-subtle);
	}
	.jd-bar :global([data-slot='tabs-list']) {
		border-bottom: 0;
	}
	.jd-count {
		margin-left: var(--space-2);
		color: var(--text-2);
	}
	.jd-progress {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		min-width: 12rem;
		padding-bottom: var(--space-2);
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.jd-progress strong {
		color: var(--text-1);
	}
	.jd-track {
		display: block;
		height: 4px;
		background: var(--surface-2);
	}
	.jd-fill {
		display: block;
		height: 100%;
		background: var(--status-success-fg);
		transition: width var(--dur-2) var(--ease-io);
	}
	.jd-tools {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
		margin-left: auto;
		padding-bottom: var(--space-2);
	}
	.jd-swap {
		display: inline-grid;
	}
	.jd-swap > span {
		grid-area: 1 / 1;
	}
	.jd-off {
		visibility: hidden;
	}
	:global([data-slot='popover-content'].jd-filters),
	:global([data-slot='popover-content'].jd-keys) {
		inline-size: min(30rem, calc(100vw - 2 * var(--space-4)));
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}
	.jd-keys-title {
		margin: 0;
		font-size: var(--text-xs);
		font-weight: var(--weight-body);
		color: var(--text-1);
	}
	.jd-keys-list {
		margin: 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	.jd-keys-list kbd {
		font-family: var(--font-mono);
		color: var(--interactive-active-fg);
	}
	.jd-switch {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		align-self: flex-start;
		font-size: var(--text-sm);
		color: var(--text-1);
		cursor: pointer;
	}
	.jd-filters-foot {
		display: flex;
		justify-content: flex-end;
	}
	.jd-question {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.jd-meta {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-1) var(--space-3);
		margin: 0;
		min-block-size: calc(1em * var(--leading-body));
		font-size: var(--text-sm);
		line-height: var(--leading-body);
		color: var(--text-2);
	}
	.jd-h2 {
		margin: 0;
		font-family: var(--font-display);
		font-size: var(--text-xl);
		font-weight: var(--weight-display);
		line-height: var(--leading-tight);
		color: var(--text-1);
	}
	.jd-h2:focus {
		outline: none;
	}
	.jd-work {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 24rem;
		gap: var(--space-5);
		align-items: start;
	}
	.jd-spine {
		--edge: var(--edge-sm);
		min-width: 0;
	}
	.jd-rail {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		position: sticky;
		top: var(--space-4);
		max-block-size: calc(100dvh - var(--jd-panel-h, 0px) - 2 * var(--space-4));
		overflow-y: auto;
		min-width: 0;
	}
	.jd-answers {
		--edge: var(--edge-sm);
		min-width: 0;
	}
	.jd-group-title {
		margin: 0;
		font-size: var(--text-xs);
		font-weight: var(--weight-body);
		color: var(--text-1);
	}
	.jd-note {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.jd-done {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-3);
		padding: var(--space-5) 0;
	}
	.jd-done-title {
		margin: 0;
		font-family: var(--font-display);
		font-size: var(--text-xl);
		font-weight: var(--weight-display);
		color: var(--text-1);
	}
	.jd-done-actions {
		display: flex;
		gap: var(--space-2);
	}
	.jd-sheet {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-4) 0;
	}
	.jd-sheet :global([data-slot='sheet-title']),
	.jd-sheet :global([data-slot='sheet-description']) {
		padding-inline: var(--space-4);
	}
	@container (max-width: 900px) {
		.jd-work {
			grid-template-columns: minmax(0, 1fr);
		}
		.jd-rail {
			position: static;
			order: -1;
			max-block-size: none;
			overflow-y: visible;
		}
	}
	@container (max-width: 640px) {
		.jd {
			padding: var(--space-4) var(--space-3) 0;
		}
		.jd-tools {
			margin-left: 0;
		}
		.jd-pager-text {
			display: none;
		}
	}
</style>
