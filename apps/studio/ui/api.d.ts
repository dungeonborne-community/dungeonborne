// Contract between the page and the app (src-tauri/src/lib.rs). The sandbox dev/mock-tauri.js imitates the same calls.

export interface Failure { code: string; message: string }

export interface TableInfo {
  name: string; title: string; about: string; where: string; layout: "wide" | "long";
  rows: number; values: number; editable_values: number;
  /** Compared with the original game: changed values, rows only we have, rows only the game has */
  changed: number; new_rows: number; removed_rows: number;
  has_orig: boolean; source: "base" | "season" | "";
  /** Heading in the list of tables; "" — an old kit without groups */
  group: string;
}

export interface EditStatus {
  count: number; errors: number; warnings: number; can_undo: boolean; can_redo: boolean; comment: string;
  /** The change is applied, but the draft file could not be written */
  draft_error?: string;
}

export interface KitOverview {
  version: string; title: string; season: string | null; notes: string[]; has_catalog: boolean; tables: TableInfo[];
  /** Screens that need particular game tables */
  features: { items: boolean; curves: boolean };
  edits: EditStatus;
  /** Only from open_kit / open_recent: draft changes that did not fit the kit */
  draft_dropped?: number;
  /** Only from open_kit / open_recent: an older kit's draft can be moved here */
  rebase_from?: string;
}

export interface RecentKit { version: string; title: string; season: string | null; source: string }

export interface AppState { lang: "ru" | "en"; author: string; recent: RecentKit[]; kit: KitOverview | null; version: string }

export interface RowQuery {
  /** "" — the table's own layout */
  view: "" | "wide" | "long";
  search: string; only_diff: boolean; only_mine: boolean; all_fields: boolean; kind: string; rarity: string;
  /** "" — table order; "name" | "key"; wide: column id; long: "label" | "value" */
  sort: string; desc: boolean; lang: "ru" | "en";
}

export interface Column {
  id: number; path: string; label: string; editable: boolean;
  /** Holds a value, not a service field: only these are shown as percent */
  value_like: boolean;
  unit?: string; range?: string;
}

/**
 * v — what to show, with my change; o — value in the original game, only if different; n — the row is not in the original;
 * k — value in the kit, only if I changed it; e / w — code of an error / warning on this cell
 */
export interface Cell { v: string; o?: string; n?: boolean; k?: string; e?: string; w?: string }

export interface Row {
  row: number; key: string; title: string; icon?: string; unit?: string; service?: boolean; new?: boolean;
  /** wide view: same order as columns; null — no value */
  cells?: (Cell | null)[];
  /** long view */
  field?: Column; cell?: Cell;
}

export interface RowPage {
  total: number; offset: number; long: boolean; rows: Row[];
  /** Only with the first page (offset 0) */
  columns?: Column[]; columns_matched?: boolean; kinds?: string[]; rarities?: { id: string; label: string }[]; has_all_fields?: boolean;
}

export interface CellRef { table: string; row: number; path: string }
export interface CellEdit extends CellRef { new: string }

export interface Change extends CellRef {
  table_title: string; key: string; title: string; icon?: string; unit?: string; field: Column; old: string; new: string;
  orig?: string; orig_absent?: boolean;
  /** Which row, beyond the name: the level of a class */
  context?: { label: string; value: string }[];
  issue?: { severity: "error" | "warning"; code: string; message: string };
}

/** One value with everything needed to edit it */
export interface CellField extends Column { table: string; row: number; row_unit: string; cell: Cell | null; currency?: string; item?: string }
/** All values of a roll; count null — can't be listed: no step, or `uneven`, the range doesn't divide by the step. With `more` only the first and last five are listed. */
export interface Roll { count: number | null; values?: string[]; more?: boolean; uneven?: boolean }

export interface LootGroupRow {
  key: string; title: string; kind: string; sources: number; mine: boolean; mode: string | null;
  items: number; items_was: number; gold: number; gold_was: number;
}

export interface LootSummary { items: number; gold: number; cats: { id: string; label: string; value: number }[] }

export interface LootItem {
  row: number; id: number; title: string; icon: string | null; rarity: string; rarity_label: string; cat: string; cat_label: string;
  w: CellField; cnt: CellField[];
  /** 0…1. share and chance include the rules and my edits; null — the rules removed it */
  template_share: number; share: number | null; chance: number | null; avg: number | null; chance_was: number | null;
}

export interface LootGroup {
  key: string; group: string; title: string; kind: string; sources: string[]; maps: string[]; types: number[];
  mode: { id: string; title: string } | null;
  rules: { scope: string; remove: string[]; chance: { label: string; value: number }[]; weight: { label: string; value: number }[]; count: { label: string; value: number }[]; quality: number | null }[];
  summary: LootSummary; summary_was: LootSummary;
  slots: {
    n: number; row: number; chance: CellField; rate: number | null; rate_was: number | null; template: number;
    pools: { key: string; label: string; share: number | null; pr: CellField | null; shared: number; shared_titles: string[]; items: LootItem[] }[];
  }[];
}

export interface ItemCard {
  id: string; title: string; icon?: string | null; kind: string; rarity: string; rarity_label: string;
  base: { attr: string; label: string; fixed?: CellField; min?: CellField; max?: CellField; roll?: Roll }[];
  durability?: CellField;
  /** Each part only if its table is in the kit */
  sell?: CellField;
  market?: { product: string; fields: CellField[] }[];
  trade?: { market: string; fields: CellField[] }[];
  dismantle?: CellField[];
  craft?: { recipe: string; fields: CellField[] }[];
  affix?: {
    group: string; shared: number; rarity: string; rarities: { id: string; label: string }[];
    rows: { row: number; title: string; unit: string; attr: string; major: AffixSide | null; minor: AffixSide | null }[];
  };
}
export interface AffixSide { min: CellField; max: CellField; roll: Roll }

export interface BulkSpec {
  table: string; query: RowQuery; paths: string[]; pairs: boolean; op: "mul" | "add" | "set"; value: number;
  /** Selected rows; without it, all rows of the filter */
  rows?: number[];
}

export interface RebaseItem {
  id: number; table: string; path: string; key: string; title: string; old: string; new: string;
  table_title?: string; field?: Column; row?: number; unit?: string; context?: { label: string; value: string }[];
}
export interface RebasePlan {
  from: string; to: string; comment: string;
  auto: RebaseItem[]; already: RebaseItem[];
  conflicts: (RebaseItem & { server: string; partial: boolean })[];
  lost: (RebaseItem & { reason: "table_gone" | "field_gone" | "row_gone" })[];
}

export interface Commands {
  app_state(): AppState;
  set_language(a: { lang: "ru" | "en" }): void;
  /** Opens the system dialog; null if cancelled */
  pick_kit(a: { kind: "zip" | "folder" }): string | null;
  open_kit(a: { path: string }): KitOverview;
  open_recent(a: { version: string }): KitOverview;
  close_kit(): void;
  query_rows(a: { table: string; query: RowQuery; offset: number; limit: number }): RowPage;
  /** One undo step. Empty new, or the kit's value, removes the change. Locked cells go to rejected. */
  set_values(a: { edits: CellEdit[] }): EditStatus & { changed: number; rejected: (CellRef & { code: string })[] };
  revert(a: { cells: CellRef[] }): EditStatus;
  undo(): EditStatus;
  redo(): EditStatus;
  list_changes(): EditStatus & { changes: Change[] };
  set_note(a: { author: string; comment: string }): void;
  /** System dialogs; null if cancelled */
  pick_patch_open(): string | null;
  pick_patch_save(a: { author: string; stamp: string }): string | null;
  import_patch(a: { path: string }): EditStatus & { applied: number; rejected: { at: number; code: string; message: string }[]; patch_warnings: { code: string; message: string }[]; author: string };
  list_items(a: { query: { search: string; kind: string; rarity: string; only_mine: boolean; lang: string } }): {
    items: { id: string; title: string; icon: string | null; kind: string; rarity: string; mine: boolean }[]; kinds: string[]; rarities: { id: string; label: string }[];
  };
  /** map: null — all maps. The groups come with the average drop from one source, now and in the kit */
  loot_groups(a: { query: { map: number | null; kind: string; search: string; only_mine: boolean; lang: string } }): { groups: LootGroupRow[]; kinds: { id: string; label: string; count: number }[]; maps: { id: number; label: string }[] };
  loot_group(a: { key: string }): LootGroup;
  /** rarity: which rarity to show in the affixes; "" — the item's own */
  get_item(a: { id: string; rarity: string }): ItemCard;
  /** path "" — the first attribute that changes with the level */
  class_curves(a: { path: string }): {
    attrs: (Column & { varies: boolean })[]; path: string | null;
    classes: { id: string; title: string; levels: { level: number; ref: CellField }[] }[];
  };
  bulk_fields(a: { table: string; query: RowQuery; rows: number[] | null }): { rows: number; fields: (Column & { count: number })[] };
  /** Changes nothing; errors and warnings count only the values this edit touches */
  preview_bulk(a: { spec: BulkSpec }): { total: number; skipped: number; empty: number; errors: number; warnings: number; changes: { row: number; key: string; title: string; unit: string; field: Column; old: string; new: string; issue?: { severity: string; code: string } }[] };
  /** One undo step */
  apply_bulk(a: { spec: BulkSpec }): EditStatus & { changed: number };
  rebase_preview(): RebasePlan;
  /** decisions: id of a conflict -> "mine" | "server" */
  rebase_apply(a: { decisions: Record<string, "mine" | "server"> }): EditStatus & { applied: number; lost: RebaseItem[] };
  export_patch(a: { path: string; author: string; comment: string; created: string }): { path: string; count: number };
}
// Icons: core.convertFileSrc(`${version}/${row.icon}`, "kit").
// Error codes: no_kit, bad_request, kit_not_found, kit_damaged, kit_unsafe, kit_too_large, kit_invalid, kit_io, patch_empty, patch_has_errors, patch_no_author, patch_invalid, patch_io, draft_io, rebase_none.
