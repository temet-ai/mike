import { vi } from 'vitest';

export interface LocalStorageMock {
	storage: Map<string, string>;
	localStorageMock: {
		getItem: ReturnType<typeof vi.fn<(key: string) => string | null>>;
		setItem: ReturnType<typeof vi.fn<(key: string, value: string) => void>>;
		removeItem: ReturnType<typeof vi.fn<(key: string) => void>>;
		clear: ReturnType<typeof vi.fn<() => void>>;
		readonly length: number;
		key: ReturnType<typeof vi.fn<(index: number) => string | null>>;
	};
}

export function createLocalStorageMock(): LocalStorageMock {
	const storage = new Map<string, string>();
	const localStorageMock = {
		getItem: vi.fn((key: string) => storage.get(key) ?? null),
		setItem: vi.fn((key: string, value: string) => {
			storage.set(key, value);
		}),
		removeItem: vi.fn((key: string) => {
			storage.delete(key);
		}),
		clear: vi.fn(() => {
			storage.clear();
		}),
		get length() {
			return storage.size;
		},
		key: vi.fn((_: number) => null)
	};
	return { storage, localStorageMock };
}
