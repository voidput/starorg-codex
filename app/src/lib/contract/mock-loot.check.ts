/**
 * Type-checks mock-loot.gen.json (the Rust mock's Loot answers, written by
 * `UPDATE_CONTRACT_SAMPLES=1 cargo test -p starorg-codex-app`) against the TS mirror, as
 * samples.check.ts does for samples.gen.json. String-union values are checked by the Rust
 * test `loot_unions_match_types_ts`.
 */
import lootData from './mock-loot.gen.json';
import type { LootMockData } from './mock-loot';

/** String unions become `string` (JSON imports infer `string`), structure stays. */
type Widen<X> = X extends string
  ? string
  : X extends number
    ? number
    : X extends boolean
      ? boolean
      : X extends null
        ? null
        : X extends readonly (infer U)[]
          ? Widen<U>[]
          : X extends object
            ? { [K in keyof X]: Widen<X[K]> }
            : X;

export const checkedLoot: Widen<LootMockData> = lootData;
