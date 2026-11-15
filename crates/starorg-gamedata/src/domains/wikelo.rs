//! Wikelo Emporium trades (port of `sc-gamedata/src/domains/wikelo.ts`, written as
//! `game-wikelo-trades`).
//!
//! Wikelo is internally "TheCollector", which is why no record path contains "wikelo". A
//! trade is a `ContractTemplate` whose objective handler carries `HaulingOrder_EntityClass`
//! nodes: deliver this many of that entity.
//!
//! What you receive is not stored on the contract (only a `rewardContribution`
//! percentage), so the reward is read from the record name (`TheCollector_Vehicles_Polaris`
//! gives up a Polaris) and published as `rewardInferred`.

use crate::collate::locale_compare;
use crate::domain::{Domain, DomainError, ParseContext, field};
use crate::helpers_gameplay::{add_count, by_count_desc, file_ref_stem};
use crate::output::js_key_order;
use crate::records::{GameRecord, num};
use serde::Serialize;
use serde_json::{Map, Value};

pub static DOMAIN: Wikelo = Wikelo;

pub struct Wikelo;

/// One requirement. Field order is the TS object literal's (`amount, maxAmount, item`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandIn {
    /// Amount required.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_amount: Option<f64>,
    pub item: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikeloTrade {
    pub name: String,
    pub record_name: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Read from the record name, not from a reward field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reward_inferred: Option<String>,
    pub hand_ins: Vec<HandIn>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Summary {
    trades: usize,
    collector_records_without_hand_ins: usize,
    distinct_items_requested: usize,
}

/// Every hauling order nested anywhere in a value, in JS enumeration order.
pub fn collect_hauling_orders(value: &Value, found: &mut Vec<HandIn>) {
    match value {
        Value::Array(entries) => {
            for entry in entries {
                collect_hauling_orders(entry, found);
            }
        }
        Value::Object(obj) => {
            if obj.get("_Type_").and_then(Value::as_str) == Some("HaulingOrder_EntityClass")
                && let Some(item) = file_ref_stem(obj.get("entityClass")).filter(|i| !i.is_empty())
            {
                    let min = num(obj.get("minAmount"));
                    let max = num(obj.get("maxAmount"));
                    found.push(HandIn {
                        // maxAmount is sometimes 0 where minAmount is the real requirement.
                        // `min && min > 0 ? min : max`
                        amount: match min {
                            Some(m) if m > 0.0 => Some(m),
                            _ => max,
                        },
                        // `max && max !== min ? max : undefined`
                        max_amount: max.filter(|x| *x != 0.0 && Some(*x) != min),
                        item: item.to_string(),
                    });
            }
            for key in js_key_order(obj) {
                collect_hauling_orders(&obj[key], found);
            }
        }
        _ => {}
    }
}

/// `splitRecordName`: "TheCollector_Vehicles_Polaris" -> (Some("Vehicles"), Some("Polaris")).
/// Both `None` for names that are not collector records.
pub fn split_record_name(short_name: &str) -> (Option<String>, Option<String>) {
    const KNOWN_CATEGORIES: [&str; 5] = ["vehicles", "forfavours", "smallitems", "ships", "intro"];
    let parts: Vec<&str> = short_name.split('_').collect();
    if parts.len() < 2 || parts[0].to_lowercase() != "thecollector" {
        return (None, None);
    }
    let rest = &parts[1..];
    let first = rest[0].to_lowercase();
    if KNOWN_CATEGORIES.contains(&first.as_str()) && rest.len() > 1 {
        return (Some(rest[0].to_string()), Some(rest[1..].join(" ")));
    }
    let category = (rest.len() != 1).then(|| rest[0].to_string());
    (category, Some(rest.join(" ")))
}

pub fn parse_wikelo_trade(record: &GameRecord) -> Option<WikeloTrade> {
    let mut hand_ins = Vec::new();
    collect_hauling_orders(&record.value, &mut hand_ins);
    if hand_ins.is_empty() {
        return None;
    }
    hand_ins.sort_by(|a, b| locale_compare(&a.item, &b.item));
    let (category, reward) = split_record_name(&record.short_name);
    Some(WikeloTrade {
        name: record.short_name.clone(),
        record_name: record.name.clone(),
        id: record.id.clone(),
        category,
        reward_inferred: reward,
        hand_ins,
    })
}

impl Domain for Wikelo {
    fn name(&self) -> &'static str {
        "game-wikelo-trades"
    }

    fn description(&self) -> &'static str {
        "Wikelo Emporium (TheCollector) barter trades and what each one asks for."
    }

    fn filters(&self) -> &'static [&'static str] {
        &["**/contracts/contracttemplates/**"]
    }

    fn parse(&self, ctx: &ParseContext<'_>) -> Result<Map<String, Value>, DomainError> {
        let collector: Vec<&GameRecord> = ctx
            .records
            .of_type("ContractTemplate")
            .filter(|r| {
                format!("{} {}", r.name, r.record_path)
                    .to_lowercase()
                    .contains("thecollector")
            })
            .collect();

        let mut trades: Vec<WikeloTrade> = collector.iter().filter_map(|r| parse_wikelo_trade(r)).collect();
        trades.sort_by(|a, b| locale_compare(&a.name, &b.name));

        // What Wikelo asks for most, across every trade, is the useful roll-up.
        let mut demand = Map::new();
        for trade in &trades {
            for hand_in in &trade.hand_ins {
                add_count(&mut demand, &hand_in.item, hand_in.amount.unwrap_or(0.0));
            }
        }

        let mut body = Map::new();
        field(&mut body, "trades", &trades)?;
        body.insert("totalDemand".into(), Value::Object(by_count_desc(&demand)));
        field(
            &mut body,
            "summary",
            &Summary {
                trades: trades.len(),
                // Templates that carry no hauling orders are flow or intro scaffolding.
                collector_records_without_hand_ins: collector.len() - trades.len(),
                distinct_items_requested: demand.len(),
            },
        )?;
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::NoFiles;
    use crate::localization::Localization;
    use crate::records::{RecordSet, normalize_record};
    use serde_json::json;

    fn template(short_name: &str, orders: Vec<Value>) -> GameRecord {
        normalize_record(
            json!({"_RecordName_": format!("ContractTemplate.{short_name}"), "_RecordId_": format!("id-{short_name}"),
                   "_RecordValue_": {"_Type_": "ContractTemplate", "objectiveTokens": [{"objectiveHandler": {"haulingOrders": orders}}]}}),
            &format!(
                "libs/foundry/records/contracts/contracttemplates/{}",
                short_name.to_lowercase()
            ),
        )
        .unwrap()
    }

    fn order(path: &str, min: f64, max: f64) -> Value {
        json!({"_Type_": "HaulingOrder_EntityClass",
               "entityClass": format!("file://./../libs/foundry/records/entities/scitem/carryables/1h/{path}.json"),
               "minAmount": min, "maxAmount": max})
    }

    fn collect(value: &Value) -> Vec<HandIn> {
        let mut found = Vec::new();
        collect_hauling_orders(value, &mut found);
        found
    }

    // Cases from sc-gamedata src/domains/wikelo.test.ts.
    #[test]
    fn finds_orders_nested_under_the_objective_handler() {
        let t = template("X", vec![order("harvestable_mineral_1h_carinite", 10.0, 10.0)]);
        assert_eq!(
            collect(&t.value),
            vec![HandIn {
                amount: Some(10.0),
                max_amount: None,
                item: "harvestable_mineral_1h_carinite".into()
            }]
        );
    }

    #[test]
    fn amount_fallbacks_and_ranges() {
        // Falls back to maxAmount when minAmount is zero.
        assert_eq!(collect(&order("thing", 0.0, 7.0))[0].amount, Some(7.0));
        // Uses minAmount when maxAmount is zero, as some records store it.
        let h = &collect(&order("volt_rifle_energy_01", 1.0, 0.0))[0];
        assert_eq!((h.amount, h.max_amount), (Some(1.0), None));
        // Records a genuine range when the two differ.
        let h = &collect(&order("thing", 5.0, 9.0))[0];
        assert_eq!((h.amount, h.max_amount), (Some(5.0), Some(9.0)));
    }

    #[test]
    fn returns_nothing_when_there_are_no_hauling_orders() {
        assert!(collect(&json!({"objectiveTokens": [{"other": true}]})).is_empty());
    }

    #[test]
    fn splits_record_names() {
        let s = |n: &str| split_record_name(n);
        assert_eq!(
            s("TheCollector_Vehicles_Polaris"),
            (Some("Vehicles".into()), Some("Polaris".into()))
        );
        assert_eq!(s("TheCollector_Vehicles_Starlancer_Tac").1.as_deref(), Some("Starlancer Tac"));
        assert_eq!(s("SomethingElse_Foo"), (None, None));
        assert_eq!(s("TheCollector_Polaris"), (None, Some("Polaris".into())));
        assert_eq!(s("TheCollector_Odd_Thing"), (Some("Odd".into()), Some("Odd Thing".into())));
    }

    fn records() -> Vec<GameRecord> {
        vec![
            template(
                "TheCollector_Vehicles_Polaris",
                vec![
                    order("carryable_1h_cy_banu_favour_wikelo", 50.0, 50.0),
                    order("harvestable_ore_1h_jacliumore", 20.0, 20.0),
                ],
            ),
            template(
                "TheCollector_ForFavours_Caranite",
                vec![order("carryable_1h_cy_banu_favour_wikelo", 10.0, 10.0)],
            ),
            // Flow scaffolding with no hand-ins should not become a trade.
            template("TheCollector_Flow", vec![]),
        ]
    }

    fn parse() -> Map<String, Value> {
        let set = RecordSet::new(records());
        DOMAIN
            .parse(&ParseContext::new(&set, &Localization::new(), &NoFiles))
            .unwrap()
    }

    #[test]
    fn keeps_only_templates_that_actually_ask_for_something() {
        let body = parse();
        assert_eq!(body["summary"]["trades"], 2);
        assert_eq!(body["summary"]["collectorRecordsWithoutHandIns"], 1);
        assert_eq!(body["summary"]["distinctItemsRequested"], 2);
    }

    #[test]
    fn totals_demand_across_every_trade() {
        let body = parse();
        assert_eq!(body["totalDemand"]["carryable_1h_cy_banu_favour_wikelo"], json!(60.0));
        let keys: Vec<_> = body["totalDemand"].as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys[0], "carryable_1h_cy_banu_favour_wikelo");
    }

    #[test]
    fn marks_the_reward_as_inferred() {
        let trade = parse_wikelo_trade(&records()[0]).unwrap();
        assert_eq!(trade.reward_inferred.as_deref(), Some("Polaris"));
        let row = serde_json::to_value(&trade).unwrap();
        let keys: Vec<&str> = row.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["name", "recordName", "id", "category", "rewardInferred", "handIns"]);
        assert_eq!(
            row["handIns"][0],
            json!({"amount": 50.0, "item": "carryable_1h_cy_banu_favour_wikelo"})
        );
    }
}
