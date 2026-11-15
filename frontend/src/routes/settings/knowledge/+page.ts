import type { PageLoad } from './$types';
import { getKnowledgeKinds } from '$lib/api/knowledgeKinds';
import type { KnowledgeKind } from '$lib/api/knowledgeKinds';

export interface KnowledgePageData {
	kinds: KnowledgeKind[] | null;
	loadError: string | null;
}

export const load: PageLoad = async (): Promise<KnowledgePageData> => {
	const [result] = await Promise.allSettled([getKnowledgeKinds()]);
	return {
		kinds: result.status === 'fulfilled' ? result.value : null,
		loadError:
			result.status === 'rejected'
				? result.reason instanceof Error
					? result.reason.message
					: String(result.reason)
				: null
	};
};
