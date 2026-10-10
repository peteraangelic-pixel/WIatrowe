//! Generowanie wykresów SVG bez żadnych zależności (czysty `String`).
//! Dzięki temu raport HTML jest w 100% samowystarczalny.

/// Jedna seria danych.
#[derive(Clone, Debug)]
pub struct Series {
    pub name: String,
    pub color: String,
    pub points: Vec<(f64, f64)>,
    pub dashed: bool,
}

impl Series {
    pub fn new(name: &str, color: &str, points: Vec<(f64, f64)>) -> Self {
        Series {
            name: name.to_string(),
            color: color.to_string(),
            points,
            dashed: false,
        }
    }
    pub fn dashed(mut self) -> Self {
        self.dashed = true;
        self
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn fmt_tick(v: f64) -> String {
    let a = v.abs();
    if a >= 1000.0 {
        format!("{:.0}", v)
    } else if a >= 10.0 {
        format!("{:.0}", v)
    } else if a >= 1.0 {
        format!("{:.1}", v)
    } else {
        format!("{:.2}", v)
    }
}

fn nice_ticks(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    if !(hi > lo) {
        return vec![lo];
    }
    let raw = (hi - lo) / n as f64;
    let mag = 10f64.powf(raw.abs().log10().floor());
    let norm = raw / mag;
    let step = if norm <= 1.0 {
        1.0
    } else if norm <= 2.0 {
        2.0
    } else if norm <= 5.0 {
        5.0
    } else {
        10.0
    } * mag;
    let start = (lo / step).ceil() * step;
    let mut out = Vec::new();
    let mut v = start;
    while v <= hi + step * 0.001 {
        out.push(v);
        v += step;
    }
    out
}

/// Wykres liniowy z osiami i siatką.
pub fn line_chart(
    w: f64,
    h: f64,
    series: &[Series],
    xlabel: &str,
    ylabel: &str,
    xrange: Option<(f64, f64)>,
    yrange: Option<(f64, f64)>,
    legend: bool,
) -> String {
    let ml = 62.0;
    let mr = if legend { 190.0 } else { 18.0 };
    let mt = 16.0;
    let mb = 44.0;
    let pw = w - ml - mr;
    let ph = h - mt - mb;

    let mut xlo = f64::INFINITY;
    let mut xhi = f64::NEG_INFINITY;
    let mut ylo = f64::INFINITY;
    let mut yhi = f64::NEG_INFINITY;
    for s in series {
        for (x, y) in &s.points {
            if x.is_finite() && y.is_finite() {
                xlo = xlo.min(*x);
                xhi = xhi.max(*x);
                ylo = ylo.min(*y);
                yhi = yhi.max(*y);
            }
        }
    }
    if !xlo.is_finite() {
        xlo = 0.0;
        xhi = 1.0;
        ylo = 0.0;
        yhi = 1.0;
    }
    if let Some((a, b)) = xrange {
        xlo = a;
        xhi = b;
    }
    if let Some((a, b)) = yrange {
        ylo = a;
        yhi = b;
    }
    if !(yhi > ylo) {
        yhi = ylo + 1.0;
    }
    if !(xhi > xlo) {
        xhi = xlo + 1.0;
    }
    let ypad = 0.06 * (yhi - ylo);
    let ylo = (ylo - ypad).max(0.0).min(ylo);
    let yhi = yhi + ypad;

    let sx = |x: f64| ml + (x - xlo) / (xhi - xlo) * pw;
    let sy = |y: f64| mt + ph - (y - ylo) / (yhi - ylo) * ph;

    let mut o = String::new();
    o.push_str(&format!(
        r#"<svg viewBox="0 0 {w} {h}" width="100%" xmlns="http://www.w3.org/2000/svg" font-family="Inter,Segoe UI,system-ui,sans-serif">"#,
        w = w,
        h = h
    ));
    o.push_str(&format!(
        r#"<rect x="0" y="0" width="{w}" height="{h}" fill="#ffffff" rx="10"/>"#,
        w = w,
        h = h
    ));

    // siatka pozioma
    for t in nice_ticks(ylo, yhi, 5) {
        let y = sy(t);
        o.push_str(&format!(
            r#"<line x1="{x0}" y1="{y}" x2="{x1}" y2="{y}" stroke="#e7ecf3" stroke-width="1"/>"#,
            x0 = ml,
            x1 = ml + pw,
            y = y
        ));
        o.push_str(&format!(
            r#"<text x="{x}" y="{y}" text-anchor="end" font-size="11" fill="#7b8794">{t}</text>"#,
            x = ml - 8.0,
            y = y + 3.5,
            t = fmt_tick(t)
        ));
    }
    // siatka pionowa
    for t in nice_ticks(xlo, xhi, 6) {
        let x = sx(t);
        o.push_str(&format!(
            r#"<line x1="{x}" y1="{y0}" x2="{x}" y2="{y1}" stroke="#f1f4f8" stroke-width="1"/>"#,
            x = x,
            y0 = mt,
            y1 = mt + ph
        ));
        o.push_str(&format!(
            r#"<text x="{x}" y="{y}" text-anchor="middle" font-size="11" fill="#7b8794">{t}</text>"#,
            x = x,
            y = mt + ph + 16.0,
            t = fmt_tick(t)
        ));
    }
    // osie
    o.push_str(&format!(
        r#"<line x1="{x}" y1="{y0}" x2="{x}" y2="{y1}" stroke="#c8d0da" stroke-width="1.2"/>"#,
        x = ml,
        y0 = mt,
        y1 = mt + ph
    ));
    o.push_str(&format!(
        r#"<line x1="{x0}" y1="{y}" x2="{x1}" y2="{y}" stroke="#c8d0da" stroke-width="1.2"/>"#,
        x0 = ml,
        x1 = ml + pw,
        y = mt + ph
    ));
    // opisy osi
    o.push_str(&format!(
        r#"<text x="{x}" y="{y}" text-anchor="middle" font-size="12" fill="#4a5568">{l}</text>"#,
        x = ml + pw / 2.0,
        y = h - 8.0,
        l = esc(xlabel)
    ));
    o.push_str(&format!(
        r#"<text x="12" y="{y}" text-anchor="start" font-size="12" fill="#4a5568" transform="rotate(-90 12 {y})">{l}</text>"#,
        y = mt + ph / 2.0,
        l = esc(ylabel)
    ));

    // serie
    for s in series {
        let mut d = String::new();
        let mut first = true;
        for (x, y) in &s.points {
            if !x.is_finite() || !y.is_finite() {
                continue;
            }
            let (px, py) = (sx(*x), sy(*y));
            if first {
                d.push_str(&format!("M {:.2} {:.2} ", px, py));
                first = false;
            } else {
                d.push_str(&format!("L {:.2} {:.2} ", px, py));
            }
        }
        let dash = if s.dashed {
            r#" stroke-dasharray="6 4""#
        } else {
            ""
        };
        o.push_str(&format!(
            r#"<path d="{d}" fill="none" stroke="{c}" stroke-width="2.1" stroke-linejoin="round"{dash}/>"#,
            d = d,
            c = s.color,
            dash = dash
        ));
    }

    // legenda
    if legend {
        let mut ly = mt + 6.0;
        for s in series {
            o.push_str(&format!(
                r#"<rect x="{x}" y="{y}" width="12" height="12" rx="3" fill="{c}"/>"#,
                x = ml + pw + 14.0,
                y = ly,
                c = s.color
            ));
            let name: String = esc(&s.name).chars().take(26).collect();
            o.push_str(&format!(
                r#"<text x="{x}" y="{y}" font-size="11.5" fill="#3d4753">{n}</text>"#,
                x = ml + pw + 32.0,
                y = ly + 10.0,
                n = name
            ));
            ly += 17.0;
        }
    }
    o.push_str("</svg>");
    o
}

/// Wykres słupkowy poziomy (np. LCOE).
pub fn bar_chart(w: f64, items: &[(String, f64, String)], unit: &str, title: &str) -> String {
    let row = 26.0;
    let h = 34.0 + items.len() as f64 * row;
    let ml = 210.0;
    let mr = 96.0;
    let pw = w - ml - mr;
    let max = items
        .iter()
        .map(|(_, v, _)| *v)
        .fold(0.0, f64::max)
        .max(1e-9);
    let mut o = String::new();
    o.push_str(&format!(
        r#"<svg viewBox="0 0 {w} {h}" width="100%" xmlns="http://www.w3.org/2000/svg" font-family="Inter,Segoe UI,system-ui,sans-serif">"#,
        w = w,
        h = h
    ));
    o.push_str(&format!(
        r#"<rect x="0" y="0" width="{w}" height="{h}" fill="#ffffff" rx="10"/>"#,
        w = w,
        h = h
    ));
    o.push_str(&format!(
        r#"<text x="14" y="22" font-size="13" font-weight="600" fill="#2d3748">{t}</text>"#,
        t = esc(title)
    ));
    for (i, (label, v, color)) in items.iter().enumerate() {
        let y = 34.0 + i as f64 * row;
        let bw = (v / max) * pw;
        let lbl: String = esc(label).chars().take(34).collect();
        o.push_str(&format!(
            r#"<text x="{x}" y="{y}" font-size="11.5" fill="#3d4753">{l}</text>"#,
            x = ml - 10.0,
            y = y + 13.0,
            l = lbl
        ));
        o.push_str(&format!(
            r#"<rect x="{x}" y="{y}" width="{bw}" height="14" rx="4" fill="{c}"/>"#,
            x = ml,
            y = y + 2.0,
            bw = bw.max(1.0),
            c = color
        ));
        o.push_str(&format!(
            r#"<text x="{x}" y="{y}" font-size="11.5" fill="#5a6472">{v} {u}</text>"#,
            x = ml + bw.max(1.0) + 8.0,
            y = y + 13.0,
            v = fmt_tick(*v),
            u = esc(unit)
        ));
    }
    o.push_str("</svg>");
    o
}

/// Mały "wykres" w tabeli (sparkline).
pub fn sparkline(points: &[(f64, f64)], color: &str, w: f64, h: f64) -> String {
    if points.len() < 2 {
        return String::new();
    }
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for (_, y) in points {
        lo = lo.min(*y);
        hi = hi.max(*y);
    }
    let xlo = points[0].0;
    let xhi = points[points.len() - 1].0;
    if !(hi > lo) {
        hi = lo + 1.0;
    }
    if !(xhi > xlo) {
        return String::new();
    }
    let mut d = String::new();
    for (i, (x, y)) in points.iter().enumerate() {
        let px = (x - xlo) / (xhi - xlo) * w;
        let py = h - (y - lo) / (hi - lo) * (h - 2.0) - 1.0;
        if i == 0 {
            d.push_str(&format!("M {:.1} {:.1} ", px, py));
        } else {
            d.push_str(&format!("L {:.1} {:.1} ", px, py));
        }
    }
    format!(
        r#"<svg viewBox="0 0 {w} {h}" width="{w}" height="{h}" xmlns="http://www.w3.org/2000/svg"><path d="{d}" fill="none" stroke="{c}" stroke-width="1.6"/></svg>"#,
        w = w,
        h = h,
        d = d,
        c = color
    )
}
