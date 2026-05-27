/** Trim long folder / project names for compact UI. Full value goes in `title`. */
export function truncateProject(name: string, max = 18): string {
  const t = name.trim();
  if (!t) return "";
  if (t.length <= max) return t;
  return `${t.slice(0, max - 1)}…`;
}
