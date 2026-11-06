import type { PageLoad } from './$types';
import { listConfigChanges } from '$lib/api/configHistory';
import type { ChangeLogPage } from '$lib/api/configHistory';

export interface HistoryPageData {
	page: ChangeLogPage | null;
	loadError: string | null;
}

export const load: PageLoad = async (): Promise<HistoryPageData> => {
	try {
		return { page: await listConfigChanges({ limit: 100 }), loadError: null };
	} catch (e) {
		return { page: null, loadError: e instanceof Error ? e.message : String(e) };
	}
};
