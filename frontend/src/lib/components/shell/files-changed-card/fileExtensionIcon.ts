import { ICON_DATA, type IconSvg } from './fileTypeIconData';

const KEY_BY_EXT: Record<string, string> = {
	rs: 'rust',
	svelte: 'svelte',
	ts: 'typescript',
	tsx: 'typescript',
	mts: 'typescript',
	cts: 'typescript',
	js: 'js',
	mjs: 'js',
	cjs: 'js',
	jsx: 'js',
	json: 'json',
	sql: 'sql',
	md: 'markdown',
	mdx: 'markdown',
	toml: 'toml',
	yaml: 'yaml',
	yml: 'yaml',
	css: 'css',
	html: 'html',
	sh: 'shell',
	bash: 'shell',
	go: 'go-gopher',
	py: 'python',
	txt: 'text',
	log: 'log',
	png: 'image',
	jpg: 'image',
	jpeg: 'image',
	gif: 'image',
	webp: 'image',
	svg: 'svg'
};

const DEFAULT_KEY = 'default';

export function iconForPath(path: string): IconSvg {
	const match = /\.([a-z0-9]+)$/i.exec(path);
	const ext = match ? match[1]!.toLowerCase() : '';
	const key = KEY_BY_EXT[ext] ?? DEFAULT_KEY;
	return ICON_DATA[key] ?? ICON_DATA[DEFAULT_KEY]!;
}
