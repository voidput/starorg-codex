/**
 * Windowed lists (fixed-height rows): which rows to draw, and where to scroll so a row the
 * keyboard moved to is in view. Shared by VirtualList and the screens that window a list of
 * their own (Diff records, Explorer panes). Plain TS, unit-tested in tests/virtual.test.ts.
 */

/** The rows a windowed list draws: `[first, end)` around the visible part, `overscan` rows either side. */
export function windowRange(scrollTop: number, viewHeight: number, rowHeight: number, total: number, overscan = 6): { first: number; end: number } {
  if (total <= 0 || rowHeight <= 0) return { first: 0, end: 0 };
  const top = Math.max(0, scrollTop);
  const first = Math.max(0, Math.floor(top / rowHeight) - overscan);
  const end = Math.min(total, Math.ceil((top + Math.max(0, viewHeight)) / rowHeight) + overscan);
  return { first: Math.min(first, end), end };
}

/**
 * The scroll position that brings row `i` into view, or null when it already is. Rows above
 * the view align to the top, rows below to the bottom (as `scrollIntoView({ block: 'nearest' })`,
 * which a windowed list cannot use: the row may not be in the DOM yet).
 */
export function revealTop(i: number, rowHeight: number, scrollTop: number, viewHeight: number): number | null {
  if (i < 0 || rowHeight <= 0) return null;
  const top = i * rowHeight;
  if (top < scrollTop) return top;
  if (top + rowHeight > scrollTop + viewHeight) return Math.max(0, top + rowHeight - viewHeight);
  return null;
}
