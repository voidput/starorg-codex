/**
 * Character labels for the Blueprints screen, the import dialog and the re-import card. Two
 * characters with one name (a reset that gave a new id the logs cannot tie to the old one)
 * are told apart by "newest" or "last seen <date>". Run the tests: npm test.
 */
/** What a label needs: the CharacterView of the screens or the finder's character. */
type Named = { id: string; name: string; last_seen: string | null };

const date = (iso: string | null): string => {
  if (!iso) return 'never';
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
};

/** The name, plus "newest" or "last seen <date>" when another character has the same name. */
export function characterLabel(c: Named, all: Named[]): string {
  const key = c.name.trim().toLowerCase();
  const same = (Array.isArray(all) ? all : []).filter((x) => x.name.trim().toLowerCase() === key);
  if (same.length < 2) return c.name;
  const newest = same.reduce((a, b) => ((b.last_seen ?? '') > (a.last_seen ?? '') ? b : a));
  return newest.id === c.id ? `${c.name} · newest` : `${c.name} · last seen ${date(c.last_seen)}`;
}

const labelOf = (all: Named[], id: string): string => {
  const c = (Array.isArray(all) ? all : []).find((x) => x.id === id);
  return c ? characterLabel(c, all) : id;
};

/** Hint when auto passed over the newest character (it owns nothing), else null. */
export function fallbackHint(all: Named[], resolved: string | null, from: string | null): string | null {
  if (!from || !resolved) return null;
  return `Using ${labelOf(all, resolved)}'s blueprints: the newest character, ${labelOf(all, from)}, has none. Pick one under One character, or choose Any of my characters.`;
}

/** Hint when another character owns far more than the one auto uses, else null. */
export function largerHint(all: Named[], resolved: string | null, larger: string | null): string | null {
  if (!larger || !resolved) return null;
  return `${labelOf(all, larger)} owns far more blueprints than ${labelOf(all, resolved)}, which [Owned] uses now. If both are you, choose Any of my characters.`;
}

/** The character with the newest session (ties: the first), or null. */
export function newestId(all: Named[]): string | null {
  const list = Array.isArray(all) ? all : [];
  if (!list.length) return null;
  return list.reduce((a, b) => ((b.last_seen ?? '') > (a.last_seen ?? '') ? b : a)).id;
}
