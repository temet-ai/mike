import { resolve } from '$app/paths';
import type { ResolvedPathname, RouteId, RouteParams } from '$app/types';

/** A route with no dynamic segment, such as `/agents` or `/settings/workers`. */
type StaticRouteId = {
	[K in RouteId]: RouteParams<K> extends Record<string, never> ? K : never;
}[RouteId];

/** A parameterless route, optionally followed by a search string or hash. */
export type StaticHref =
	StaticRouteId | `${StaticRouteId}?${string}` | `${StaticRouteId}#${string}`;

/**
 * Resolve a link that is one of many parameterless routes.
 *
 * `resolve()` types its arguments as one tuple per route, and TypeScript cannot match a
 * union of more than a handful of routes against that set of tuples, so a table of links
 * (the command palette, the home destination cards) cannot call it directly.
 * `StaticHref` already guarantees the route exists and takes no parameters; the one cast
 * below only tells `resolve()` which of its tuple shapes the argument has.
 */
export function resolveStatic(href: StaticHref): ResolvedPathname {
	return resolve(...([href] as Parameters<typeof resolve<'/'>>));
}

/**
 * Resolve an app path that arrives as text, such as one inside a fix written by the proxy.
 * The path cannot be checked against the route table at compile time; `resolve()` still
 * applies the base path to it.
 */
export function resolveAppPath(path: string): ResolvedPathname {
	return resolve(...([path] as Parameters<typeof resolve<'/'>>));
}

/**
 * A pathname that is already resolved, with a search string appended, for `goto()` and
 * `replaceState()` that rewrite the current page's query in place. `page.url.pathname` carries
 * the base path (SvelteKit documents it as a resolved pathname), so it must not go through
 * `resolve()` again; its type only widens it to `string`, and this states what it is.
 */
export function resolvedPathWithSearch(pathname: string, search: string): ResolvedPathname {
	return `${pathname}${search}` as ResolvedPathname;
}

/**
 * A link that leaves the app's own routes: a full `http(s)` URL, or a path on the inspect API
 * (`/api/...`), which the same origin serves but SvelteKit does not route.
 */
export type ExternalHref = `http://${string}` | `https://${string}` | `/api/${string}`;

export function isExternalHref(href: string): href is ExternalHref {
	return /^(https?:\/\/|\/api\/)/.test(href);
}
