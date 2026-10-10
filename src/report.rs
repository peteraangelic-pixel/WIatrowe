//! Wyjście: CSV, JSON oraz samowystarczalny raport HTML z wykresami SVG.

use std::fmt::Write;
use std::path::Path;

use crate::charts::{bar_chart, line_chart, sparkline, Series};
use crate::designs::Design;
use crate::shapes::{family_color, shape};
use crate::sweep::{score, Variant};

fn num(x: f64, d: usize) -> String {
    if !x.is_finite() {
        return "".to_string();
    }
    format!("{:.*}", d, x)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn write_file(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::write(path, content)
}

/// Tabela podsumowująca (CSV).
pub fn write_summary_csv(path: &Path, v: &[Variant]) -> std::io::Result<()> {
    let mut s = String::from(
        "id,nazwa,rodzina,skala,jednostka_skali,pole_m2,wysokosc_m,Cp_max,lambda_opt,skrecenie_k,\
moc_kW,AEP_MWh,capacity_factor,CAPEX_PLN,PLN_na_kW,LCOE_PLN_na_MWh,kWh_na_kg_rok,\
m2_na_MWh_rok,masa_t,masa_lopat_t,zycie_lat,pewnosc\n",
    );
    for x in v {
        let _ = writeln!(
            s,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            x.id,
            esc(&x.name),
            esc(&x.family),
            num(x.scale, 2),
            x.scale_kind,
            num(x.area, 1),
            num(x.hub, 1),
            num(x.cp_max, 4),
            num(x.lambda_opt, 3),
            num(x.spin_opt, 3),
            num(x.rated_kw, 1),
            num(x.aep_kwh / 1000.0, 1),
            num(x.capacity_factor, 4),
            num(x.econ.capex_total, 0),
            num(x.econ.pln_per_kw, 0),
            num(x.econ.lcoe, 1),
            num(x.econ.kwh_per_kg, 2),
            num(x.econ.m2_per_mwh, 2),
            num(x.econ.mass_total / 1000.0, 2),
            num(x.econ.mass_blades / 1000.0, 2),
            num(x.econ.lifetime_y, 0),
            esc(&x.confidence)
        );
    }
    write_file(path, &s)
}

/// Krzywe mocy (CSV): wiersze = prędkość, kolumny = warianty.
pub fn write_curves_csv(path: &Path, v: &[Variant]) -> std::io::Result<()> {
    let n = v.first().map(|x| x.curve.len()).unwrap_or(0);
    let mut s = String::from("v_mps");
    for x in v {
        let _ = write!(s, ",{}@{}", x.id, num(x.scale, 2));
    }
    s.push('\n');
    for i in 0..n {
        let _ = write!(s, "{}", num(v[0].curve[i].0, 2));
        for x in v {
            let _ = write!(s, ",{}", num(x.curve[i].1 / 1000.0, 4));
        }
        s.push('\n');
    }
    write_file(path, &s)
}

/// Krzywe Cp(lambda) (CSV).
pub fn write_cp_csv(path: &Path, v: &[Variant]) -> std::io::Result<()> {
    let n = v.first().map(|x| x.cp_curve.len()).unwrap_or(0);
    let m = n.max(1);
    let mut s = String::from("x");
    for x in v {
        let _ = write!(s, ",{}@{}", x.id, num(x.scale, 2));
    }
    s.push('\n');
    for i in 0..m {
        let x0 = v.iter().find_map(|y| y.cp_curve.get(i).map(|p| p.0));
        let _ = write!(s, "{}", num(x0.unwrap_or(0.0), 3));
        for x in v {
            let val = x.cp_curve.get(i).map(|p| p.1).unwrap_or(f64::NAN);
            let _ = write!(s, ",{}", num(val, 5));
        }
        s.push('\n');
    }
    write_file(path, &s)
}

/// Zrzut liczb kontrolnych do porównania z implementacją referencyjną (Python).
pub fn write_checksums(path: &Path, rows: &[(String, f64, f64, f64)]) -> std::io::Result<()> {
    let mut s = String::from("id,scale,lambda,cp\n");
    for (id, scale, l, cp) in rows {
        let _ = writeln!(s, "{},{},{:.6},{:.9}", id, scale, l, cp);
    }
    write_file(path, &s)
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// JSON z wynikami (do dalszej obróbki).
pub fn write_json(path: &Path, v: &[Variant], meta: &[(&str, String)]) -> std::io::Result<()> {
    let mut s = String::from("{\n  \"meta\": {\n");
    for (i, (k, val)) in meta.iter().enumerate() {
        let comma = if i + 1 == meta.len() { "" } else { "," };
        let _ = writeln!(s, "    \"{}\": \"{}\"{}", k, json_escape(val), comma);
    }
    s.push_str("  },\n  \"variants\": [\n");
    for (i, x) in v.iter().enumerate() {
        let comma = if i + 1 == v.len() { "" } else { "," };
        let _ = write!(
            s,
            concat!(
                "    {{\"id\": \"{}\", \"name\": \"{}\", \"family\": \"{}\", \"scale\": {}, ",
                "\"area\": {:.3}, \"hub\": {:.2}, \"cp_max\": {:.5}, \"lambda_opt\": {:.4}, ",
                "\"rated_kw\": {:.3}, \"aep_mwh\": {:.3}, \"cf\": {:.5}, ",
                "\"capex_pln\": {:.1}, \"pln_per_kw\": {:.1}, \"lcoe\": {:.2}, ",
                "\"kwh_per_kg\": {:.4}, \"m2_per_mwh\": {:.3}, \"mass_t\": {:.3}, ",
                "\"score\": {:.2}, \"note\": \"{}\"}}{}\n"
            ),
            x.id,
            json_escape(&x.name),
            json_escape(&x.family),
            num(x.scale, 3),
            x.area,
            x.hub,
            x.cp_max,
            x.lambda_opt,
            x.rated_kw,
            x.aep_kwh / 1000.0,
            x.capacity_factor,
            x.econ.capex_total,
            x.econ.pln_per_kw,
            x.econ.lcoe,
            x.econ.kwh_per_kg,
            x.econ.m2_per_mwh,
            x.econ.mass_total / 1000.0,
            score(x),
            json_escape(&x.note),
            comma
        );
    }
    s.push_str("  ]\n}\n");
    write_file(path, &s)
}

const CSS: &str = r#"
:root { --bg:#f5f7fb; --card:#ffffff; --ink:#1f2933; --muted:#6b7684; --line:#e3e8ef; --acc:#2f6fb2; }
* { box-sizing: border-box; }
body { margin:0; background:var(--bg); color:var(--ink);
  font-family: Inter, "Segoe UI", system-ui, -apple-system, "Helvetica Neue", Arial, sans-serif;
  line-height:1.55; }
header { padding: 34px 28px 22px; background: linear-gradient(135deg,#0f2740,#2f6fb2); color:#fff; }
header h1 { margin:0 0 6px; font-size: 27px; letter-spacing:.2px; }
header p { margin:0; opacity:.9; max-width: 1000px; }
main { max-width: 1180px; margin: 0 auto; padding: 24px 18px 70px; }
section { background: var(--card); border:1px solid var(--line); border-radius: 14px;
  padding: 20px 22px; margin-bottom: 22px; box-shadow: 0 1px 2px rgba(16,24,40,.04); }
h2 { margin: 0 0 4px; font-size: 19px; }
h2 .sub { font-weight:400; color:var(--muted); font-size:13.5px; margin-left:8px; }
table { border-collapse: collapse; width: 100%; font-size: 13px; }
th, td { padding: 7px 8px; text-align: right; border-bottom: 1px solid var(--line); }
th { text-align: right; color: var(--muted); font-weight: 600; background:#fafbfd; position: sticky; top: 0; }
td.l, th.l { text-align: left; }
tr:hover td { background: #f8fbff; }
.bad { color:#c0392b; } .good { color:#1e7d4f; }
.grid { display:grid; grid-template-columns: repeat(auto-fit, minmax(320px,1fr)); gap:16px; }
.card { border:1px solid var(--line); border-radius:12px; padding:14px 16px; background:#fff; }
.card .top { display:flex; gap:14px; align-items:center; }
.card h3 { margin:0 0 2px; font-size:15.5px; }
.card .fam { color:var(--muted); font-size:12.5px; }
.kv { display:grid; grid-template-columns: auto auto; gap:2px 12px; font-size:12.5px; margin-top:10px; }
.kv div:nth-child(odd) { color:var(--muted); }
.kv div:nth-child(even) { text-align:right; font-variant-numeric: tabular-nums; }
.note { font-size:12.5px; color:#4a5568; margin-top:10px; border-left:3px solid var(--line); padding-left:9px; }
.conf { font-size:11.5px; color:var(--muted); margin-top:8px; }
.pill { display:inline-block; padding:2px 9px; border-radius:999px; font-size:11.5px; font-weight:600; }
ul.tight { margin:8px 0 0; padding-left:20px; font-size:13.5px; }
code { background:#eef2f7; padding:1px 5px; border-radius:4px; font-size:12.5px; }
footer { color:var(--muted); font-size:12.5px; text-align:center; padding: 18px; }
.scroll { overflow-x:auto; }
"#;

fn pill(lcoe: f64) -> String {
    let (t, c) = if lcoe < 250.0 {
        ("LCOE bardzo dobry", "#d7f2e3")
    } else if lcoe < 400.0 {
        ("LCOE dobry", "#e6f2ff")
    } else if lcoe < 800.0 {
        ("LCOE średni", "#fdf0d5")
    } else {
        ("LCOE słaby", "#fde2e1")
    };
    format!(
        r#"<span class="pill" style="background:{c};color:#1f2933">{t}</span>"#,
        c = c,
        t = t
    )
}

/// Raport HTML.
pub fn write_html(path: &Path, v: &[Variant], _designs: &[Design], meta: &[(&str, String)]) -> std::io::Result<()> {
    let mut o = String::new();
    o.push_str("<!DOCTYPE html><html lang=\"pl\"><head><meta charset=\"utf-8\">");
    o.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    o.push_str("<title>WIatrowe - symulacja alternatywnych elektrowni wiatrowych</title>");
    let _ = write!(o, "<style>{}</style></head><body>", CSS);

    // nagłówek
    o.push_str(
        r#"<header><h1>WIatrowe - przesiewowe porównanie 17 kształtów elektrowni wiatrowych</h1>
<p>Symulator w Rust + rayon: BEMT (wirniki łopatowe, dyfuzor, Magnus), DMST (Darrieus),
modele quasi-statyczne i empiryczne (Savonius, VIV, lej Venturiego, taśma) oraz wzór Loyda
(latawiec AWE) - połączone z jednolitym modelem masy, CAPEX-u, OPEX-u i LCOE.
To narzędzie do <b>przesiewu koncepcji</b>, nie do certyfikacji: każdy wynik ma przypisaną
pewność modelu.</p></header><main>"#,
    );

    // meta
    let mut meta_html = String::new();
    for (k, val) in meta {
        let _ = write!(meta_html, "<li><b>{}</b>: {}</li>", esc(k), esc(val));
    }

    // ---- ranking
    let top: Vec<&Variant> = v
        .iter()
        .filter(|x| x.econ.lcoe.is_finite() && x.aep_kwh > 1.0)
        .take(20)
        .collect();

    o.push_str(r#"<section><h2>Ranking według LCOE<span class="sub">20 najlepszych wariantów (każdy wariant liczony w kilku skalach)</span></h2><div class="scroll"><table>"#);
    o.push_str(
        r#"<tr><th class="l">#</th><th class="l">Wariant</th><th>Skala</th><th>Moc<br>kW</th>
<th>Cp<br>max</th><th>λ<br>opt</th><th>AEP<br>MWh/rok</th><th>CF</th>
<th>CAPEX<br>tys. PLN</th><th>PLN<br>/kW</th><th>LCOE<br>PLN/MWh</th>
<th>kWh<br>/kg·rok</th><th>m²<br>/MWh·rok</th><th>Krzywa</th></tr>"#,
    );
    for (i, x) in top.iter().enumerate() {
        let col = family_color(&x.id);
        let spark = sparkline(&x.curve, col, 92.0, 26.0);
        let _ = write!(
            o,
            concat!(
                "<tr><td class=\"l\">{}</td><td class=\"l\"><b>{}</b><br><span style=\"color:#6b7684;font-size:11.5px\">{} · {}</span></td>",
                "<td>{} {}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td>",
                "<td>{}</td><td>{}</td><td><b>{}</b></td><td>{}</td><td>{}</td><td>{}</td></tr>"
            ),
            i + 1,
            esc(&x.name),
            esc(&x.family),
            esc(&x.id),
            num(x.scale, 1),
            x.scale_kind,
            num(x.rated_kw, 0),
            num(x.cp_max, 3),
            num(x.lambda_opt, 2),
            num(x.aep_kwh / 1000.0, 0),
            num(x.capacity_factor, 3),
            num(x.econ.capex_total / 1000.0, 0),
            num(x.econ.pln_per_kw, 0),
            num(x.econ.lcoe, 0),
            num(x.econ.kwh_per_kg, 1),
            num(x.econ.m2_per_mwh, 1),
            spark
        );
    }
    o.push_str("</table></div></section>");

    // ---- wykresy LCOE / materiał / CF
    let items: Vec<(String, f64, String)> = top
        .iter()
        .take(14)
        .map(|x| {
            (
                format!("{} ({}{})", x.name, num(x.scale, 1), x.scale_kind),
                x.econ.lcoe,
                family_color(&x.id).to_string(),
            )
        })
        .collect();
    o.push_str(&format!(
        r#"<section><h2>Koszt energii (LCOE)<span class="sub">niżej = lepiej</span></h2>{}</section>"#,
        bar_chart(1000.0, &items, "PLN/MWh", "LCOE [PLN/MWh]")
    ));

    let mut by_mat: Vec<&Variant> = v.iter().filter(|x| x.aep_kwh > 1.0).collect();
    by_mat.sort_by(|a, b| {
        b.econ
            .kwh_per_kg
            .partial_cmp(&a.econ.kwh_per_kg)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let items2: Vec<(String, f64, String)> = by_mat
        .iter()
        .take(14)
        .map(|x| {
            (
                format!("{} ({}{})", x.name, num(x.scale, 1), x.scale_kind),
                x.econ.kwh_per_kg,
                family_color(&x.id).to_string(),
            )
        })
        .collect();
    o.push_str(&format!(
        r#"<section><h2>Energia z kilograma materiału<span class="sub">ile rocznie kWh uzyskamy z każdego kg konstrukcji - miara "materiałochłonności"</span></h2>{}</section>"#,
        bar_chart(1000.0, &items2, "kWh/kg·rok", "AEP / masa całkowita")
    ));

    // ---- krzywe mocy
    let mut series = Vec::new();
    for x in top.iter().take(8) {
        let pts: Vec<(f64, f64)> = x
            .curve
            .iter()
            .map(|(v, p)| (*v, p / 1000.0))
            .filter(|(v, _)| *v <= 28.0)
            .collect();
        series.push(Series::new(
            &format!("{} ({} {})", x.name, num(x.scale, 1), x.scale_kind),
            family_color(&x.id),
            pts,
        ));
    }
    o.push_str(&format!(
        r#"<section><h2>Krzywe mocy 8 najlepszych wariantów<span class="sub">moc elektryczna na wyjściu [kW] od prędkości wiatru na wysokości piasty</span></h2>{}</section>"#,
        line_chart(1000.0, 430.0, &series, "prędkość wiatru [m/s]", "moc [kW]", Some((0.0, 28.0)), None, true)
    ));

    // ---- Cp(lambda)
    let mut series2 = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for x in v.iter() {
        if x.cp_max <= 0.0 || !seen.insert(x.id.clone()) {
            continue;
        }
        series2.push(Series::new(
            &format!("{} ({} {})", x.name, num(x.scale, 1), x.scale_kind),
            family_color(&x.id),
            x.cp_curve.clone(),
        ));
    }
    let betz: Vec<(f64, f64)> = (0..=60).map(|i| (i as f64 * 0.5, 16.0 / 27.0)).collect();
    series2.push(Series::new("limit Betza (16/27)", "#c0392b", betz).dashed());
    o.push_str(&format!(
        r#"<section><h2>Cp w funkcji prędkości obrotowej<span class="sub">dla wirników: λ = ΩR/V; dla latawca i VIV: oś X to prędkość wiatru</span></h2>{}</section>"#,
        line_chart(1000.0, 470.0, &series2, "λ (lub v [m/s])", "Cp [-]", None, Some((0.0, 1.15)), true)
    ));

    // ---- karty wariantów
    o.push_str(r#"<section><h2>Wszystkie warianty - karty<span class="sub">najlepsza skala dla każdej koncepcji</span></h2><div class="grid">"#);
    let mut best_per_id: std::collections::HashMap<String, &Variant> = std::collections::HashMap::new();
    for x in v.iter() {
        let entry = best_per_id.get(&x.id);
        if entry.is_none() || x.econ.lcoe < entry.unwrap().econ.lcoe {
            best_per_id.insert(x.id.clone(), x);
        }
    }
    let mut cards: Vec<&Variant> = best_per_id.values().copied().collect();
    cards.sort_by(|a, b| {
        a.econ
            .lcoe
            .partial_cmp(&b.econ.lcoe)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for x in cards {
        let col = family_color(&x.id);
        let _ = write!(
            o,
            concat!(
                r#"<div class="card"><div class="top"><div style="color:{col}">{}</div>"#,
                r#"<div><h3>{}</h3><div class="fam">{} · skala {} {}</div><div class="fam">kod: {}</div></div></div>"#,
                r#"<div class="kv">"#,
                r#"<div>Cp max</div><div>{}</div>"#,
                r#"<div>λ opt</div><div>{}</div>"#,
                r#"<div>moc znamionowa</div><div>{} kW</div>"#,
                r#"<div>AEP</div><div>{} MWh/rok</div>"#,
                r#"<div>capacity factor</div><div>{}</div>"#,
                r#"<div>CAPEX</div><div>{} tys. PLN ({} PLN/kW)</div>"#,
                r#"<div>LCOE</div><div><b>{}</b> PLN/MWh</div>"#,
                r#"<div>masa / kWh z kg</div><div>{} t · {} kWh/kg·rok</div>"#,
                r#"<div>pole / teren</div><div>{} m² · {} m²/MWh</div>"#,
                r#"</div><div style="margin-top:10px">{}</div>"#,
                r#"<div class="note">{}</div><div class="conf">Pewność modelu: {}</div></div>"#
            ),
            shape(&x.id, col),
            esc(&x.name),
            esc(&x.family),
            esc(&x.id),
            num(x.scale, 1),
            x.scale_kind,
            num(x.cp_max, 3),
            num(x.lambda_opt, 2),
            num(x.rated_kw, 0),
            num(x.aep_kwh / 1000.0, 0),
            num(x.capacity_factor, 3),
            num(x.econ.capex_total / 1000.0, 0),
            num(x.econ.pln_per_kw, 0),
            num(x.econ.lcoe, 0),
            num(x.econ.mass_total / 1000.0, 1),
            num(x.econ.kwh_per_kg, 1),
            num(x.area, 0),
            num(x.econ.m2_per_mwh, 1),
            pill(x.econ.lcoe),
            esc(&x.note),
            esc(&x.confidence),
            col = col
        );
    }
    o.push_str("</div></section>");

    // ---- założenia
    o.push_str(&format!(
        concat!(
            r#"<section><h2>Jak to policzono - założenia i ograniczenia</h2>"#,
            r#"<ul class="tight">{}</ul>"#,
            r#"<h2 style="margin-top:18px">Parametry uruchomienia</h2><ul class="tight">{}</ul>"#,
            r#"<h2 style="margin-top:18px">Reprodukcja</h2>"#,
            r#"<p style="font-size:13.5px">Kod w Rust (+rayon). <code>cargo run --release -- --mode sweep --out results</code>. "#,
            r#"Testy: <code>cargo test</code>. Benchmark Rust vs Python (implementacja referencyjna "#,
            r#"w <code>tools/reference_sweep.py</code>): <code>cargo run --release -- --mode bench</code>. "#,
            r#"Pełny raport, CSV i JSON są generowane automatycznie przez GitHub Actions.</p></section>"#
        ),
        assumptions(),
        meta_html
    ));

    o.push_str(
        r#"<footer>WIatrowe · model przesiewowy (screening) · wyniki nie zastępują pomiarów terenowych ani CFD</footer>"#,
    );
    o.push_str("</main></body></html>");
    write_file(path, &o)
}

fn assumptions() -> String {
    let items = [
        "Aerodynamika: BEMT (Burton, poprawka Prandtla i Glauerta) dla wirników łopatowych, DMST (Paraschivoiu, ślad równowagowy) dla Darrieusa, krzywe empiryczne skalibrowane do zakresów z literatury dla Savoniusa i taśmy, wzór Loyda dla latawca, model pasma synchronizacji Strouhala dla VIV.",
        "Cp odniesione jest do pola zakreślanego (lub pola wlotu dla leja / pola równoważnego dla latawca) - dlatego DAWT i latawiec mogą przekroczyć wartość Betza liczoną na polu wirnika, co nie łamie fizyki, bo liczone jest wtedy względem innego pola.",
        "Masa: łopata kompositowa 0.55·R^2.5 kg (NREL 5 MW ≈ 17.7 t), wieża z momentu wywracającego (≈320 t dla NREL 5 MW), fundament 8.5 kg/kNm, przekładnia+generator ≈ 45 kg/kW.",
        "Ceny: kompozyt 52 PLN/kg, stalowa wieża 8 PLN/kg, przekładnia 20-30 PLN/kg, elektronika 320 PLN/kW, BOS 1050 PLN/kW × czynnik montażu; kalibracja globalna 1.06 tak, by bazowy HAWT 5 MW wypadał ~5 000-6 000 PLN/kW.",
        "OPEX 95 PLN/kW/rok × czynnik dostępności (serwis przy ziemi: 0.55-0.60; lotny: 1.5), WACC 7%, czas życia 8-25 lat w zależności od koncepcji.",
        "Wiatr: Weibull k=2.0, średnia 6.5 m/s na 100 m, ścinanie potęgowe α=0.20 (premia za wysokość piasty i za 300 m latawca jest policzona jawnie).",
        "Sterowanie: maszyny z regulacją kąta łopat utrzymują moc znamionową powyżej V_nom; bez regulacji moc rośnie dalej, więc generator i konstrukcja są przewymiarowane (to widać w koszcie).",
        "Czego tu nie ma: aeroelastyki, zmęczenia materiałowego (poza prostymi czynnikami), hałasu, oddziaływania parku (wake losses), kosztów sieci systemowych, ryzyka technologicznego dla AWE/VIV (modele o niskiej pewności).",
    ];
    let mut s = String::new();
    for i in items {
        let _ = write!(s, "<li>{}</li>", esc(i));
    }
    s
}
