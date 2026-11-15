//! Number formats and line joining (spec 5.1).
//!
//! Every stat uses one class of the format table; golden tests are written from that table.

/// Joins stats on one line: two spaces, a pipe, two spaces.
pub const SEP: &str = "  |  ";

/// Round half away from zero to `places` decimals.
fn round_places(v: f64, places: i32) -> f64 {
    let f = 10f64.powi(places);
    let r = (v.abs() * f + 1e-9).round() / f;
    if v < 0.0 { -r } else { r }
}

/// Thousands separators for the integer part of an already-formatted number.
fn group(int_part: &str) -> String {
    let (sign, digits) = match int_part.strip_prefix('-') {
        Some(d) => ("-", d),
        None => ("", int_part),
    };
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    format!("{sign}{out}")
}

fn fixed(v: f64, places: usize) -> String {
    let r = round_places(v, places as i32);
    let s = format!("{r:.places$}");
    // `-0` and `-0.0` read badly.
    let s = if s
        .trim_start_matches('-')
        .chars()
        .all(|c| c == '0' || c == '.')
    {
        s.trim_start_matches('-').to_string()
    } else {
        s
    };
    match s.split_once('.') {
        Some((i, f)) => format!("{}.{f}", group(i)),
        None => group(&s),
    }
}

/// `int`: round half away from zero; thousands separators. `1,193` `25`.
pub fn int(v: f64) -> String {
    fixed(v, 0)
}

/// `d1`: at most one decimal, a trailing `.0` dropped; thousands separators.
/// `225.4` `60` `16.5` `1,700`.
pub fn d1(v: f64) -> String {
    let s = fixed(v, 1);
    s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
}

/// `d2`: always two decimals. `0.75` `1.10` `-0.20`.
pub fn d2(v: f64) -> String {
    fixed(v, 2)
}

/// `sig2`: two significant digits, at most four decimals; for values below 0.1. `0.0054`.
pub fn sig2(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let mag = v.abs().log10().floor() as i32;
    let places = (1 - mag).clamp(0, 4) as usize;
    let s = fixed(v, places);
    if s.contains('.') {
        let t = s.trim_end_matches('0').trim_end_matches('.');
        t.to_string()
    } else {
        s
    }
}

/// `pct`: whole percent, no space before `%`. `25%`.
pub fn pct(fraction_times_100: f64) -> String {
    format!("{}%", int(fraction_times_100))
}

/// `pct` with an explicit sign for modifiers: `+35%`, `-40%`.
pub fn pct_signed(v: f64) -> String {
    let s = int(v);
    if s.starts_with('-') || s == "0" {
        format!("{s}%")
    } else {
        format!("+{s}%")
    }
}

/// A distance: `d1` km at 1,000 m and above, else `int` m.
pub fn distance(m: f64) -> String {
    if m.abs() >= 1000.0 {
        format!("{} km", d1(m / 1000.0))
    } else {
        format!("{} m", int(m))
    }
}

/// A range of distances, `1.8–10 km` or `50 m–10 km`.
pub fn distance_range(a: f64, b: f64) -> String {
    if a >= 1000.0 && b >= 1000.0 {
        format!("{}–{} km", d1(a / 1000.0), d1(b / 1000.0))
    } else {
        format!("{}–{}", distance(a), distance(b))
    }
}

/// Some(v) when present and non-zero ("left out" rule of the format table).
pub fn nz(v: Option<f64>) -> Option<f64> {
    v.filter(|x| *x != 0.0 && x.is_finite())
}

/// Join the present parts with [`SEP`]; `None` when nothing is left.
pub fn line(parts: impl IntoIterator<Item = Option<String>>) -> Option<String> {
    let v: Vec<String> = parts.into_iter().flatten().collect();
    (!v.is_empty()).then(|| v.join(SEP))
}

/// `2× S3, 1× S2`: counts by size, largest size first.
pub fn by_size(items: impl IntoIterator<Item = (Option<f64>, f64)>) -> Option<String> {
    let mut sizes: Vec<(i64, f64)> = Vec::new();
    for (size, count) in items {
        let s = size.map(|s| s.round() as i64).unwrap_or(-1);
        match sizes.iter_mut().find(|(k, _)| *k == s) {
            Some((_, c)) => *c += count,
            None => sizes.push((s, count)),
        }
    }
    sizes.retain(|(_, c)| *c > 0.0);
    if sizes.is_empty() {
        return None;
    }
    sizes.sort_by_key(|s| std::cmp::Reverse(s.0));
    Some(
        sizes
            .iter()
            .map(|(s, c)| {
                if *s >= 0 {
                    format!("{}× S{s}", int(*c))
                } else {
                    format!("{}×", int(*c))
                }
            })
            .collect::<Vec<_>>()
            .join(", "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The examples column of the spec 5.1 format table.
    #[test]
    fn format_table_examples() {
        assert_eq!(int(1193.0), "1,193");
        assert_eq!(int(25.0), "25");
        assert_eq!(int(2.4), "2");
        assert_eq!(int(2.5), "3");
        assert_eq!(int(-2.5), "-3");
        assert_eq!(int(6_936_600.0), "6,936,600");
        assert_eq!(d1(225.4), "225.4");
        assert_eq!(d1(60.0), "60");
        assert_eq!(d1(8.0), "8");
        assert_eq!(d1(16.5), "16.5");
        assert_eq!(d1(1700.0), "1,700");
        assert_eq!(d1(1537.5), "1,537.5");
        assert_eq!(d1(375.66), "375.7");
        assert_eq!(d1(0.04), "0");
        assert_eq!(d2(0.75), "0.75");
        assert_eq!(d2(1.1), "1.10");
        assert_eq!(d2(-0.2), "-0.20");
        assert_eq!(d2(-0.001), "0.00");
        assert_eq!(sig2(0.0054), "0.0054");
        assert_eq!(sig2(0.00588), "0.0059");
        assert_eq!(sig2(0.012), "0.012");
        assert_eq!(sig2(0.000012), "0");
        assert_eq!(pct(25.0), "25%");
        assert_eq!(pct_signed(35.0), "+35%");
        assert_eq!(pct_signed(-40.0), "-40%");
    }

    #[test]
    fn distances() {
        assert_eq!(distance(4003.0), "4 km");
        assert_eq!(distance(1100.0), "1.1 km");
        assert_eq!(distance(999.6), "1,000 m");
        assert_eq!(distance(550.0), "550 m");
        assert_eq!(distance_range(1800.0, 10_000.0), "1.8–10 km");
        assert_eq!(distance_range(50.0, 10_000.0), "50 m–10 km");
    }

    #[test]
    fn lines_and_sizes() {
        assert_eq!(
            line([Some("A: 1".into()), None, Some("B: 2".into())]).as_deref(),
            Some("A: 1  |  B: 2")
        );
        assert_eq!(line([None, None]), None);
        assert_eq!(
            by_size([(Some(2.0), 1.0), (Some(3.0), 2.0)]).as_deref(),
            Some("2× S3, 1× S2")
        );
        assert_eq!(
            by_size([(Some(1.0), 1.0), (Some(1.0), 1.0)]).as_deref(),
            Some("2× S1")
        );
        assert_eq!(by_size(std::iter::empty()), None);
        assert_eq!(by_size([(Some(0.0), 1.0)]).as_deref(), Some("1× S0"));
        assert_eq!(by_size([(None, 2.0)]).as_deref(), Some("2×"));
        assert_eq!(nz(Some(0.0)), None);
        assert_eq!(nz(Some(f64::NAN)), None);
    }
}
