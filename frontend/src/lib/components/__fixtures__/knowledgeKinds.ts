import type {
	KnowledgeKind,
	KnowledgeKindPreviewRow,
	KnowledgeTable
} from '$lib/api/knowledgeKinds';

/** One kind row as `GET /api/knowledge/kinds` returns it; override what a story needs. */
export function knowledgeKind(
	over: Partial<Omit<KnowledgeKind, 'stats'>> & {
		kind: string;
		stats?: Partial<KnowledgeKind['stats']>;
	}
): KnowledgeKind {
	const { stats, ...rest } = over;
	return {
		label: over.kind,
		description: '',
		built_in: true,
		enabled: true,
		settle_secs: 30,
		body_view: null,
		generated_view: null,
		definition: null,
		watches: [],
		behind: false,
		...rest,
		stats: {
			items: 0,
			embedded: 0,
			waiting: 0,
			failed: 0,
			newest_error: null,
			newest_error_at: null,
			oldest_waiting_ready_at: null,
			...stats
		}
	};
}

const MINUTE = 60_000;
const iso = (offsetMs: number): string => new Date(Date.now() + offsetMs).toISOString();

/** Three built-in kinds (one off, one failing, one settling) and one custom kind, unsorted. */
export function knowledgeKindsFixture(): KnowledgeKind[] {
	return [
		knowledgeKind({
			kind: 'turn',
			label: 'Session turns',
			description: 'Operator and assistant turns, embedded so recall can find earlier decisions.',
			settle_secs: 30,
			body_view: 'knowledge.turn_bodies',
			watches: [{ table: 'sessions.turns', key_column: 'id', watch_columns: ['text'] }],
			stats: { items: 12840, embedded: 12840 }
		}),
		knowledgeKind({
			kind: 'lesson',
			label: 'Lessons',
			description: 'Recorded corrections.',
			enabled: false,
			settle_secs: 60,
			watches: [{ table: 'lessons.lessons', key_column: 'id', watch_columns: ['body', 'title'] }],
			stats: { items: 214, embedded: 200, waiting: 14 }
		}),
		knowledgeKind({
			kind: 'artifact',
			label: 'Artifacts',
			description: 'Project documents.',
			settle_secs: 120,
			watches: [
				{ table: 'knowledge.artifacts', key_column: 'id', watch_columns: ['body'] },
				{ table: 'knowledge.artifact_sections', key_column: 'id', watch_columns: ['text'] }
			],
			behind: true,
			stats: {
				items: 96,
				embedded: 80,
				waiting: 9,
				failed: 7,
				newest_error:
					'embedding server answered 500: model "embeddinggemma" is not loaded\nretry after the server reports ready',
				newest_error_at: iso(-12 * MINUTE),
				oldest_waiting_ready_at: iso(-9 * MINUTE)
			}
		}),
		knowledgeKind({
			kind: 'invoice',
			label: 'Invoices',
			description: 'Supplier invoices from the finance schema.',
			built_in: false,
			settle_secs: 45,
			definition: {
				source_table: 'finance.invoices',
				key_column: 'id',
				title_column: 'supplier',
				body_columns: ['notes', 'lines'],
				project_column: 'project_id'
			},
			watches: [{ table: 'finance.invoices', key_column: 'id', watch_columns: ['notes', 'lines'] }],
			stats: { items: 31, embedded: 28, waiting: 3, oldest_waiting_ready_at: iso(2 * MINUTE) }
		})
	];
}

export const KNOWLEDGE_TABLES: KnowledgeTable[] = [
	{
		name: 'finance.invoices',
		columns: [
			{ name: 'id', type: 'uuid' },
			{ name: 'supplier', type: 'text' },
			{ name: 'notes', type: 'text' },
			{ name: 'lines', type: 'jsonb' },
			{ name: 'project_id', type: 'uuid' }
		]
	},
	{
		name: 'finance.credits',
		columns: [
			{ name: 'id', type: 'uuid' },
			{ name: 'memo', type: 'text' }
		]
	}
];

export const KNOWLEDGE_PREVIEW_ROW: KnowledgeKindPreviewRow = {
	reference: 'finance.invoices:7c1e',
	header: 'Acme Ltd',
	body: 'Delivered late, credit note agreed.\n3 lines: pallets, wrap, haulage.'
};
