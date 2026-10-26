//! design.lint — one JSON line in, one JSON line out.
//! Checks an HTML page (and optional CSS) against the landing-page-design rules:
//! type scale, spacing table, flat backgrounds, all-round borders, one typeface,
//! no italics, alt text, focus ring, reduced motion, ship requirements.
use serde_json::{json, Value};
use std::io::BufRead;

const TYPE_SCALE: [f64; 13] = [12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 30.0, 36.0, 48.0, 60.0, 72.0, 96.0, 128.0];
const SPACE_SCALE: [f64; 13] = [0.0, 2.0, 4.0, 8.0, 12.0, 16.0, 24.0, 32.0, 40.0, 48.0, 64.0, 80.0, 96.0];
const SPACE_PROPS: [&str; 14] = [
    "padding", "padding-top", "padding-right", "padding-bottom", "padding-left",
    "margin", "margin-top", "margin-right", "margin-bottom", "margin-left",
    "gap", "row-gap", "column-gap", "row-gap",
];
const BANNED_FONTS: [&str; 5] = ["inter", "roboto", "arial", "open sans", "helvetica"];

fn main() {
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).unwrap();
    let v: Value = serde_json::from_str(&line).unwrap_or_default();
    let html = v["html"].as_str().unwrap_or("");
    let css = v["css"].as_str().unwrap_or("");
    println!("{}", lint(html, css));
}

struct Hit {
    rule: &'static str,
    severity: &'static str,
    evidence: String,
    fix: &'static str,
}

fn in_scale(x: f64, scale: &[f64]) -> bool {
    scale.iter().any(|s| (s - x).abs() < 0.51)
}

fn px_of(raw: &str) -> Option<f64> {
    let t = raw.trim().to_ascii_lowercase();
    if t.contains("var(") || t.contains("calc(") || t.contains("clamp(") || t.contains("env(") {
        return None;
    }
    let t = t.split_whitespace().next().unwrap_or("");
    let num: String = t.chars().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect();
    let n: f64 = num.parse().ok()?;
    if t.contains("rem") || t.contains("em") {
        Some(n * 16.0)
    } else if t.contains('%') {
        None
    } else {
        Some(n)
    }
}

/// (value, offset) for every `prop:` declaration in `doc`.
fn decls(doc: &str, prop: &str) -> Vec<(String, usize)> {
    let low = doc.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(p) = low[from..].find(prop) {
        let i = from + p;
        let after = i + prop.len();
        let boundary_ok = low[after..].starts_with(':')
            && (i == 0
                || matches!(low.as_bytes()[i - 1], b';' | b'{' | b'}' | b'\n' | b' ' | b'\t' | b')'));
        if boundary_ok {
            let rest = &doc[after + 1..];
            let end = rest.find(|c| c == ';' || c == '}' || c == '\n' || c == '<' || c == '"')
                .unwrap_or(rest.len());
            out.push((rest[..end].trim().to_string(), i));
        }
        from = after;
    }
    out
}

fn count(hay: &str, needle: &str) -> usize {
    hay.to_ascii_lowercase().matches(needle).count()
}

fn lint(html: &str, css: &str) -> Value {
    if html.trim().is_empty() {
        return json!({"score": 0, "violations": [], "checked": 0,
                      "note": "empty html: send the page source in {html}, and its css in {css} if it lives in a separate file"});
    }
    let doc = format!("{}\n{}", html, css);
    let low = doc.to_ascii_lowercase();
    let text_low = html.to_ascii_lowercase();
    let mut hits: Vec<Hit> = Vec::new();
    let mut muted: Vec<&'static str> = Vec::new();

    // B1 type scale
    let mut seen_type = 0;
    let mut size_decls = decls(&doc, "font-size");
    for (v, i) in decls(&doc, "font") {
        // shorthand: font: 600 17px/1.6 family
        if let Some(slash) = v.find('/') {
            let head = &v[..slash];
            let size = head.split_whitespace().last().unwrap_or("").to_string();
            if size.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                size_decls.push((size, i));
            }
        }
    }
    for (v, _) in size_decls {
        if let Some(px) = px_of(&v) {
            seen_type += 1;
            if !in_scale(px, &TYPE_SCALE) {
                hits.push(Hit { rule: "type-scale", severity: "warning",
                    evidence: format!("font-size: {}", v.chars().take(40).collect::<String>()),
                    fix: "snap to the nearest Tailwind step below (12/14/16/18/20/24/30/36/48/60/72/96/128)" });
            }
        }
    }
    if seen_type == 0 { muted.push("type-scale"); }

    // B2 spacing
    let mut seen_space = 0;
    for prop in SPACE_PROPS {
        for (v, _) in decls(&doc, prop) {
            for token in v.split_whitespace() {
                if let Some(px) = px_of(token) {
                    seen_space += 1;
                    if !in_scale(px, &SPACE_SCALE) {
                        hits.push(Hit { rule: "spacing", severity: "warning",
                            evidence: format!("{}: {}", prop, v.chars().take(40).collect::<String>()),
                            fix: "use 0/2/4/8/12/16/24/32/40/48/64/80/96 only" });
                    }
                }
            }
        }
    }
    if seen_space == 0 { muted.push("spacing"); }

    // B4 backgrounds flat
    for pat in ["linear-gradient(", "radial-gradient(", "conic-gradient("] {
        let mut from = 0;
        while let Some(p) = low[from..].find(pat) {
            let i = from + p;
            let start = low[..i].rfind(|c| c == ';' || c == '{' || c == '}').map(|x| x + 1).unwrap_or(0);
            let head = &low[start..i];
            let block_end = low[i..].find('}').map(|x| i + x).unwrap_or(low.len());
            let block = &low[i..block_end.min(i + 400)];
            from = i + pat.len();
            if !head.contains("background") && !head.contains("bg") {
                continue;
            }
            if block.contains("background-clip") || block.contains("-webkit-background-clip") {
                continue; // the one allowed gradient: heading text
            }
            hits.push(Hit { rule: "flat-background", severity: "error",
                evidence: format!("{}…{}", head.trim().chars().take(30).collect::<String>(), pat),
                fix: "backgrounds are flat; the only gradient allowed is the hero heading text" });
        }
    }

    // B4 all-round borders
    for prop in ["border-top", "border-bottom", "border-left", "border-right"] {
        let side_stripe = prop == "border-left" || prop == "border-right";
        for (v, _) in decls(&doc, prop) {
            let t = v.trim().to_ascii_lowercase();
            if t == "0" || t == "0px" || t == "none" { continue; } // resets are fine
            hits.push(Hit { rule: "all-round-border",
                severity: if side_stripe { "error" } else { "warning" },
                evidence: format!("{}: {}", prop, v.chars().take(30).collect::<String>()),
                fix: "a border goes all the way around or not at all (top/bottom alone is a divider, left/right alone is a stripe)" });
        }
    }

    // B1 no italics
    if text_low.contains("<em") || text_low.contains("<i>") || text_low.contains("<i ") {
        hits.push(Hit { rule: "no-italics", severity: "warning",
            evidence: "<em>/<i> in markup".into(), fix: "never italicise; use weight or colour" });
    }
    for (v, _) in decls(&doc, "font-style") {
        if v.to_ascii_lowercase().contains("italic") || v.to_ascii_lowercase().contains("oblique") {
            hits.push(Hit { rule: "no-italics", severity: "warning",
                evidence: format!("font-style: {}", v.chars().take(20).collect::<String>()),
                fix: "never italicise" });
        }
    }

    // B1 one typeface, allowed families only
    let mut families: Vec<String> = Vec::new();
    for (v, _) in decls(&doc, "font-family") {
        for part in v.split(',') {
            let fam = part.trim().trim_matches(|c| c == '"' || c == '\'').trim().to_ascii_lowercase();
            if fam.is_empty() || fam.starts_with("var(") { continue; }
            if !families.contains(&fam) { families.push(fam); }
        }
    }
    let generic = ["sans-serif", "serif", "monospace", "system-ui", "ui-sans-serif", "ui-monospace", "cursive", "fantasy", "inherit", "initial", "unset"];
    let primaries: Vec<&String> = families.iter()
        .filter(|f| !generic.contains(&f.as_str()) && !f.contains("mono"))
        .collect();
    for f in &families {
        if BANNED_FONTS.iter().any(|b| f.contains(b)) {
            hits.push(Hit { rule: "banned-typeface", severity: "warning",
                evidence: format!("font-family: {}", f),
                fix: "the rules allow Geist, Manrope, Poppins (plus Geist Mono for code/figures)" });
        }
    }
    if primaries.len() > 1 {
        hits.push(Hit { rule: "one-typeface", severity: "warning",
            evidence: format!("{} families: {}", primaries.len(),
                primaries.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")),
            fix: "one typeface per site (a mono for code/data is the only exception)" });
    }
    if families.is_empty() { muted.push("one-typeface"); }

    // B9 focus ring
    if !low.contains(":focus") && !low.contains("focus-visible") {
        hits.push(Hit { rule: "focus-ring", severity: "error",
            evidence: "no :focus or :focus-visible anywhere".into(),
            fix: "every interactive element needs a visible focus ring" });
    }

    // motion: reduced motion
    if !low.contains("prefers-reduced-motion") {
        hits.push(Hit { rule: "reduced-motion", severity: "warning",
            evidence: "no prefers-reduced-motion".into(),
            fix: "honour reduced motion, especially for scroll reveals and the tagline reveal" });
    }

    // B7 performance: transition-all
    if low.contains("transition-all") || low.contains("transition: all") {
        hits.push(Hit { rule: "transition-all", severity: "warning",
            evidence: "transition-all / transition: all".into(),
            fix: "transition the properties you mean (transform, opacity, filter, colour)" });
    }

    // images need alt text
    let imgs = count(html, "<img");
    let alts = count(html, "alt=");
    if imgs > alts {
        hits.push(Hit { rule: "alt-text", severity: "error",
            evidence: format!("{} <img>, {} alt=", imgs, alts),
            fix: "alt text on every meaningful image" });
    }

    // ship requirements
    if !text_low.contains("<title") {
        hits.push(Hit { rule: "title", severity: "warning", evidence: "no <title>".into(),
            fix: "every page needs a title" });
    }
    if !text_low.contains("description") {
        hits.push(Hit { rule: "meta-description", severity: "warning", evidence: "no meta description".into(),
            fix: "add a meta description and og:image" });
    }
    if !text_low.contains("viewport") {
        hits.push(Hit { rule: "viewport", severity: "warning", evidence: "no viewport meta".into(),
            fix: "add <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">" });
    }
    if !text_low.contains("privacy") || !text_low.contains("terms") {
        hits.push(Hit { rule: "legal-links", severity: "warning",
            evidence: "privacy or terms link missing".into(),
            fix: "privacy and terms belong in the footer" });
    }
    if !text_low.contains("skip") {
        hits.push(Hit { rule: "skip-link", severity: "info", evidence: "no skip link".into(),
            fix: "add a skip to content link for keyboard users" });
    }
    let h1 = count(html, "<h1");
    if h1 == 0 {
        hits.push(Hit { rule: "heading", severity: "error", evidence: "no <h1>".into(),
            fix: "one h1 that states the offer" });
    } else if h1 > 1 {
        hits.push(Hit { rule: "heading", severity: "warning", evidence: format!("{} <h1>", h1),
            fix: "one h1 per page" });
    }

    // B11 tagline reveal
    if !low.contains("data-tagline") && !low.contains("class=\"tagline") && !low.contains("id=\"tagline") {
        hits.push(Hit { rule: "tagline-reveal", severity: "info",
            evidence: "no tagline section marker".into(),
            fix: "include the scroll-driven tagline reveal (two lines, words activating in order)" });
    }

    // errors first, then warnings, then info; identical evidence deduped, 4 per rule, 40 total
    let mut out: Vec<Value> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for sev in ["error", "warning", "info"] {
        for h in hits.iter().filter(|h| h.severity == sev) {
            if out.len() >= 40 { break; }
            let key = format!("{}|{}", h.rule, h.evidence);
            if seen.contains(&key) { continue; }
            if out.iter().filter(|o| o["severity"] == sev && o["rule"] == h.rule).count() >= 4 { continue; }
            seen.push(key);
            out.push(json!({"rule": h.rule, "severity": h.severity, "evidence": h.evidence, "fix": h.fix}));
        }
    }

    let mut rules_seen: Vec<(&str, &str)> = Vec::new();
    for h in hits.iter() {
        if !rules_seen.iter().any(|(r, _)| *r == h.rule) {
            rules_seen.push((h.rule, h.severity));
        }
    }
    let penalty: i32 = rules_seen.iter().map(|(_, sev)| match *sev {
        "error" => 12, "warning" => 6, _ => 2,
    }).sum();
    let score = (100 - penalty).max(0);

    json!({
        "score": score,
        "violations": out,
        "counts": {
            "error": out.iter().filter(|o| o["severity"] == "error").count(),
            "warning": out.iter().filter(|o| o["severity"] == "warning").count(),
            "info": out.iter().filter(|o| o["severity"] == "info").count()
        },
        "not_checked": muted,
        "note": "rules from github.com/elayadesign/ai-design-skills landing-page-design; pass the css in {css} when it is a separate file"
    })
}
