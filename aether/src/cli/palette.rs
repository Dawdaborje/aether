//! `aether --theme-palette "#6750a4"`: a starting set of light and dark colour tokens for a
//! theme, made from one seed colour. The output is the `light` and `dark` objects of a
//! `theme.json`, ready to paste and then adjust by hand.
//!
//! Colours are made in OKLCH (lightness, chroma, hue), so a given lightness looks equally light
//! at every hue; the seed gives the hue and, within limits, the chroma.

use serde_json::{Map, Value, json};

/// How colourful the surfaces are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    /// Surfaces tinted with a little of the seed's hue (the default).
    Tonal,
    /// Grey surfaces; only accents carry colour.
    Neutral,
    /// Surfaces with a stronger tint.
    Vivid,
    /// No colour except the primary.
    Mono,
}

impl Scheme {
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "tonal" => Ok(Self::Tonal),
            "neutral" => Ok(Self::Neutral),
            "vivid" => Ok(Self::Vivid),
            "mono" => Ok(Self::Mono),
            other => Err(format!("unknown scheme `{other}`; schemes are tonal, neutral, vivid, mono")),
        }
    }

    fn surface_chroma(self) -> f64 {
        match self {
            Self::Tonal => 1.0,
            Self::Neutral => 0.3,
            Self::Vivid => 2.2,
            Self::Mono => 0.0,
        }
    }
}

fn parse_hex(seed: &str) -> Result<[f64; 3], String> {
    let hex = seed.trim().trim_start_matches('#');
    let full = match hex.len() {
        3 => hex.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 => hex.to_string(),
        _ => return Err(format!("`{seed}` is not a colour like #6750a4")),
    };
    let byte = |i: usize| {
        u8::from_str_radix(&full[i..i + 2], 16).map_err(|_| format!("`{seed}` is not a colour like #6750a4"))
    };
    Ok([f64::from(byte(0)?) / 255.0, f64::from(byte(2)?) / 255.0, f64::from(byte(4)?) / 255.0])
}

fn linear(c: f64) -> f64 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn encode(c: f64) -> f64 {
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

/// sRGB (0..1) → OKLCH.
fn to_oklch([r, g, b]: [f64; 3]) -> (f64, f64, f64) {
    let (r, g, b) = (linear(r), linear(g), linear(b));
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    let lightness = 0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s;
    let a = 1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s;
    let b = 0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s;
    (lightness, a.hypot(b), b.atan2(a))
}

fn from_oklch(lightness: f64, chroma: f64, hue: f64) -> [f64; 3] {
    let mut chroma = chroma;
    loop {
        let (a, b) = (chroma * hue.cos(), chroma * hue.sin());
        let l = (lightness + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
        let m = (lightness - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
        let s = (lightness - 0.089_484_177_5 * a - 1.291_485_548 * b).powi(3);
        let rgb = [
            4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s,
            -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s,
            -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701 * s,
        ];
        // Stay inside what a screen can show by taking colour out, not by clipping.
        if rgb.iter().all(|c| (-0.000_5..=1.000_5).contains(c)) || chroma < 0.001 {
            return rgb.map(|c| encode(c.clamp(0.0, 1.0)));
        }
        chroma *= 0.92;
    }
}

fn hex(rgb: [f64; 3]) -> String {
    let [r, g, b] = rgb.map(|c| (c * 255.0).round().clamp(0.0, 255.0) as u8);
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn luminance([r, g, b]: [f64; 3]) -> f64 {
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

/// WCAG contrast ratio of two `#rrggbb` colours.
pub fn contrast(a: &str, b: &str) -> f64 {
    let (la, lb) = match (parse_hex(a), parse_hex(b)) {
        (Ok(a), Ok(b)) => (luminance(a), luminance(b)),
        _ => return 0.0,
    };
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// The token maps for a seed: `{ "light": {…}, "dark": {…} }`.
pub fn palette(seed: &str, scheme: Scheme) -> Result<Value, String> {
    let (_, seed_chroma, hue) = to_oklch(parse_hex(seed)?);
    let accent_chroma = seed_chroma.clamp(0.08, 0.2);
    let k = scheme.surface_chroma();
    let at = |lightness: f64, chroma: f64, hue: f64| hex(from_oklch(lightness, chroma, hue));
    // A tinted surface: `c` is the chroma at the default tint.
    let tint = |lightness: f64, c: f64| at(lightness, c * k, hue);

    let build = |dark: bool| {
        let (bg, card, muted, line, field_line, fg, soft_fg) = if dark {
            (0.16, 0.2, 0.25, 0.33, 0.55, 0.94, 0.75)
        } else {
            (0.985, 0.965, 0.945, 0.86, 0.6, 0.2, 0.45)
        };
        let primary = if dark { at(0.8, accent_chroma, hue) } else { at(0.48, accent_chroma, hue) };
        let on_primary = if dark { tint(0.22, 0.03) } else { "#ffffff".to_string() };
        let secondary = if dark { tint(0.3, 0.04) } else { tint(0.91, 0.04) };
        let on_secondary = if dark { tint(0.92, 0.02) } else { tint(0.25, 0.03) };
        let foreground = tint(fg, 0.012);
        let mut tokens = Map::new();
        let mut put = |name: &str, value: String| {
            tokens.insert(name.to_string(), json!(value));
        };
        put("background", tint(bg, 0.01));
        put("foreground", foreground.clone());
        put("canvas", tint(bg, 0.01));
        put("card", tint(card, 0.012));
        put("card-foreground", foreground.clone());
        put("popover", tint(card, 0.012));
        put("popover-foreground", foreground.clone());
        put("primary", primary.clone());
        put("primary-foreground", on_primary.clone());
        put("secondary", secondary.clone());
        put("secondary-foreground", on_secondary.clone());
        put("muted", tint(muted, 0.015));
        put("muted-foreground", tint(soft_fg, 0.02));
        put("accent", secondary.clone());
        put("accent-foreground", on_secondary.clone());
        put("destructive", if dark { "#f2b8b5".into() } else { "#b3261e".into() });
        put("success", if dark { "#9ad77d".into() } else { "#386a20".into() });
        put("success-foreground", if dark { "#0b2000".into() } else { "#ffffff".into() });
        put("warning", "#f9bd4d".into());
        put("warning-foreground", "#2a1800".into());
        put("info", primary.clone());
        put("info-foreground", on_primary.clone());
        put("border", tint(line, 0.02));
        put("input", tint(field_line, 0.02));
        put("ring", primary.clone());
        put("highlight", "#f9bd4d".into());
        put("highlight-foreground", "#2a1800".into());
        put("sidebar", tint(bg, 0.01));
        put("sidebar-foreground", tint(soft_fg, 0.02));
        put("sidebar-primary", primary.clone());
        put("sidebar-primary-foreground", on_primary);
        put("sidebar-accent", secondary);
        put("sidebar-accent-foreground", on_secondary);
        put("sidebar-border", tint(line, 0.02));
        put("sidebar-ring", primary);
        // Five chart colours spread around the colour wheel, equally light.
        for (index, turn) in [0.0, 60.0, 140.0, 200.0, 280.0].into_iter().enumerate() {
            let chart_hue = hue + f64::to_radians(turn);
            tokens.insert(
                format!("chart-{}", index + 1),
                json!(at(if dark { 0.72 } else { 0.55 }, 0.13, chart_hue)),
            );
        }
        Value::Object(tokens)
    };
    Ok(json!({ "light": build(false), "dark": build(true) }))
}

/// Print the palette and, on stderr, any pair that is hard to read.
pub fn run(seed: &str, scheme: &str) -> Result<(), String> {
    let scheme = Scheme::parse(scheme)?;
    let value = palette(seed, scheme)?;
    println!("{}", serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?);
    for mode in ["light", "dark"] {
        for (text, ground) in [
            ("foreground", "background"),
            ("primary-foreground", "primary"),
            ("muted-foreground", "background"),
        ] {
            let tokens = &value[mode];
            let ratio = contrast(tokens[text].as_str().unwrap_or(""), tokens[ground].as_str().unwrap_or(""));
            if ratio < 4.5 {
                eprintln!("note: {mode} `{text}` on `{ground}` has contrast {ratio:.1}:1 (4.5:1 is the usual minimum)");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_round_trip_through_oklch() {
        for seed in ["#6750a4", "#0f6cbd", "#ffffff", "#000000", "#00703c"] {
            let rgb = parse_hex(seed).unwrap();
            let (l, c, h) = to_oklch(rgb);
            assert_eq!(hex(from_oklch(l, c, h)), seed);
        }
    }

    #[test]
    fn a_palette_has_every_token_and_readable_text() {
        let value = palette("#6750a4", Scheme::Tonal).unwrap();
        for mode in ["light", "dark"] {
            let tokens = value[mode].as_object().unwrap();
            assert!(tokens.len() >= 39, "{mode}");
            for (name, colour) in tokens {
                assert!(parse_hex(colour.as_str().unwrap()).is_ok(), "{mode} {name}");
            }
            assert!(contrast(tokens["foreground"].as_str().unwrap(), tokens["background"].as_str().unwrap()) >= 7.0);
            assert!(contrast(tokens["primary-foreground"].as_str().unwrap(), tokens["primary"].as_str().unwrap()) >= 4.5);
        }
    }

    #[test]
    fn schemes_change_the_surfaces_and_bad_input_is_refused() {
        let mono = palette("#6750a4", Scheme::Mono).unwrap();
        let background = mono["light"]["background"].as_str().unwrap();
        assert_eq!(background[1..3], background[3..5]);
        assert!(palette("purple", Scheme::Tonal).is_err());
        assert!(Scheme::parse("loud").is_err());
    }
}
