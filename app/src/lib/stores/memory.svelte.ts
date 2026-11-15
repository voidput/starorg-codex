/**
 * The memory limit of the Developer mode and MCP workers (addendum 7.18), shared by the
 * Settings row, the warning dialog and the short reminders. Read on first use and again
 * whenever the warning opens (free memory changes).
 */
import { api } from '@/lib/contract/client';
import type { MemoryStatus } from '@/lib/contract/types';

export const memory = $state<{ status: MemoryStatus | null }>({ status: null });

let loading: Promise<MemoryStatus | null> | null = null;

/** Fresh numbers (free memory moves); errors leave the last status in place. */
export function loadMemory(): Promise<MemoryStatus | null> {
  loading ??= api
    .memoryStatus()
    .then((s) => (memory.status = s))
    .catch(() => memory.status)
    .finally(() => (loading = null));
  return loading;
}

/** Load once for a reminder line. */
export function ensureMemory(): void {
  if (!memory.status) void loadMemory();
}

export async function saveMemoryLimit(gb: number | null): Promise<MemoryStatus> {
  const s = await api.setMemoryLimit(gb);
  memory.status = s;
  return s;
}
