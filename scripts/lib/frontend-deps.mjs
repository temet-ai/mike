/**
 * frontend-deps.mjs — resolve the design-token toolchain (culori) from
 * frontend/node_modules, so the repo-root scripts/*.mjs token
 * tooling can import them without those packages being installed at the repo
 * root. ESM bare-specifier resolution walks up from the IMPORTING file, which
 * for scripts/ would miss frontend/node_modules; createRequire anchored at
 * frontend/package.json fixes that.
 */
import { createRequire } from 'node:module';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const FRONTEND_PKG = resolve(__dirname, '../../frontend/package.json');

const frontendRequire = createRequire(FRONTEND_PKG);

/** Dynamic-import an ESM package resolved from frontend/node_modules. */
export async function importFrontend(spec) {
	const resolved = frontendRequire.resolve(spec);
	return import(pathToFileURL(resolved).href);
}

export const FRONTEND_ROOT = resolve(__dirname, '../../frontend');
