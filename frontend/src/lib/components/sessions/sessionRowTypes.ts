import type { SearchMatchSnippet } from '$lib/types';

export interface DetailMetrics {
	loading: boolean;
	error: string | null;
	model: string | null;
	inTokens: number | null;
	outTokens: number | null;
	cachePct: number | null;
	costStr: string;
	costEstimated: boolean;
	stop: string | null;
	partial: boolean;
}

export interface SessionMatchInfo {
	titleHit: boolean;
	conversationCount: number;
	bodyCount: number;
	conversationSnippets: SearchMatchSnippet[];
	bodySnippets: SearchMatchSnippet[];
	conversationTruncated: boolean;
}
