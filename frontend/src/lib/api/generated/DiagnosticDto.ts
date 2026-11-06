import type { DiagnosticCategory } from './DiagnosticCategory';

export type DiagnosticDto = {
	category: DiagnosticCategory;
	analyzer: string;
	advice: string;
	matched_text: string;
	match_range: [number, number];
	other_hits: number;
	created_at: string;
};
