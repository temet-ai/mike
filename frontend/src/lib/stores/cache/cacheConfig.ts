export type BodyKind = 'request' | 'request_sent' | 'response';

export const CURRENT_SCHEMA_VERSION = 1;

export interface CacheConfig {
	enabled: boolean;

	bodyMemoryEntryCap: number;
	bodyMemoryByteCap: number;
	detailMemoryEntryCap: number;
	sessionPageMemoryEntryCap: number;
	apiResponseMemoryEntryCap: number;

	idbDatabaseName: string;
	idbVersion: number;
	schemaVersion: number;
	idbBodyBudgetBytes: number;
	idbDetailCap: number;
	idbSessionPageCap: number;
	apiResponseBudgetBytes: number;
}

export const defaultCacheConfig: CacheConfig = {
	enabled: true,

	bodyMemoryEntryCap: 200,
	bodyMemoryByteCap: 256 * 1024 * 1024,
	detailMemoryEntryCap: 200,
	sessionPageMemoryEntryCap: 200,
	apiResponseMemoryEntryCap: 400,

	idbDatabaseName: 'temet-inspector-cache',
	idbVersion: 2,
	schemaVersion: CURRENT_SCHEMA_VERSION,
	idbBodyBudgetBytes: 512 * 1024 * 1024,
	idbDetailCap: 5000,
	idbSessionPageCap: 2000,
	apiResponseBudgetBytes: 64 * 1024 * 1024
};

let activeCacheConfig: CacheConfig = { ...defaultCacheConfig };

export function getCacheConfig(): CacheConfig {
	return activeCacheConfig;
}

export function configureCacheFromSettings(partial: Partial<CacheConfig>): void {
	activeCacheConfig = { ...activeCacheConfig, ...partial };
}

export function __resetCacheConfigForTests(): void {
	activeCacheConfig = { ...defaultCacheConfig };
}
