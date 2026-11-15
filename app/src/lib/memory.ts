/**
 * Memory limit of the Developer mode and MCP workers (addendum 7.17): wording shared by the
 * warning dialog, the Settings row and the short reminders.
 */
import type { MemoryEnforcement, MemoryStatus } from './contract/types';

const GIB = 1024 ** 3;

/** "1.5 GB", "4 GB", "0.9 GB", "64 MB" (as Rust's `memlimit::human`). */
export function gbText(bytes: number): string {
  if (bytes < GIB / 10) return `${Math.round(bytes / 1024 ** 2)} MB`;
  const s = (bytes / GIB).toFixed(1);
  return `${s.endsWith('.0') ? s.slice(0, -2) : s} GB`;
}

/** A slider value in GB as text: "4 GB", "6.5 GB". */
export function limitText(gb: number): string {
  return gbText(gb * GIB);
}

export function enforcementText(e: MemoryEnforcement): string {
  switch (e) {
    case 'cgroup':
      return 'Each worker is stopped cleanly at 90% of the limit; a hard cap (cgroup, opt-in) sits 10% (at least 512 MB) above it.';
    case 'rlimit':
      return 'Each worker is stopped cleanly at 90% of the limit; a hard cap (RLIMIT_DATA) sits 10% (at least 512 MB) above it.';
    case 'watchdog':
      return 'Each worker is stopped cleanly at 90% of the limit (Windows has no hard cap here).';
  }
}

/** "32 GB RAM, 18.4 GB free now" (or what of it is known). */
export function machineText(s: Pick<MemoryStatus, 'total_bytes' | 'available_bytes'>): string {
  const parts: string[] = [];
  if (s.total_bytes !== null) parts.push(`${gbText(s.total_bytes)} RAM`);
  if (s.available_bytes !== null) parts.push(`${gbText(s.available_bytes)} free now`);
  return parts.length ? parts.join(', ') : 'RAM size unknown';
}

/** The one-line reminder on Dev screens and the MCP status. */
export function reminderText(s: Pick<MemoryStatus, 'limit_gb'> | null): string {
  return s ? `Can use several GB of RAM; capped at ${limitText(s.limit_gb)} (Settings > Advanced).` : 'Can use several GB of RAM.';
}

/** How many of the measured peaks fit under the limit. */
export function fitsCount(s: Pick<MemoryStatus, 'estimates'>): [number, number] {
  const list = Array.isArray(s.estimates) ? s.estimates : [];
  return [list.filter((e) => e.fits).length, list.length];
}

/** The view for a saved limit (null = default) over a base status, as `MemoryStatus::build`. */
export function memoryView(base: MemoryStatus, gb: number | null): MemoryStatus {
  const limit = gb ?? base.default_gb;
  const bytes = limit * GIB;
  return {
    ...structuredClone(base),
    limit_gb: limit,
    is_default: gb === null,
    estimates: (Array.isArray(base.estimates) ? base.estimates : []).map((e) => ({ ...e, fits: e.bytes <= bytes })),
  };
}

/** Rounded to the step and kept inside the slider (`Bounds::clamp`). */
export function clampGb(gb: number, s: Pick<MemoryStatus, 'min_gb' | 'max_gb' | 'step_gb' | 'default_gb'>): number {
  if (!Number.isFinite(gb)) return s.default_gb;
  return Math.min(s.max_gb, Math.max(s.min_gb, Math.round(gb / s.step_gb) * s.step_gb));
}
