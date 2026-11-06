import type { SystemBlockProjection } from './SystemBlockProjection';

export type SystemBlocksProjection = {
	prompt_sha256: string | null;
	composition_id: string | null;
	composition_version: bigint | null;
	blocks: Array<SystemBlockProjection>;
};
