//! Proceduralne sylwetki (SVG) poszczególnych koncepcji - do raportu.
//! Czysta geometria, bez bibliotek.

use std::f64::consts::{FRAC_PI_2, FRAC_PI_3};

const S: f64 = 120.0;

fn head(c: &str, extra: &str) -> String {
    format!(
        r#"<svg viewBox="0 0 {s} {s}" width="{s}" height="{s}" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="{c}" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">{e}</svg>"#,
        s = S,
        c = c,
        e = extra
    )
}

fn blades(cx: f64, cy: f64, n: usize, r: f64, w: f64, rot: f64) -> String {
    let mut o = String::new();
    for i in 0..n {
        let a = rot + 2.0 * std::f64::consts::PI * i as f64 / n as f64;
        let (sa, ca) = (a.sin(), a.cos());
        let px = -sa;
        let py = ca;
        let b = 0.22 * r;
        o.push_str(&format!(
            r#"<path d="M {x0} {y0} L {x1} {y1} L {x2} {y2} Z" fill="{c}" fill-opacity="0.22"/>"#,
            x0 = cx + ca * b,
            y0 = cy + sa * b,
            x1 = cx + ca * r + px * w,
            y1 = cy + sa * r + py * w,
            x2 = cx + ca * r - px * w * 0.35,
            y2 = cy + sa * r - py * w * 0.35,
            c = "currentColor"
        ));
    }
    o
}

/// Sylwetka dla danego identyfikatora wariantu.
pub fn shape(id: &str, color: &str) -> String {
    let c = color;
    match id {
        "hawt3" => head(
            c,
            &format!(
                r#"<line x1="60" y1="110" x2="60" y2="46"/><circle cx="60" cy="44" r="4" fill="{c}"/>{b}<line x1="44" y1="112" x2="76" y2="112"/>"#,
                c = c,
                b = blades(60.0, 44.0, 3, 34.0, 4.5, -FRAC_PI_2)
            ),
        ),
        "hawt2" => head(
            c,
            &format!(
                r#"<line x1="60" y1="110" x2="60" y2="46"/><circle cx="60" cy="44" r="4" fill="{c}"/>{b}<line x1="44" y1="112" x2="76" y2="112"/>"#,
                c = c,
                b = blades(60.0, 44.0, 2, 36.0, 6.0, -FRAC_PI_2)
            ),
        ),
        "hawt1" => head(
            c,
            &format!(
                r#"<line x1="60" y1="110" x2="60" y2="46"/><circle cx="60" cy="44" r="4" fill="{c}"/>{b}<circle cx="86" cy="44" r="7" fill="{c}" fill-opacity="0.35"/><line x1="60" y1="44" x2="86" y2="44"/><line x1="44" y1="112" x2="76" y2="112"/>"#,
                c = c,
                b = blades(60.0, 44.0, 1, 38.0, 7.0, -FRAC_PI_2)
            ),
        ),
        "hawt6" => head(
            c,
            &format!(
                r#"<line x1="60" y1="110" x2="60" y2="46"/><circle cx="60" cy="44" r="4" fill="{c}"/>{b}<line x1="44" y1="112" x2="76" y2="112"/>"#,
                c = c,
                b = blades(60.0, 44.0, 6, 33.0, 3.2, -FRAC_PI_2)
            ),
        ),
        "farm18" => {
            let mut b = String::new();
            for i in 0..18 {
                let a = 2.0 * std::f64::consts::PI * i as f64 / 18.0;
                b.push_str(&format!(
                    r#"<line x1="{x0}" y1="{y0}" x2="{x1}" y2="{y1}" stroke-width="1.6"/>"#,
                    x0 = 60.0 + a.cos() * 8.0,
                    y0 = 46.0 + a.sin() * 8.0,
                    x1 = 60.0 + a.cos() * 32.0,
                    y1 = 46.0 + a.sin() * 32.0
                ));
            }
            head(
                c,
                &format!(
                    r#"<line x1="60" y1="112" x2="60" y2="46" stroke-width="2.6"/>{b}<circle cx="60" cy="46" r="5"/><path d="M100 46 L112 40 L112 58 L100 52 Z" fill="{c}" fill-opacity="0.3"/><line x1="60" y1="46" x2="100" y2="46"/><path d="M28 112 L92 112"/><path d="M40 112 L60 86 L80 112"/>"#,
                    c = c,
                    b = b
                ),
            )
        }
        "dawt" => head(
            c,
            &format!(
                r#"<path d="M22 34 L38 62 L38 78 L22 106" stroke-width="2.6"/><path d="M98 34 L82 62 L82 78 L98 106" stroke-width="2.6"/><ellipse cx="60" cy="44" rx="34" ry="12"/><rect x="14" y="34" width="12" height="6" rx="2" fill="{c}" fill-opacity="0.35"/><rect x="94" y="34" width="12" height="6" rx="2" fill="{c}" fill-opacity="0.35"/>{b}<line x1="60" y1="106" x2="60" y2="118"/>"#,
                c = c,
                b = blades(60.0, 44.0, 5, 22.0, 3.0, -FRAC_PI_2)
            ),
        ),
        "counter" => head(
            c,
            &format!(
                r#"<line x1="60" y1="112" x2="60" y2="46"/><circle cx="60" cy="44" r="4" fill="{c}"/>{b1}{b2}<line x1="44" y1="114" x2="76" y2="114"/>"#,
                c = c,
                b1 = blades(60.0, 44.0, 3, 32.0, 4.2, -FRAC_PI_2),
                b2 = blades(60.0, 44.0, 3, 24.0, 3.0, -FRAC_PI_2 + FRAC_PI_3)
            ),
        ),
        "venturi" => head(
            c,
            &format!(
                r#"<path d="M12 26 L52 26 L40 52 L40 68 L52 94 L12 94 Z" fill="{c}" fill-opacity="0.10"/><path d="M108 26 L68 26 L80 52 L80 68 L68 94 L108 94 Z" fill="{c}" fill-opacity="0.10"/>{b}<path d="M60 118 L60 106"/><path d="M40 118 L80 118"/>"#,
                c = c,
                b = blades(60.0, 60.0, 3, 15.0, 2.4, 0.0)
            ),
        ),
        "hrotor" => head(
            c,
            &format!(
                r#"<line x1="60" y1="112" x2="60" y2="14" stroke-width="2.6"/><path d="M28 22 Q22 60 28 98" stroke-width="3.2"/><path d="M92 22 Q98 60 92 98" stroke-width="3.2"/><line x1="60" y1="30" x2="30" y2="30"/><line x1="60" y1="90" x2="30" y2="90"/><line x1="60" y1="30" x2="90" y2="30"/><line x1="60" y1="90" x2="90" y2="90"/><ellipse cx="60" cy="14" rx="34" ry="6" stroke-dasharray="4 4" stroke-width="1.2"/><path d="M34 118 L86 118"/>"#,
            ),
        ),
        "helical" => head(
            c,
            &format!(
                r#"<line x1="60" y1="112" x2="60" y2="14" stroke-width="2.6"/><path d="M28 20 Q40 40 28 60 Q16 80 28 100" stroke-width="3.2"/><path d="M92 20 Q80 40 92 60 Q104 80 92 100" stroke-width="3.2"/><line x1="60" y1="34" x2="30" y2="34"/><line x1="60" y1="86" x2="30" y2="86"/><line x1="60" y1="34" x2="90" y2="34"/><line x1="60" y1="86" x2="90" y2="86"/><path d="M34 118 L86 118"/>"#,
            ),
        ),
        "troposkien" => head(
            c,
            &format!(
                r#"<line x1="60" y1="110" x2="60" y2="12" stroke-width="2.6"/><path d="M60 12 Q18 50 60 108" stroke-width="3.4"/><path d="M60 12 Q102 50 60 108" stroke-width="3.4"/><ellipse cx="60" cy="12" rx="30" ry="5" stroke-dasharray="4 4" stroke-width="1.2"/><path d="M34 116 L86 116"/>"#,
            ),
        ),
        "savonius" => head(
            c,
            &format!(
                r#"<ellipse cx="60" cy="24" rx="32" ry="8"/><ellipse cx="60" cy="98" rx="32" ry="8"/><path d="M46 24 A 16 16 0 0 0 46 56" stroke-width="3.2"/><path d="M74 98 A 16 16 0 0 0 74 66" stroke-width="3.2"/><line x1="60" y1="24" x2="60" y2="98" stroke-width="2.2"/><line x1="60" y1="108" x2="60" y2="118"/><path d="M40 118 L80 118"/>"#,
            ),
        ),
        "hybrid" => head(
            c,
            &format!(
                r#"<line x1="60" y1="112" x2="60" y2="16" stroke-width="2.6"/><path d="M26 24 Q20 60 26 96" stroke-width="3.2"/><path d="M94 24 Q100 60 94 96" stroke-width="3.2"/><line x1="60" y1="34" x2="28" y2="34"/><line x1="60" y1="86" x2="28" y2="86"/><line x1="60" y1="34" x2="92" y2="34"/><line x1="60" y1="86" x2="92" y2="86"/><path d="M52 50 A 9 9 0 0 0 52 68" stroke-width="2.4"/><path d="M68 68 A 9 9 0 0 0 68 50" stroke-width="2.4"/><path d="M34 118 L86 118"/>"#,
            ),
        ),
        "magnus" => head(
            c,
            &format!(
                r#"<line x1="60" y1="112" x2="60" y2="46"/><circle cx="60" cy="44" r="5" fill="{c}"/><rect x="54" y="10" width="12" height="30" rx="6" fill="{c}" fill-opacity="0.18"/><rect x="86" y="52" width="30" height="12" rx="6" fill="{c}" fill-opacity="0.18"/><rect x="28" y="52" width="30" height="12" rx="6" fill="{c}" fill-opacity="0.18"/><path d="M20 34 a 6 6 0 1 0 0.1 0" stroke-dasharray="3 3" stroke-width="1.2"/><line x1="44" y1="112" x2="76" y2="112"/>"#,
                c = c
            ),
        ),
        "bladeless" => head(
            c,
            &format!(
                r#"<path d="M52 108 L55 24 Q60 12 65 24 L68 108 Z" fill="{c}" fill-opacity="0.14"/><line x1="60" y1="18" x2="60" y2="108" stroke-dasharray="5 5" stroke-width="1.2"/><path d="M34 60 L46 60" stroke-width="1.6"/><path d="M74 60 L86 60" stroke-width="1.6"/><path d="M40 54 L34 60 L40 66" stroke-width="1.6"/><path d="M80 54 L86 60 L80 66" stroke-width="1.6"/><ellipse cx="60" cy="110" rx="16" ry="4" fill="{c}" fill-opacity="0.25"/>"#,
                c = c
            ),
        ),
        "kite" => head(
            c,
            &format!(
                r#"<path d="M40 112 L80 112 L74 100 L46 100 Z" fill="{c}" fill-opacity="0.2"/><path d="M60 100 Q40 70 78 52 Q96 44 84 30 Q70 22 58 40" stroke-width="1.8" stroke-dasharray="5 4" fill="none"/><path d="M58 40 Q68 30 82 34 Q78 48 66 48 Z" fill="{c}" fill-opacity="0.25" stroke-width="2.4"/><path d="M34 116 L86 116"/>"#,
                c = c
            ),
        ),
        "belt" => head(
            c,
            &format!(
                r#"<line x1="20" y1="50" x2="20" y2="94" stroke-width="3"/><line x1="100" y1="50" x2="100" y2="94" stroke-width="3"/><path d="M20 62 Q40 50 60 62 T100 62" stroke-width="3"/><path d="M20 82 Q40 70 60 82 T100 82" stroke-width="1.4" stroke-dasharray="4 4"/><path d="M16 96 L24 96"/><path d="M96 96 L104 96"/><line x1="20" y1="112" x2="100" y2="112"/>"#,
            ),
        ),
        _ => head(c, r#"<circle cx="60" cy="60" r="30"/>"#),
    }
}

/// Paleta kolorów dla rodzin.
pub fn family_color(id: &str) -> &'static str {
    match id {
        "hawt3" | "hawt2" | "hawt1" | "hawt6" | "counter" => "#2f6fb2",
        "farm18" => "#7a8b3a",
        "dawt" | "venturi" => "#b5651d",
        "hrotor" | "helical" | "troposkien" | "hybrid" => "#2a9d8f",
        "savonius" => "#c1440e",
        "magnus" => "#7b2cbf",
        "bladeless" => "#0891b2",
        "kite" => "#d90429",
        "belt" => "#6c757d",
        _ => "#495057",
    }
}
