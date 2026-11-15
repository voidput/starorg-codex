//! Enhancement generators (spec 5, 6): `GameData` + stock strings + profile -> edits.
//!
//! Generators are pure. Each produces, per record, a payload (a stat block, a name tag or
//! mission text); [`Engine`] decides which record owns each loc key (the namesake rule of
//! spec 5.1), checks the result against the per-value rules of spec 7.3, and counts every
//! outcome in the coverage report (spec 13).
//!
//! Edits stay in parts (`prefix` + stock + `suffix`) until render time, so validation knows
//! exactly which text we added and nothing ever has to be stripped from a previous run.

pub mod check;
pub mod format;
pub mod items;
pub mod missions;
pub mod owned;
pub mod profile;
pub mod ships;
pub mod tags;

use crate::ini::{Edits, IniError, StockIni, fold_key, strip_flag_suffix};
use crate::model::namesake::{Ownership, named_after, resolve};
use crate::model::{GameData, WeaponClass, is_placeholder_name, loc_key};
use crate::source::Warning;
use owned::OwnedBlueprints;
use profile::Profile;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Which generator an edit comes from; also the coverage categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Ships,
    Components,
    ShipWeapons,
    FpsWeapons,
    Missiles,
    ComponentTags,
    ShipWeaponTags,
    MissileTags,
    MissionTitles,
    MissionDescriptions,
    Branding,
    /// Sort prefixes and renames on ship name keys (addendum §6.5). Opt-in, no preset.
    ShipNames,
}

impl Category {
    pub const ALL: [Category; 12] = [
        Self::Ships,
        Self::Components,
        Self::ShipWeapons,
        Self::FpsWeapons,
        Self::Missiles,
        Self::ComponentTags,
        Self::ShipWeaponTags,
        Self::MissileTags,
        Self::MissionTitles,
        Self::MissionDescriptions,
        Self::Branding,
        Self::ShipNames,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Self::Ships => "ships",
            Self::Components => "components",
            Self::ShipWeapons => "ship-weapons",
            Self::FpsWeapons => "fps-weapons",
            Self::Missiles => "missiles",
            Self::ComponentTags => "component-tags",
            Self::ShipWeaponTags => "ship-weapon-tags",
            Self::MissileTags => "missile-tags",
            Self::MissionTitles => "mission-titles",
            Self::MissionDescriptions => "mission-descriptions",
            Self::Branding => "branding",
            Self::ShipNames => "ship-names",
        }
    }
}

/// One generated edit, in parts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GenEdit {
    /// Stock key as spelled in stock (including any flag suffix).
    pub key: String,
    #[serde(skip)]
    pub line: usize,
    pub category: Category,
    /// Record short name (or contract group) whose data produced the text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record: Option<String>,
    pub prefix: String,
    pub suffix: String,
    /// The stock part loses its trailing `\n` escapes and spaces (spec 5.1, 6.2).
    pub trim_stock: bool,
    /// Whole-value part in place of the stock part (a ship rename, addendum §6.5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replace: Option<String>,
}

/// Drop trailing `\n` escapes, spaces and tabs.
pub fn trim_tail(s: &str) -> &str {
    let mut t = s;
    loop {
        let before = t.len();
        t = t.trim_end_matches([' ', '\t']);
        if let Some(x) = t.strip_suffix("\\n") {
            t = x;
        }
        if t.len() == before {
            return t;
        }
    }
}

impl GenEdit {
    pub fn stock_part<'a>(&'a self, stock_value: &'a str) -> &'a str {
        if let Some(r) = &self.replace {
            r
        } else if self.trim_stock {
            trim_tail(stock_value)
        } else {
            stock_value
        }
    }

    pub fn value(&self, stock_value: &str) -> String {
        format!(
            "{}{}{}",
            self.prefix,
            self.stock_part(stock_value),
            self.suffix
        )
    }

    /// The text we added (for loc patches' `from_part = "generated"`); a rename counts.
    pub fn generated_part(&self) -> String {
        format!(
            "{}{}{}",
            self.prefix,
            self.replace.as_deref().unwrap_or_default(),
            self.suffix
        )
    }
}

/// What a record would write into a key; equal payloads count as "the same text"
/// (namesake rule step 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    /// Stat lines, placed after (or above) the stock prose.
    Block(Vec<String>),
    /// A name tag, placed per the tag style.
    Tag { prefix: String, suffix: String },
    /// Text appended after the trimmed stock value.
    Append(String),
    /// A sort prefix and an optional whole-value rename (ship names).
    Name {
        prefix: String,
        replace: Option<String>,
    },
}

/// Why a candidate key was left as stock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    ContendersDiffer,
    Placeholder,
    NotInStock,
    /// Two stock keys fold to the same key (spec 4.3).
    Collision,
    FormatKey,
    DroppedByValidation,
    /// Another generator already owns the key.
    OwnedByOther,
}

impl Reason {
    pub fn name(&self) -> &'static str {
        match self {
            Self::ContendersDiffer => "contenders-differ",
            Self::Placeholder => "placeholder",
            Self::NotInStock => "not-in-stock",
            Self::Collision => "collision",
            Self::FormatKey => "format-key",
            Self::DroppedByValidation => "dropped-by-validation",
            Self::OwnedByOther => "owned-by-other",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CategoryCoverage {
    /// Keys some record had text for.
    pub candidates: usize,
    pub written: usize,
    /// Left as stock, by reason.
    #[serde(default)]
    pub left: BTreeMap<Reason, usize>,
    /// Written keys whose stock value ends inside an open EM tag.
    #[serde(default)]
    pub open_tail: usize,
    /// Written keys whose reference matched stock only after case folding.
    #[serde(default)]
    pub case_folded: usize,
}

impl CategoryCoverage {
    pub fn left_for(&self, r: Reason) -> usize {
        self.left.get(&r).copied().unwrap_or(0)
    }

    pub fn written_share(&self) -> f64 {
        if self.candidates == 0 {
            1.0
        } else {
            self.written as f64 / self.candidates as f64
        }
    }
}

/// A key left as stock because its records would write different text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contest {
    pub category: Category,
    pub key: String,
    pub records: Vec<String>,
}

/// Coverage report of one build (spec 13). Counts only; no prose.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Coverage {
    pub schema: u32,
    #[serde(default)]
    pub categories: BTreeMap<Category, CategoryCoverage>,
    /// Up to [`MAX_CONTESTS`] diagnostics per category (not part of the baseline).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contested: Vec<Contest>,
}

pub const COVERAGE_SCHEMA: u32 = 1;
pub const MAX_CONTESTS: usize = 20;
/// M2 acceptance (spec 14): written share at least this, per category.
pub const MIN_WRITTEN: f64 = 0.90;
/// M2 acceptance: "contenders-differ" at most this share of candidates.
pub const MAX_CONTESTED: f64 = 0.05;
/// CI gate (spec 13): a category's written count may fall at most this far below baseline.
pub const BASELINE_SLACK: f64 = 0.02;

impl Coverage {
    pub fn get(&self, c: Category) -> CategoryCoverage {
        self.categories.get(&c).cloned().unwrap_or_default()
    }

    /// Categories missing the M2 thresholds, as messages.
    pub fn threshold_failures(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (c, v) in &self.categories {
            if v.candidates == 0 {
                continue;
            }
            if v.written_share() < MIN_WRITTEN {
                out.push(format!(
                    "{}: {:.1}% written ({} of {}), need {:.0}%",
                    c.name(),
                    v.written_share() * 100.0,
                    v.written,
                    v.candidates,
                    MIN_WRITTEN * 100.0
                ));
            }
            let contested = v.left_for(Reason::ContendersDiffer) as f64 / v.candidates as f64;
            if contested > MAX_CONTESTED {
                out.push(format!(
                    "{}: contenders-differ {:.1}% of candidates, at most {:.0}%",
                    c.name(),
                    contested * 100.0,
                    MAX_CONTESTED * 100.0
                ));
            }
        }
        out
    }

    /// Categories whose written count fell more than 2% below `baseline` (spec 13).
    pub fn baseline_failures(&self, baseline: &Coverage) -> Vec<String> {
        baseline
            .categories
            .iter()
            .filter_map(|(c, b)| {
                let now = self.get(*c).written;
                let floor = (b.written as f64 * (1.0 - BASELINE_SLACK)).floor() as usize;
                (now < floor).then(|| {
                    format!(
                        "{}: {} written, baseline {} (floor {})",
                        c.name(),
                        now,
                        b.written,
                        floor
                    )
                })
            })
            .collect()
    }

    /// The baseline form: counts only.
    pub fn baseline(&self) -> Coverage {
        Coverage {
            schema: self.schema,
            categories: self.categories.clone(),
            contested: Vec::new(),
        }
    }
}

/// Output of [`generate`].
#[derive(Debug, Clone, Default)]
pub struct Generated {
    /// By stock line.
    pub edits: BTreeMap<usize, GenEdit>,
    pub coverage: Coverage,
    pub warnings: Vec<Warning>,
}

impl Generated {
    /// Final values for the writer.
    pub fn to_edits(&self, stock: &StockIni) -> Edits {
        let mut e = Edits::new();
        for g in self.edits.values() {
            let sv = stock.value_at(g.line).unwrap_or_default();
            // Values were checked in `offer`; set() matches by key again.
            let _ = e.set(stock, &g.key, g.value(sv));
        }
        e
    }

    pub fn count(&self, c: Category) -> usize {
        self.edits.values().filter(|e| e.category == c).count()
    }
}

/// A record's claim on a key.
#[derive(Debug, Clone)]
pub struct Claim {
    pub record: String,
    /// Loc reference (with or without `@`).
    pub key: String,
    pub payload: Option<Payload>,
    /// The record's display name is a placeholder (template, test record).
    pub placeholder: bool,
    /// Player-hull rule (spec 5.2): false for AI/PU/unmanned variants.
    pub player: bool,
}

/// Owns the stock index, the profile and the result while generators run.
pub struct Engine<'a> {
    pub stock: &'a StockIni,
    pub profile: &'a Profile,
    pub out: Generated,
}

impl<'a> Engine<'a> {
    pub fn new(stock: &'a StockIni, profile: &'a Profile) -> Self {
        let mut out = Generated::default();
        out.coverage.schema = COVERAGE_SCHEMA;
        Self {
            stock,
            profile,
            out,
        }
    }

    fn cov(&mut self, c: Category) -> &mut CategoryCoverage {
        self.out.coverage.categories.entry(c).or_default()
    }

    fn left(&mut self, c: Category, r: Reason) {
        *self.cov(c).left.entry(r).or_default() += 1;
    }

    /// Turn a payload into parts for a given stock value.
    fn parts(&self, payload: &Payload, stock_value: &str) -> (String, String, bool) {
        match payload {
            Payload::Block(lines) => {
                let body = format!("{}\\n{}", self.profile.header, lines.join("\\n"));
                if trim_tail(stock_value).is_empty() {
                    (String::new(), body, true)
                } else if self.profile.stats_above {
                    (format!("{body}\\n\\n------\\n\\n"), String::new(), true)
                } else {
                    (String::new(), format!("\\n\\n{body}"), true)
                }
            }
            Payload::Tag { prefix, suffix } => (prefix.clone(), suffix.clone(), false),
            Payload::Append(s) => (String::new(), s.clone(), true),
            Payload::Name { prefix, .. } => (prefix.clone(), String::new(), false),
        }
    }

    /// Offer an edit for a candidate key. Counts the outcome; returns true when written.
    pub fn offer(
        &mut self,
        cat: Category,
        key: &str,
        record: Option<String>,
        payload: &Payload,
    ) -> bool {
        self.cov(cat).candidates += 1;
        let line = match self.stock.find(key) {
            Ok(l) => l,
            Err(IniError::AmbiguousKey { .. }) => {
                self.left(cat, Reason::Collision);
                return false;
            }
            Err(_) => {
                self.left(cat, Reason::NotInStock);
                return false;
            }
        };
        if self.out.edits.contains_key(&line) {
            self.left(cat, Reason::OwnedByOther);
            return false;
        }
        let stock_key = self.stock.key_at(line).unwrap_or_default().to_string();
        let sv = self.stock.value_at(line).unwrap_or_default();
        if check::is_format_value(sv) {
            self.left(cat, Reason::FormatKey);
            return false;
        }
        if matches!(payload, Payload::Tag { .. }) && is_placeholder_name(Some(sv.trim())) {
            self.left(cat, Reason::Placeholder);
            return false;
        }
        let (prefix, suffix, trim_stock) = self.parts(payload, sv);
        let replace = match payload {
            Payload::Name { replace, .. } => replace.clone(),
            _ => None,
        };
        let stock_part = match &replace {
            Some(r) => r.as_str(),
            None if trim_stock => trim_tail(sv),
            None => sv,
        };
        let checked = match &replace {
            Some(_) => check::check_replaced(sv, &format!("{prefix}{stock_part}{suffix}")),
            None => check::check_generated(sv, stock_part, &prefix, &suffix),
        };
        if let Err(rule) = checked {
            self.left(cat, Reason::DroppedByValidation);
            self.out.warnings.push(Warning::new(
                "dropped-edit",
                format!("{stock_key}: generated value dropped ({})", rule.name()),
            ));
            return false;
        }
        let len = prefix.len() + stock_part.len() + suffix.len();
        if len > check::WARN_LEN {
            self.out.warnings.push(Warning::new(
                "long-value",
                format!("{stock_key}: {len} bytes"),
            ));
        }
        let open_tail = check::tokens(sv).open_tail && !suffix.is_empty();
        let folded_only = strip_flag_suffix(&stock_key) != key.strip_prefix('@').unwrap_or(key);
        let c = self.cov(cat);
        c.written += 1;
        if open_tail {
            c.open_tail += 1;
        }
        if folded_only {
            c.case_folded += 1;
        }
        self.out.edits.insert(
            line,
            GenEdit {
                key: stock_key,
                line,
                category: cat,
                record,
                prefix,
                suffix,
                trim_stock,
                replace,
            },
        );
        true
    }

    /// Resolve ownership of every key the claims reference and offer the winners
    /// (namesake rule, spec 5.1).
    pub fn assign(&mut self, cat: Category, claims: Vec<Claim>) {
        let mut by_key: BTreeMap<String, (String, Vec<Claim>)> = BTreeMap::new();
        for c in claims {
            let Some(k) = loc_key(&c.key) else { continue };
            let k = k.to_string();
            let e = by_key
                .entry(fold_key(&k))
                .or_insert_with(|| (k.clone(), Vec::new()));
            if !e.1.iter().any(|x| x.record == c.record) {
                e.1.push(c);
            }
        }
        for (_, (key, mut all)) in by_key {
            all.sort_by(|a, b| a.record.cmp(&b.record));
            let live: Vec<&Claim> = all.iter().filter(|c| !c.placeholder).collect();
            if live.is_empty() {
                if all.iter().any(|c| c.payload.is_some()) {
                    self.cov(cat).candidates += 1;
                    self.left(cat, Reason::Placeholder);
                }
                continue;
            }
            let payload_of = |r: &str| {
                live.iter()
                    .find(|c| c.record == r)
                    .and_then(|c| c.payload.clone())
            };
            let own = if let Some(n) = live.iter().find(|c| named_after(&key, &c.record)) {
                Ownership::Namesake(n.record.clone())
            } else {
                let players: Vec<String> = live
                    .iter()
                    .filter(|c| c.player)
                    .map(|c| c.record.clone())
                    .collect();
                let pool = if players.is_empty() {
                    live.iter().map(|c| c.record.clone()).collect()
                } else {
                    players
                };
                match resolve(&key, &pool, payload_of) {
                    Some(o) => o,
                    None => continue,
                }
            };
            match &own {
                Ownership::Contested(records) => {
                    if records.iter().any(|r| payload_of(r).is_some()) {
                        self.cov(cat).candidates += 1;
                        self.left(cat, Reason::ContendersDiffer);
                        let shown = self
                            .out
                            .coverage
                            .contested
                            .iter()
                            .filter(|c| c.category == cat)
                            .count();
                        if shown < MAX_CONTESTS {
                            self.out.coverage.contested.push(Contest {
                                category: cat,
                                key: key.clone(),
                                records: records.clone(),
                            });
                        }
                    }
                }
                own => {
                    let w = own.writer().unwrap_or_default().to_string();
                    if let Some(p) = payload_of(&w) {
                        self.offer(cat, &key, Some(w), &p);
                    }
                }
            }
        }
    }
}

/// Stock display name of a loc reference, when it is real text.
pub fn stock_name<'s>(stock: &'s StockIni, reference: Option<&str>) -> Option<&'s str> {
    let v = stock.value(loc_key(reference?)?)?.trim();
    (!is_placeholder_name(Some(v))).then_some(v)
}

/// Run every enabled generator in the fixed order (spec 4.3): stat blocks (description
/// keys), name tags (name keys), missions, branding. No ship names (see [`generate_with`]).
pub fn generate(
    gd: &GameData,
    stock: &StockIni,
    profile: &Profile,
    owned: &dyn OwnedBlueprints,
) -> Generated {
    generate_with(gd, stock, profile, owned, None)
}

/// [`generate`], then the `ship-names` category when `ship_names` is given (the channel's
/// active settings, addendum §6.5). It runs last: any other generator that ever writes a
/// ship name key owns it first.
pub fn generate_with(
    gd: &GameData,
    stock: &StockIni,
    profile: &Profile,
    owned: &dyn OwnedBlueprints,
    ship_names: Option<&crate::fleet::store::ShipNames>,
) -> Generated {
    let mut en = Engine::new(stock, profile);
    let p = profile;
    let item_placeholder = |name: Option<&str>| stock_name(stock, name).is_none();

    if p.ships {
        let claims = gd
            .ships
            .iter()
            .filter(|s| !s.is_template)
            .filter_map(|s| {
                Some(Claim {
                    record: s.short_name().to_string(),
                    key: s.loc.desc.clone()?,
                    payload: ships::ship_lines(s, stock, p).map(Payload::Block),
                    placeholder: item_placeholder(s.loc.name.as_deref()),
                    player: ships::is_player_hull(s.short_name()),
                })
            })
            .collect();
        en.assign(Category::Ships, claims);
    }
    if p.components {
        let claims = gd
            .components
            .iter()
            .filter(|c| {
                !c.item.is_template && !matches!(c.category.as_str(), "weapons" | "missiles")
            })
            .filter_map(|c| {
                Some(Claim {
                    record: c.item.short_name().to_string(),
                    key: c.item.loc.desc.clone()?,
                    payload: items::component_lines(c, p).map(Payload::Block),
                    placeholder: item_placeholder(c.item.loc.name.as_deref()),
                    player: ships::is_main_item(c.item.short_name()),
                })
            })
            .collect();
        en.assign(Category::Components, claims);
    }
    for (class, cat, on) in [
        (WeaponClass::Ship, Category::ShipWeapons, p.ship_weapons),
        (WeaponClass::Fps, Category::FpsWeapons, p.fps_weapons),
    ] {
        if !on {
            continue;
        }
        let claims = gd
            .weapons
            .iter()
            .filter(|w| w.class == class && !w.item.is_template)
            .filter_map(|w| {
                Some(Claim {
                    record: w.item.short_name().to_string(),
                    key: w.item.loc.desc.clone()?,
                    payload: items::weapon_lines(w, p).map(Payload::Block),
                    placeholder: item_placeholder(w.item.loc.name.as_deref()),
                    player: ships::is_main_item(w.item.short_name()),
                })
            })
            .collect();
        en.assign(cat, claims);
    }
    if p.missiles {
        let claims = gd
            .missiles
            .iter()
            .filter(|m| !m.item.is_template)
            .filter_map(|m| {
                Some(Claim {
                    record: m.item.short_name().to_string(),
                    key: m.item.loc.desc.clone()?,
                    payload: items::missile_lines(m, p).map(Payload::Block),
                    placeholder: item_placeholder(m.item.loc.name.as_deref()),
                    player: ships::is_main_item(m.item.short_name()),
                })
            })
            .collect();
        en.assign(Category::Missiles, claims);
    }

    // Name tags: name keys, and short-name keys when the option is on.
    let tag_payload = |style: &profile::TagStyle, elements: Option<Vec<tags::Element>>| {
        let t = tags::build(style, &elements?)?;
        let (prefix, suffix) = tags::place(style, &t);
        Some(Payload::Tag { prefix, suffix })
    };
    let mut fields: Vec<fn(&crate::model::LocKeys) -> Option<&String>> = vec![|l| l.name.as_ref()];
    if p.tags.short_names {
        fields.push(|l| l.short_name.as_ref());
    }
    for field in &fields {
        if p.tags.components.enabled {
            let claims = gd
                .components
                .iter()
                .filter(|c| !c.item.is_template)
                .filter_map(|c| {
                    let class = stock_name(stock, c.item.loc.desc.as_deref())
                        .and_then(tags::class_from_description);
                    Some(Claim {
                        record: c.item.short_name().to_string(),
                        key: field(&c.item.loc)?.clone(),
                        payload: tag_payload(&p.tags.components, items::component_tag(c, class)),
                        placeholder: item_placeholder(c.item.loc.name.as_deref()),
                        player: ships::is_main_item(c.item.short_name()),
                    })
                })
                .collect();
            en.assign(Category::ComponentTags, claims);
        }
        if p.tags.ship_weapons.enabled {
            let claims = gd
                .weapons
                .iter()
                .filter(|w| !w.item.is_template && w.class == WeaponClass::Ship)
                .filter_map(|w| {
                    Some(Claim {
                        record: w.item.short_name().to_string(),
                        key: field(&w.item.loc)?.clone(),
                        payload: tag_payload(&p.tags.ship_weapons, items::weapon_tag(w, p)),
                        placeholder: item_placeholder(w.item.loc.name.as_deref()),
                        player: ships::is_main_item(w.item.short_name()),
                    })
                })
                .collect();
            en.assign(Category::ShipWeaponTags, claims);
        }
        if p.tags.missiles.enabled {
            let claims = gd
                .missiles
                .iter()
                .filter(|m| !m.item.is_template)
                .filter_map(|m| {
                    Some(Claim {
                        record: m.item.short_name().to_string(),
                        key: field(&m.item.loc)?.clone(),
                        payload: tag_payload(&p.tags.missiles, Some(items::missile_tag(m))),
                        placeholder: item_placeholder(m.item.loc.name.as_deref()),
                        player: ships::is_main_item(m.item.short_name()),
                    })
                })
                .collect();
            en.assign(Category::MissileTags, claims);
        }
    }

    if p.missions {
        missions::generate(&mut en, gd, owned);
    }

    if p.branding {
        let text = format!(
            "\\nText enhanced with StarOrg Strings v{}",
            env!("CARGO_PKG_VERSION")
        );
        en.offer(
            Category::Branding,
            "Frontend_PU_Version",
            None,
            &Payload::Append(text),
        );
    }

    if let Some(s) = ship_names {
        let cat = crate::fleet::Catalog::build(gd, stock);
        let plan = crate::fleet::names::plan(s, &cat);
        en.cov(Category::ShipNames);
        for e in plan.edits {
            en.offer(
                Category::ShipNames,
                &e.key,
                Some(e.hull),
                &Payload::Name {
                    prefix: e.prefix,
                    replace: e.replace,
                },
            );
        }
    }
    en.out
}

/// Render the generated edits and run the whole-file checks.
pub fn render(stock: &StockIni, g: &Generated) -> Result<Vec<u8>, check::FileError> {
    let bytes = stock.render(&g.to_edits(stock));
    check::check_file(stock, &bytes)?;
    Ok(bytes)
}
