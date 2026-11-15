/**
 * Developer mode contract (addendum 7.7): types mirrored from app/src-tauri/src/contract/dev.rs
 * (and the core shapes it re-exports) plus the client calls. Re-exported by types.ts.
 * Every command fails with `dev-disabled` while Settings > Advanced > Developer mode is off.
 */
import { call } from './client';
import type { ChannelRef } from './types';

export type WorkerState = 'stopped' | 'starting' | 'ready' | 'failed';

export interface DevStatus {
  state: WorkerState;
  channel: string | null;
  build: string | null;
  records: number;
  main: number;
  types: number;
  started: string | null;
  start_ms: number | null;
  memory: number | null;
  error: string | null;
  /** The section index (7.17). */
  sections: SectionStats | null;
  /** The channel the explorer is compared with (7.17). */
  compared: DevCompare | null;
}

export type TreeRoot = 'type' | 'path' | 'section';
export type NodeKind = 'type' | 'folder' | 'record' | 'section';

/** How the section index was built (7.17). */
export interface SectionStats {
  build_ms: number;
  scan_ms: number;
  bytes: number;
  entities: number;
  models: number;
  sounds: number;
  /** Checked against the archive's files (false for a bare DCB). */
  verified: boolean;
}

export type SortKey = 'name' | 'size' | 'manufacturer';
export type ListGroup = 'none' | 'section' | 'kind' | 'manufacturer' | 'size';
export type Change = 'added' | 'changed';
/** How the search words matched: as typed, or (only when nothing did) close spellings or an abbreviation. */
export type Matching = 'exact' | 'typo' | 'abbreviation';

/** Filters of the item finder (7.17): OR within a facet, AND across facets. */
export interface Filters {
  sections: string[];
  kinds: string[];
  sub_types: string[];
  sizes: number[];
  manufacturers: string[];
  grades: string[];
  classes: string[];
  tags: string[];
  model: boolean;
  sounds: boolean;
  player_only: boolean;
  changes: string[];
}

export interface ListQuery {
  /** A Sections-view node id, or `''` for every record. */
  node: string;
  /** Words and prefixes: `type:` `maker:` `size:` `tag:` `section:` `grade:` `class:` `has:model` `has:sounds`. */
  query: string;
  filters: Filters;
  sort: SortKey;
  group: ListGroup;
  collapsed: string[];
  offset: number;
  limit: number;
  main_only: boolean;
}

export interface Stat {
  key: string;
  label: string;
  value: number;
  unit: string | null;
}

export interface ListItem {
  guid: string;
  name: string;
  label: string;
  detail: string | null;
  type: string;
  section: string;
  kind: string | null;
  kind_label: string | null;
  sub_type: string | null;
  sub_label: string | null;
  size: number | null;
  manufacturer: string | null;
  maker_label: string | null;
  grade: string | null;
  class: string | null;
  tag: string | null;
  model: boolean;
  sounds: boolean;
  variant: boolean;
  main: boolean;
  group: string | null;
  change: string | null;
  stats: Stat[];
}

export interface FacetValue {
  value: string;
  label: string;
  count: number;
}

export interface FacetCounts {
  sections: FacetValue[];
  kinds: FacetValue[];
  sub_types: FacetValue[];
  sizes: FacetValue[];
  manufacturers: FacetValue[];
  grades: FacetValue[];
  classes: FacetValue[];
  tags: FacetValue[];
  model: number;
  sounds: number;
  player: number;
  changes: FacetValue[];
}

export interface GroupHead {
  key: string;
  label: string;
  count: number;
  collapsed: boolean;
}

export interface ListPage {
  items: ListItem[];
  offset: number;
  total: number;
  matched: number;
  facets: FacetCounts;
  groups: GroupHead[];
  title: string;
  understood: string[];
  words: string[];
  matching: Matching;
}

export interface DevListRequest {
  target: ChannelRef;
  query: ListQuery;
}

export interface DevOwnerRequest {
  target: ChannelRef;
  path: string;
}

export interface DevCompareRequest {
  target: ChannelRef;
  other: string | null;
}

export interface DevCompare {
  other: string;
  added: number;
  changed: number;
  removed: number;
  marked: number;
  ms: number;
}

export interface TreeNode {
  id: string;
  label: string;
  kind: NodeKind;
  count: number;
  main: boolean;
}

export interface TreePage {
  nodes: TreeNode[];
  offset: number;
  total: number;
}

export interface SearchHit {
  guid: string;
  name: string;
  type: string;
  path: string;
  main: boolean;
}

export interface SearchPage {
  hits: SearchHit[];
  total: number;
}

export interface TypeCount {
  name: string;
  records: number;
  main: number;
}

export interface RecordHeader {
  guid: string;
  name: string;
  type: string;
  path: string;
  tag: string | null;
  main: boolean;
  bytes: number;
}

/** What a raw value reads as (addendum 7.14). */
export type LabelKind = 'record' | 'loc' | 'tag' | 'manufacturer';

export interface Readable {
  kind: LabelKind;
  /** Display name, stock text, tag name or manufacturer name. */
  text: string;
  /** Records with a display name: the short record name. */
  detail: string | null;
  /** Our enhanced text of the same key, when it differs from stock. */
  enhanced: string | null;
}

export interface LocText {
  key: string;
  text: string;
}

export interface RecordView {
  header: RecordHeader;
  json: string;
  pointer: string;
  truncated: boolean;
  loc: LocText[];
  /** Raw value (GUID, `@key`, name) -> what it reads as. */
  labels: Record<string, Readable>;
}

export interface DevTreeRequest {
  target: ChannelRef;
  root: TreeRoot;
  node: string;
  offset: number;
  limit: number;
  main_only: boolean;
  /** Sections view: hide AI, NPC, test and template variants. */
  player_only?: boolean;
}

export interface DevSearchRequest {
  target: ChannelRef;
  query: string;
  limit: number;
}

export interface DevRecordRequest {
  target: ChannelRef;
  what: string;
  pointer: string;
  depth: number | null;
}

export interface DevExportRecordRequest {
  target: ChannelRef;
  what: string;
}

export interface DevExportTypeRequest {
  target: ChannelRef;
  type_name: string;
  main_only: boolean;
}

export interface DevExportResult {
  cancelled: boolean;
  path: string | null;
  records: number;
  bytes: number;
}

export type SourceKind = 'build' | 'folder' | 'dump';
export type SourceState = 'ready' | 'off' | 'missing' | 'error';

export interface DevDatasetSummary {
  file: string;
  bytes: number;
  tables: number;
  rows: number;
}

export interface DevSource {
  id: string;
  kind: SourceKind;
  label: string;
  build: string | null;
  state: SourceState;
  detail: string | null;
  datasets: DevDatasetSummary[];
}

export interface DevSourceRef {
  target: ChannelRef;
  source: string;
}

export interface DevDatasetRef {
  target: ChannelRef;
  source: string;
  dataset: string;
}

export interface DevTableRef {
  target: ChannelRef;
  source: string;
  dataset: string;
  table: string;
}

export type ColType = 'text' | 'number' | 'bool' | 'object' | 'list' | 'mixed' | 'empty';

export interface ValueCount {
  value: string;
  count: number;
}

export interface Column {
  name: string;
  kind: ColType;
  filled: number;
  distinct: ValueCount[] | null;
  min: number | null;
  max: number | null;
}

export type TableKind = 'main' | 'index' | 'tree';

export interface TableSummary {
  name: string;
  kind: TableKind;
  rows: number;
}

export interface Fact {
  key: string;
  value: string;
}

export interface DatasetInfo {
  file: string;
  bytes: number;
  meta: Fact[];
  facts: Fact[];
  tables: TableSummary[];
}

export interface TableInfo {
  name: string;
  kind: TableKind;
  rows: number;
  columns: Column[];
  identity: string | null;
  default_columns: string[];
  /** Group-bys offered for the tree (addendum 7.15), the default first; empty = no tree. */
  group_presets: GroupPreset[];
}

/** One level of a group-by: a column's value, or (`folders`, last only) a path's folders. */
export interface GroupLevel {
  column: string;
  folders: boolean;
}

export interface GroupPreset {
  /** `chain`, `col:<column>` or `folders:<column>`. */
  id: string;
  label: string;
  levels: GroupLevel[];
}

/** Rows of one tree node; the id is the JSON array of keys from the root. */
export interface GroupPick {
  by: GroupLevel[];
  node: string;
}

export interface DiffCounts {
  added: number;
  removed: number;
  changed: number;
}

/** One visible node of a group tree. */
export interface DataGroup {
  id: string;
  parent: string | null;
  label: string;
  /** 1 = top level. */
  depth: number;
  /** Rows in the node matching the search and filters. */
  count: number;
  /** Child nodes (0 = leaf). */
  children: number;
  /** Children not listed (open nodes only). */
  more: number;
  /** Diff trees only. */
  diff: DiffCounts | null;
}

export interface GroupPage {
  nodes: DataGroup[];
  total: number;
  all: number;
}

export interface DevGroupsRequest {
  target: ChannelRef;
  source: string;
  dataset: string;
  table: string;
  by: GroupLevel[];
  open: string[];
  columns: string[];
  search: string;
  filters: RowFilter[];
}

/** How the Diff tree groups below dataset and table. */
export type DiffGroupBy = 'tables' | 'auto' | 'folders';

export interface DiffGroupPick {
  by: DiffGroupBy;
  node: string;
}

export interface DevDiffGroupsRequest {
  target: ChannelRef;
  by: DiffGroupBy;
  open: string[];
}

export interface RowFilter {
  column: string;
  values: string[];
  min: number | null;
  max: number | null;
  has: boolean | null;
}

export interface RowSort {
  column: string;
  desc: boolean;
}

export interface DevRowsRequest {
  target: ChannelRef;
  source: string;
  dataset: string;
  table: string;
  columns: string[];
  search: string;
  filters: RowFilter[];
  sort: RowSort | null;
  offset: number;
  limit: number;
  /** Only the rows of this tree node. */
  group: GroupPick | null;
}

export interface Row {
  id: number;
  cells: (string | null)[];
  /** What each cell reads as, same order; null = show the raw cell. */
  labels: (Readable | null)[];
  /** The row's display name. */
  title: string | null;
}

export interface RowsPage {
  columns: string[];
  rows: Row[];
  offset: number;
  total: number;
  all: number;
}

export interface DevRowRequest {
  target: ChannelRef;
  source: string;
  dataset: string;
  table: string;
  row: number;
}

export interface ExplorerLink {
  pointer: string;
  value: string;
  what: string;
}

export interface DevRowDetail {
  row: number;
  json: string;
  truncated: boolean;
  loc: LocText[];
  links: ExplorerLink[];
  labels: Record<string, Readable>;
}

export type Presence = 'both' | 'only-a' | 'only-b';
export type RecordState = 'added' | 'removed' | 'changed';

export interface DiffTable {
  dataset: string;
  table: string;
  presence: Presence;
  added: number;
  removed: number;
  changed: number;
  same: number;
  identity: string | null;
}

export interface DiffRecord {
  id: string;
  state: RecordState;
  fields: string[];
  a: number | null;
  b: number | null;
  /** The record's display name. */
  label: string | null;
}

export interface FieldChange {
  path: string;
  old: unknown;
  new: unknown;
  delta: number | null;
  percent: number | null;
  /** The path with its `[column=value]` segments read. */
  path_label: string | null;
  /** Shown under this group (the item or top-level field it belongs to). */
  group: string | null;
  unit: string | null;
  old_label: Readable | null;
  new_label: Readable | null;
}

export interface DevDiffRequest {
  target: ChannelRef;
  a: string;
  b: string;
}

export interface DevDiffSummary {
  a: string;
  b: string;
  tables: DiffTable[];
  ms: number;
}

export interface DevDiffRecordsRequest {
  target: ChannelRef;
  dataset: string;
  table: string;
  state: RecordState | null;
  field: string | null;
  identity: string | null;
  /** Only records whose id or display name contains this. */
  query: string | null;
  offset: number;
  limit: number;
  /** Only records in this Diff tree node. */
  group: DiffGroupPick | null;
}

export interface DevDiffRecords {
  summary: DiffTable;
  records: DiffRecord[];
  offset: number;
  total: number;
  fields: ValueCount[];
  identity_options: string[];
}

export interface DevDiffRecordRequest {
  target: ChannelRef;
  dataset: string;
  table: string;
  id: string;
  identity: string | null;
}

export interface DevFieldDiff {
  id: string;
  /** The record's display name. */
  label: string | null;
  state: RecordState;
  changes: FieldChange[];
}

/** The cut stub that stands for a node not sent (`{"_cut": {"kind", "len"}}`). */
export const CUT_KEY = '_cut';
/** A reference written by the explorer (`{"_ref": guid, "name", "type", "path"}`). */
export const REF_KEY = '_ref';
/** The export dialog warns above this many records (7.4). */
export const EXPORT_WARN_RECORDS = 5_000;
/** Most rows per reply (7.3). */
export const MAX_PAGE = 500;

/** Command name -> [arguments object, result], merged into client.ts's CommandMap. */
export interface DevCommandMap {
  dev_status: [{ req: ChannelRef }, DevStatus];
  dev_open: [{ req: ChannelRef }, DevStatus];
  dev_close: [{ req: ChannelRef }, DevStatus];
  dev_types: [{ req: ChannelRef }, TypeCount[]];
  dev_tree: [{ req: DevTreeRequest }, TreePage];
  dev_list: [{ req: DevListRequest }, ListPage];
  dev_owner: [{ req: DevOwnerRequest }, SearchHit[]];
  dev_compare: [{ req: DevCompareRequest }, DevCompare | null];
  dev_search: [{ req: DevSearchRequest }, SearchPage];
  dev_record: [{ req: DevRecordRequest }, RecordView];
  dev_export_record: [{ req: DevExportRecordRequest }, DevExportResult];
  dev_export_type: [{ req: DevExportTypeRequest }, DevExportResult];
  dev_sources: [{ req: ChannelRef }, DevSource[]];
  dev_add_source: [{ req: ChannelRef }, DevSource[]];
  dev_remove_source: [{ req: DevSourceRef }, DevSource[]];
  dev_download_dump: [{ req: ChannelRef }, DevSource[]];
  dev_dataset: [{ req: DevDatasetRef }, DatasetInfo];
  dev_table: [{ req: DevTableRef }, TableInfo];
  dev_rows: [{ req: DevRowsRequest }, RowsPage];
  dev_row: [{ req: DevRowRequest }, DevRowDetail];
  dev_groups: [{ req: DevGroupsRequest }, GroupPage];
  dev_diff: [{ req: DevDiffRequest }, DevDiffSummary];
  dev_diff_summary: [{ req: ChannelRef }, DevDiffSummary];
  dev_diff_records: [{ req: DevDiffRecordsRequest }, DevDiffRecords];
  dev_diff_groups: [{ req: DevDiffGroupsRequest }, GroupPage];
  dev_diff_record: [{ req: DevDiffRecordRequest }, DevFieldDiff];
}

/** Same order as the dev part of `COMMAND_NAMES` in contract/mod.rs. */
export const DEV_COMMANDS = [
  'dev_status',
  'dev_open',
  'dev_close',
  'dev_types',
  'dev_tree',
  'dev_list',
  'dev_owner',
  'dev_compare',
  'dev_search',
  'dev_record',
  'dev_export_record',
  'dev_export_type',
  'dev_sources',
  'dev_add_source',
  'dev_remove_source',
  'dev_download_dump',
  'dev_dataset',
  'dev_table',
  'dev_rows',
  'dev_row',
  'dev_groups',
  'dev_diff',
  'dev_diff_summary',
  'dev_diff_records',
  'dev_diff_groups',
  'dev_diff_record',
] as const satisfies readonly (keyof DevCommandMap)[];

export const devApi = {
  status: (req: ChannelRef) => call('dev_status', { req }),
  open: (req: ChannelRef) => call('dev_open', { req }),
  close: (req: ChannelRef) => call('dev_close', { req }),
  types: (req: ChannelRef) => call('dev_types', { req }),
  tree: (req: DevTreeRequest) => call('dev_tree', { req }),
  list: (req: DevListRequest) => call('dev_list', { req }),
  owner: (req: DevOwnerRequest) => call('dev_owner', { req }),
  compare: (req: DevCompareRequest) => call('dev_compare', { req }),
  search: (req: DevSearchRequest) => call('dev_search', { req }),
  record: (req: DevRecordRequest) => call('dev_record', { req }),
  /** Native save dialog (Rust side); `cancelled` when closed. */
  exportRecord: (req: DevExportRecordRequest) => call('dev_export_record', { req }),
  exportType: (req: DevExportTypeRequest) => call('dev_export_type', { req }),
  sources: (req: ChannelRef) => call('dev_sources', { req }),
  /** Native folder dialog (Rust side). */
  addSource: (req: ChannelRef) => call('dev_add_source', { req }),
  removeSource: (req: DevSourceRef) => call('dev_remove_source', { req }),
  downloadDump: (req: ChannelRef) => call('dev_download_dump', { req }),
  dataset: (req: DevDatasetRef) => call('dev_dataset', { req }),
  table: (req: DevTableRef) => call('dev_table', { req }),
  rows: (req: DevRowsRequest) => call('dev_rows', { req }),
  row: (req: DevRowRequest) => call('dev_row', { req }),
  groups: (req: DevGroupsRequest) => call('dev_groups', { req }),
  diff: (req: DevDiffRequest) => call('dev_diff', { req }),
  diffSummary: (req: ChannelRef) => call('dev_diff_summary', { req }),
  diffRecords: (req: DevDiffRecordsRequest) => call('dev_diff_records', { req }),
  diffGroups: (req: DevDiffGroupsRequest) => call('dev_diff_groups', { req }),
  diffRecord: (req: DevDiffRecordRequest) => call('dev_diff_record', { req }),
};
