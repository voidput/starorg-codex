#!/usr/bin/env node
// Answers a list of queries with sc-gamedata's own query code (its compiled dist/query/), so
// `gamedata-port query` (the Rust port in crates/starorg-gamedata/src/query) can be checked
// against it on the same datasets. Reads the dist and the data directory only.
//
//   node scripts/query-golden.mjs --dist ~/git/sc-gamedata/dist --data ~/git/sc-gamedata/data \
//     --queries queries.json > golden.json
//   node scripts/query-golden.mjs --compare ours.json golden.json
//
// Query file: [{ "op": "find", "q": "..." } | { "op": "where" | "obtain", "q": "...", "at"?: "..." }
// | { "op": "shop", "place": "..." }]. `--sample N` instead writes a query file: fixed cases
// plus every Nth item of game-item-tags.json.
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

const args = process.argv.slice(2);
const flag = (name) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : undefined;
};

if (flag("compare") !== undefined) {
  const [a, b] = [flag("compare"), args[args.indexOf("--compare") + 2]];
  const ours = JSON.parse(fs.readFileSync(a, "utf8"));
  const golden = JSON.parse(fs.readFileSync(b, "utf8"));
  let bad = 0;
  const same = (x, y, at) => {
    if (typeof x === "number" && typeof y === "number") {
      if (x === y || Math.abs(x - y) <= 1e-12 * Math.max(Math.abs(x), Math.abs(y))) return true;
      return report(at, x, y);
    }
    if (Array.isArray(x) || Array.isArray(y)) {
      if (!Array.isArray(x) || !Array.isArray(y) || x.length !== y.length) return report(at, len(x), len(y));
      return x.every((v, i) => same(v, y[i], `${at}[${i}]`));
    }
    if (x && y && typeof x === "object" && typeof y === "object") {
      const keys = [...new Set([...Object.keys(x), ...Object.keys(y)])];
      return keys.every((k) => same(x[k], y[k], `${at}.${k}`));
    }
    return x === y || report(at, x, y);
  };
  const len = (v) => (Array.isArray(v) ? `array(${v.length})` : JSON.stringify(v)?.slice(0, 80));
  function report(at, x, y) {
    if (bad++ < 20) console.log(`  ${at}: ours ${JSON.stringify(x)?.slice(0, 160)} golden ${JSON.stringify(y)?.slice(0, 160)}`);
    return false;
  }
  let differing = 0;
  ours.forEach((o, i) => {
    const before = bad;
    same(o.answer, golden[i]?.answer, `${i} ${o.query.op} ${JSON.stringify(o.query.q ?? o.query.place)}`);
    if (bad > before) differing++;
  });
  console.log(`${ours.length - differing} of ${ours.length} answers identical`);
  process.exit(differing === 0 && ours.length === golden.length ? 0 : 1);
}

const dist = path.resolve(flag("dist") ?? path.join(process.env.HOME, "git/sc-gamedata/dist"));
const data = path.resolve(flag("data") ?? path.join(process.env.HOME, "git/sc-gamedata/data"));
const load = (file) => {
  try {
    return JSON.parse(fs.readFileSync(path.join(data, file), "utf8"));
  } catch {
    return undefined;
  }
};

if (flag("sample") !== undefined) {
  const every = Number(flag("sample"));
  const items = load("game-item-tags.json").items;
  const fixed = ["verdict", "cambino srt", "gladius", "fs9", "p4-ar", "arrowhead", "gallent", "devestator", "medpen", "multi-tool", "carinite", "quantainium", "pembroke", "tsk-1", "behring"];
  const places = ["grim hex", "lorville", "pyro", "area 18", "stanton", "ruin station", "nb", "orison", "levski"];
  const q = [];
  for (const f of fixed) q.push({ op: "find", q: f }, { op: "where", q: f }, { op: "obtain", q: f });
  q.push({ op: "where", q: "fs-9", at: "grim hex" }, { op: "obtain", q: "p4-ar", at: "lorville" }, { op: "where", q: "medpen", at: "pyro" });
  for (const p of places) q.push({ op: "shop", place: p });
  // Contract awards and Wikelo trades, so the reward rows are covered too.
  for (const a of (load("game-item-sources.json")?.contractAwards ?? []).filter((_, i) => i % 8 === 0)) q.push({ op: "where", q: a.item });
  for (const t of (load("game-wikelo-trades.json")?.trades ?? []).filter((t) => t.rewardInferred).slice(0, 12)) q.push({ op: "where", q: t.rewardInferred });
  items.forEach((it, i) => {
    if (i % every === 0) q.push({ op: "where", q: it.className }, { op: "find", q: it.name });
  });
  console.log(JSON.stringify(q));
  process.exit(0);
}

const mod = (f) => import(pathToFileURL(path.join(dist, "query", f)).href);
const { buildEntries, rankEntries } = await mod("fuzzy.js");
const { loadSearchIndexEntries } = await mod("search-index.js");
const { buildLootIndex, findItems, obtain } = await mod("obtain.js");
const { whereSummary } = await mod("where.js");
const { shopInventory, limitShop } = await mod("shops.js");

const tags = load("game-item-tags.json");
const loot = load("game-loot.json");
const sources = load("game-item-sources.json");
const blueprints = load("game-blueprints.json");
const wikelo = load("game-wikelo-trades.json");
const shops = load("uex-item-prices.json");
const terminals = load("uex-terminals.json");
const index = load("game-search-index.json");
const datasets = {
  items: tags?.items ?? [],
  loot,
  areas: sources?.areas,
  contractAwards: sources?.contractAwards,
  blueprints: blueprints?.blueprints,
  wikelo: wikelo?.trades,
  shops: shops?.prices,
  terminals: terminals?.terminals,
  shopsAsOf: shops?._fetched,
};
const lootIndex = loot ? buildLootIndex(loot) : undefined;
const entries = index ? loadSearchIndexEntries(index) : [];
const shopsLoaded = shops !== undefined;

// sc-gamedata's `where`/`obtain` item resolution (cli.ts).
function resolve(q) {
  let found = findItems(datasets.items, q);
  if (found.length === 0) {
    const ranked = rankEntries(buildEntries(datasets.items, []), q).filter((e) => e.kind === "item");
    const best = ranked.length ? datasets.items.find((i) => i.className === ranked[0].key) : undefined;
    found = best ? datasets.items.filter((i) => i.name === best.name) : [];
  }
  return found;
}
// Full blueprint and trade records are not part of the port's typed answer; compare by name.
const slim = (r) => ({ ...r, blueprints: r.blueprints.map((b) => b.name), wikeloInferred: r.wikeloInferred.map((t) => t.name) });

const queries = JSON.parse(fs.readFileSync(flag("queries"), "utf8"));
const out = queries.map((query) => {
  let answer;
  if (query.op === "find") {
    answer = rankEntries(entries, query.q).slice(0, 25).map((e) => [e.kind, e.key, e.name]);
  } else if (query.op === "shop") {
    const report = shopInventory({ shops: shops?.prices ?? [], terminals: terminals?.terminals, items: tags?.items, asOf: shops?._fetched }, query.place);
    answer = limitShop(report);
  } else {
    const found = resolve(query.q);
    const reports = found.slice(0, 3).map((item) => obtain(datasets, item, lootIndex, { at: query.at }));
    answer = {
      matches: found.map((m) => m.className),
      [query.op === "where" ? "summaries" : "reports"]: reports.map((r) => (query.op === "where" ? whereSummary(r, { shopsLoaded }) : slim(r))),
    };
  }
  return { query, answer };
});
console.log(JSON.stringify(out));
