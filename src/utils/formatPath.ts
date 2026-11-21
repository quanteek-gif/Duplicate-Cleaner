/**
 * Keep the tail of a path. History tables are narrow and `truncate` cuts the
 * end, which is exactly the part that names the file — "/tmp/swee…" tells the
 * user nothing. The leading ellipsis is plain text, so the path is never
 * reordered (a right-to-left trick reorders the slashes and misrepresents it).
 */
export function shortenPath(path: string, max = 64): string {
  if (path.length <= max) return path;
  return `…${path.slice(path.length - max + 1)}`;
}
