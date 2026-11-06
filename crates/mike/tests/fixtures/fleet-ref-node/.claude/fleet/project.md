# This project: how it is built and checked

- The fleet's check is `scripts/check`: the type check, then every test.
- After an edit: `npm run typecheck`; one test: `node --test test/<name>.test.ts`.
- Source is TypeScript that Node runs directly: imports carry the `.ts` extension; no enums or namespaces.
- The quick gate and the full gate are `scripts/check`.
- Dependencies are installed in `node_modules`; nobody runs `npm install`.
