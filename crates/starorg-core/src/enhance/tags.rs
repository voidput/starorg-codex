//! Name tags (spec 5.1, 5.3, 5.4, 5.6): built from DataCore fields, never from prose, with
//! the one stated exception of the component class.

use super::profile::TagStyle;

/// One tag element's value, rendered in the element's style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Element {
    /// `Military`, `Civilian`, `Industrial`, `Stealth`, `Competition`.
    Class(String),
    Size(i64),
    /// 1 to 4, shown as A to D.
    Grade(i64),
    /// A damage type of `model::items::DAMAGE_TYPES`.
    Damage(String),
    /// `Infrared`, `Electromagnetic`, `CrossSection`.
    Tracking(String),
    Bomb,
}

impl Element {
    fn name(&self) -> &'static str {
        match self {
            Self::Class(_) => "class",
            Self::Size(_) => "size",
            Self::Grade(_) => "grade",
            Self::Damage(_) => "damage",
            Self::Tracking(_) | Self::Bomb => "tracking",
        }
    }

    fn render(&self, style: &str) -> Option<String> {
        let long = style == "long";
        let short = style == "short";
        Some(match self {
            Self::Class(c) => {
                let (s, m) = match c.as_str() {
                    "Military" => ("M", "MIL"),
                    "Civilian" => ("C", "CIV"),
                    "Industrial" => ("I", "IND"),
                    "Stealth" => ("S", "STH"),
                    "Competition" => ("R", "CMP"),
                    _ => return None,
                };
                if long {
                    c.clone()
                } else if short {
                    s.into()
                } else {
                    m.into()
                }
            }
            Self::Size(n) => match style {
                "short" => n.to_string(),
                "long" => format!("Size {n}"),
                _ => format!("S{n}"),
            },
            Self::Grade(g) => {
                let letter = grade_letter(*g)?;
                if long {
                    format!("Grade {letter}")
                } else {
                    letter.to_string()
                }
            }
            Self::Damage(d) => {
                let (s, m, l) = match d.as_str() {
                    "physical" => ("P", "Phys", "Physical"),
                    "energy" => ("E", "Energy", "Energy"),
                    "distortion" => ("D", "Dist", "Distortion"),
                    "thermal" => ("T", "Therm", "Thermal"),
                    "biochemical" => ("B", "Bio", "Biochemical"),
                    "stun" => ("S", "Stun", "Stun"),
                    _ => return None,
                };
                match style {
                    "short" => s,
                    "long" => l,
                    _ => m,
                }
                .into()
            }
            Self::Tracking(t) => {
                let (s, l) = match t.as_str() {
                    "Infrared" => ("IR", "Infrared"),
                    "Electromagnetic" => ("EM", "Electromagnetic"),
                    "CrossSection" => ("CS", "Cross-section"),
                    _ => return None,
                };
                if long { l } else { s }.into()
            }
            Self::Bomb => if long { "Bomb" } else { "B" }.into(),
        })
    }
}

pub fn grade_letter(g: i64) -> Option<char> {
    match g {
        1 => Some('A'),
        2 => Some('B'),
        3 => Some('C'),
        4 => Some('D'),
        _ => None,
    }
}

/// Build the tag text (`[MIL-S1-B]`) from the elements the record has, in the order the
/// style lists. Elements with no value are dropped; with none left there is no tag.
pub fn build(style: &TagStyle, have: &[Element]) -> Option<String> {
    if !style.enabled {
        return None;
    }
    let parts: Vec<String> = style
        .elements
        .iter()
        .filter_map(|name| {
            have.iter()
                .find(|e| e.name() == name)
                .and_then(|e| e.render(style.style_of(name)))
        })
        .collect();
    if parts.is_empty() {
        return None;
    }
    let body = parts.join(&style.separator);
    let mut enc = style.enclosing.chars();
    Some(match (enc.next(), enc.next()) {
        (Some(a), Some(b)) => format!("{a}{body}{b}"),
        _ => body,
    })
}

/// Place a tag on a stock name: `<tag> <name>` or `<name> <tag>`. Returns (prefix, suffix).
pub fn place(style: &TagStyle, tag: &str) -> (String, String) {
    if style.placement == "suffix" {
        (String::new(), format!(" {tag}"))
    } else {
        (format!("{tag} "), String::new())
    }
}

/// Component class from the owning record's own stock description: a line starting
/// `Class:` (Q7: DataCore has no class field). The only value read out of prose.
pub fn class_from_description(desc: &str) -> Option<String> {
    for line in desc.split("\\n") {
        if let Some(rest) = line.trim_start().strip_prefix("Class:") {
            let c = rest.trim();
            return [
                "Military",
                "Civilian",
                "Industrial",
                "Stealth",
                "Competition",
            ]
            .iter()
            .find(|k| k.eq_ignore_ascii_case(c))
            .map(|k| k.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enhance::profile::Tags;

    #[test]
    fn default_tags() {
        let t = Tags::default();
        let c = [
            Element::Class("Military".into()),
            Element::Size(1),
            Element::Grade(2),
        ];
        assert_eq!(build(&t.components, &c).as_deref(), Some("[MIL-S1-B]"));
        // No class found: element dropped.
        assert_eq!(build(&t.components, &c[1..]).as_deref(), Some("[S1-B]"));
        let w = [Element::Damage("energy".into()), Element::Size(2)];
        assert_eq!(build(&t.ship_weapons, &w).as_deref(), Some("[E-S2]"));
        assert_eq!(build(&t.ship_weapons, &w[1..]).as_deref(), Some("[S2]"));
        let m = [Element::Tracking("Infrared".into()), Element::Size(2)];
        assert_eq!(build(&t.missiles, &m).as_deref(), Some("[IR-S2]"));
        assert_eq!(
            build(&t.missiles, &[Element::Bomb, Element::Size(3)]).as_deref(),
            Some("[B-S3]")
        );
        assert_eq!(build(&t.missiles, &[]), None);
    }

    #[test]
    fn builder_settings() {
        let mut s = Tags::default().components;
        s.separator = String::new();
        s.enclosing = "()".into();
        s.elements = vec!["size".into(), "class".into()];
        s.styles.insert("class".into(), "long".into());
        let c = [Element::Class("Stealth".into()), Element::Size(3)];
        assert_eq!(build(&s, &c).as_deref(), Some("(S3Stealth)"));
        s.enclosing = String::new();
        s.separator = " ".into();
        assert_eq!(build(&s, &c).as_deref(), Some("S3 Stealth"));
        s.placement = "suffix".into();
        assert_eq!(place(&s, "T"), (String::new(), " T".into()));
        s.enabled = false;
        assert_eq!(build(&s, &c), None);
    }

    #[test]
    fn class_line() {
        assert_eq!(
            class_from_description("Item Type: X\\nSize: 1\\nClass: Industrial\\n\\nProse")
                .as_deref(),
            Some("Industrial")
        );
        assert_eq!(
            class_from_description("Prose with Class: Military inline"),
            None
        );
        assert_eq!(class_from_description("Class: Unknown"), None);
    }
}
