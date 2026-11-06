import { slugify } from "./slug.ts";

export function titleSlug(title: string, id: number): string {
  return `${id}-${slugify(title)}`;
}
