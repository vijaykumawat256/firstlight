/** "just now", "5 min ago", "3 h ago", "2 d ago". */
export function timeAgo(iso: string | null, now: Date = new Date()): string {
  if (!iso) return "never";
  const mins = Math.floor((now.getTime() - new Date(iso).getTime()) / 60_000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins} min ago`;
  const h = Math.floor(mins / 60);
  if (h < 24) return `${h} h ago`;
  return `${Math.floor(h / 24)} d ago`;
}
