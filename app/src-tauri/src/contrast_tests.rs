//! Spec 9.6: every theme x palette x accent keeps 4.5:1 for text and 3:1 for UI parts.
//! Reads the tokens straight from `app/src/app.css`, so a token edit that breaks AA fails here.

use std::collections::HashMap;

type Rgb = [f64; 3];

fn css() -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/app.css");
    let raw = std::fs::read_to_string(p).unwrap();
    // Drop /* comments */ so they never merge into a declaration or a selector.
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw.as_str();
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        rest = rest[i..].find("*/").map_or("", |j| &rest[i + j + 2..]);
    }
    out.push_str(rest);
    out
}

/// `selector -> {var: #hex}` for the plain `#rrggbb` declarations of each rule.
fn blocks(css: &str) -> Vec<(String, HashMap<String, String>)> {
    let mut out = Vec::new();
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let sel = rest[..open]
            .rsplit(['}', ';'])
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        let body = &rest[open + 1..open + close];
        let mut vars = HashMap::new();
        for decl in body.split(';') {
            if let Some((k, v)) = decl.split_once(':') {
                let (k, v) = (k.trim(), v.trim());
                if k.starts_with("--") && v.starts_with('#') && v.len() == 7 {
                    vars.insert(k.to_string(), v.to_string());
                }
            }
        }
        out.push((sel, vars));
        rest = &rest[open + close + 1..];
    }
    out
}

fn rgb(hex: &str) -> Rgb {
    let n = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap() as f64 / 255.0;
    [n(1), n(3), n(5)]
}

fn lum(c: Rgb) -> f64 {
    let l = |v: f64| {
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * l(c[0]) + 0.7152 * l(c[1]) + 0.0722 * l(c[2])
}

fn ratio(a: &str, b: &str) -> f64 {
    let (x, y) = (lum(rgb(a)), lum(rgb(b)));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// The resolved tokens for one combination, applying rules in source order like the cascade.
fn resolve(
    all: &[(String, HashMap<String, String>)],
    theme: &str,
    palette: &str,
    accent: &str,
) -> HashMap<String, String> {
    let applies = |sel: &str| -> bool {
        if !sel.starts_with(":root") || sel.contains(',') {
            return false;
        }
        let want = |attr: &str, val: &str| {
            let k = format!("[{attr}=");
            match sel.find(&k) {
                None => true,
                Some(_) => sel.contains(&format!("[{attr}='{val}']")),
            }
        };
        want("data-theme", theme)
            && want("data-palette", palette)
            && want("data-accent", accent)
            && !sel.contains("data-perf")
            && !sel.contains(":not")
    };
    let mut t = HashMap::new();
    for (sel, vars) in all {
        if applies(sel) {
            t.extend(vars.clone());
        }
    }
    t
}

#[test]
fn every_theme_and_accent_meets_wcag_aa() {
    let all = blocks(&css());
    let mut failures = Vec::new();
    let combos = [("dark", "void"), ("dark", "black"), ("light", "void")];
    let accents = ["cyan", "green", "amber", "orange", "rose", "violet"];
    for (theme, palette) in combos {
        for accent in accents {
            let t = resolve(&all, theme, palette, accent);
            let g = |k: &str| {
                t.get(k)
                    .unwrap_or_else(|| panic!("{theme}/{palette}/{accent}: no {k}"))
                    .clone()
            };
            // (foreground, background, minimum, what)
            let mut pairs = vec![
                (
                    "--primary-foreground",
                    "--primary",
                    4.5,
                    "text on primary buttons",
                ),
                (
                    "--primary",
                    "--background",
                    3.0,
                    "primary UI parts on the page",
                ),
                ("--primary", "--card", 3.0, "primary UI parts on cards"),
            ];
            for bg in ["--background", "--card", "--popover", "--muted"] {
                pairs.push(("--foreground", bg, 4.5, "body text"));
                pairs.push(("--muted-foreground", bg, 4.5, "secondary text"));
            }
            for fg in [
                "--success",
                "--warning",
                "--info",
                "--destructive",
                "--accent-2",
            ] {
                pairs.push((fg, "--card", 4.5, "status text on cards"));
                pairs.push((fg, "--background", 4.5, "status text on the page"));
            }
            for (fg, bg, min, what) in pairs {
                let r = ratio(&g(fg), &g(bg));
                if r < min {
                    failures.push(format!(
                        "{theme}/{palette}/{accent}: {fg} on {bg} = {r:.2} < {min} ({what})"
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "contrast failures:\n{}",
        failures.join("\n")
    );
}
