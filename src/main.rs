//! CLI symulatora `wiatrowe`.
//!
//!   cargo run --release -- --mode sweep    --quality normal   # pełne przeliczenie + raport
//!   cargo run --release -- --mode bench    --repeat 12        # skalowanie rayonem
//!   cargo run --release -- --mode checksum                    # liczby kontrolne (vs Python)

use std::time::Instant;

use wiatrowe::climate::Climate;
use wiatrowe::designs::{catalog, Grid};
use wiatrowe::report;
use wiatrowe::sweep::{self, SweepCfg};

fn parse_args() -> std::collections::HashMap<String, String> {
    let mut m = std::collections::HashMap::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        if let Some(key) = a.strip_prefix("--") {
            let val = it.next().unwrap_or_else(|| "1".to_string());
            m.insert(key.to_string(), val);
        }
    }
    m
}

fn get<'a>(m: &'a std::collections::HashMap<String, String>, k: &str, d: &'a str) -> &'a str {
    m.get(k).map(|s| s.as_str()).unwrap_or(d)
}

fn print_table(v: &[wiatrowe::sweep::Variant], limit: usize) {
    println!(
        "{:<38} {:>7} {:>8} {:>7} {:>6} {:>9} {:>7} {:>9} {:>8}",
        "wariant", "skala", "moc kW", "Cp", "CF", "AEP MWh", "zł/kW", "LCOE", "kWh/kg"
    );
    println!("{}", "-".repeat(108));
    for x in v.iter().take(limit) {
        println!(
            "{:<38} {:>7} {:>8} {:>7} {:>6} {:>9} {:>7} {:>9} {:>8}",
            format!("{} [{}]", truncate(&x.name, 30), x.id),
            fmt(x.scale, 1),
            fmt(x.rated_kw, 0),
            fmt(x.cp_max, 3),
            fmt(x.capacity_factor, 3),
            fmt(x.aep_kwh / 1000.0, 0),
            fmt(x.econ.pln_per_kw, 0),
            fmt(x.econ.lcoe, 0),
            fmt(x.econ.kwh_per_kg, 1),
        );
    }
}

fn fmt(v: f64, d: usize) -> String {
    if !v.is_finite() {
        return "-".to_string();
    }
    format!("{:.*}", d, v)
}

fn truncate(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    while out.chars().count() < n {
        out.push(' ');
    }
    out
}

fn main() {
    let args = parse_args();
    let mode = get(&args, "mode", "sweep").to_string();
    let quality = match get(&args, "quality", "normal") {
        "fast" => Grid::coarse(),
        "fine" => Grid::fine(),
        _ => Grid::normal(),
    };
    let repeat: usize = get(&args, "repeat", "10").parse().unwrap_or(10);
    let out_dir = std::path::PathBuf::from(get(&args, "out", "results"));
    let threads: Option<usize> = args.get("threads").and_then(|s| s.parse().ok());
    let climate = match get(&args, "climate", "poland") {
        "coastal" => Climate::coastal(),
        _ => Climate::poland(),
    };

    let pool = match threads {
        Some(n) => Some(
            rayon::ThreadPoolBuilder::new()
                .num_threads(n)
                .build()
                .expect("nie można utworzyć puli wątków"),
        ),
        None => None,
    };

    let mut all = catalog();
    if let Some(list) = args.get("designs") {
        let keep: Vec<String> = list.split(',').map(|s| s.trim().to_string()).collect();
        all.retain(|d| keep.iter().any(|k| k == d.id));
    }

    let cfg = SweepCfg {
        climate,
        ..SweepCfg::new(quality)
    };

    let run = || match mode.as_str() {
        "bench" => bench(&all, &cfg, repeat),
        "checksum" => checksum(&all, &cfg, &out_dir),
        _ => sweep_mode(&all, &cfg, &out_dir),
    };

    match pool {
        Some(p) => p.install(run),
        None => run(),
    }
}

fn sweep_mode(all: &[wiatrowe::designs::Design], cfg: &SweepCfg, out: &std::path::Path) {
    let t0 = Instant::now();
    let variants = sweep::run(all, cfg);
    let dt = t0.elapsed();

    let n_eval = variants.len() * ((cfg.v_max / cfg.dv) as usize + 41);
    println!(
        "\n[wiatrowe] przeliczono {} wariantów ({} skal) w {:.2} s · {} wątków rayon · ~{} ewaluacji",
        variants.len(),
        all.iter().map(|d| d.scales.len()).sum::<usize>(),
        dt.as_secs_f64(),
        rayon::current_num_threads(),
        n_eval
    );
    print_table(&variants, 24);

    let meta = vec![
        ("Wątki rayon", rayon::current_num_threads().to_string()),
        ("Czas obliczeń", format!("{:.2} s", dt.as_secs_f64())),
        ("Wariantów × skali", variants.len().to_string()),
        (
            "Klimat wiatrowy",
            format!(
                "Weibull k={:.2}, średnia {:.1} m/s na {:.0} m, ścinanie α={:.2}",
                cfg.climate.k, cfg.climate.mean_v, cfg.climate.ref_height, cfg.climate.shear
            ),
        ),
        ("Jakość siatki", format!("{:?}", cfg.grid)),
    ];

    if let Err(e) = report::write_summary_csv(&out.join("summary.csv"), &variants) {
        eprintln!("CSV: {e}");
    }
    if let Err(e) = report::write_curves_csv(&out.join("power_curves.csv"), &variants) {
        eprintln!("CSV curves: {e}");
    }
    if let Err(e) = report::write_cp_csv(&out.join("cp_curves.csv"), &variants) {
        eprintln!("CSV cp: {e}");
    }
    if let Err(e) = report::write_json(&out.join("summary.json"), &variants, &meta) {
        eprintln!("JSON: {e}");
    }
    if let Err(e) = report::write_html(&out.join("report.html"), &variants, all, &meta) {
        eprintln!("HTML: {e}");
    }
    println!("\n[wiatrowe] wyniki zapisane do {:?}", out);

    // Krótkie podsumowanie "co wygrywa".
    if let Some(best) = variants.first() {
        println!(
            "\nNajniższe LCOE: {} ({:.0} kW, {:.0} PLN/MWh, skala {:.1} {})",
            best.name, best.rated_kw, best.econ.lcoe, best.scale, best.scale_kind
        );
    }
    let best_cp = variants
        .iter()
        .max_by(|a, b| a.cp_max.partial_cmp(&b.cp_max).unwrap());
    if let Some(b) = best_cp {
        println!(
            "Najwyższe Cp: {} (Cp={:.3}, λ={:.2})",
            b.name, b.cp_max, b.lambda_opt
        );
    }
    let best_mat = variants
        .iter()
        .max_by(|a, b| a.econ.kwh_per_kg.partial_cmp(&b.econ.kwh_per_kg).unwrap());
    if let Some(b) = best_mat {
        println!(
            "Najwięcej energii z kg materiału: {} ({:.1} kWh/kg·rok)",
            b.name, b.econ.kwh_per_kg
        );
    }
}

fn bench(all: &[wiatrowe::designs::Design], cfg: &SweepCfg, repeat: usize) {
    let hw = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2);
    let mut counts: Vec<usize> = vec![1, 2];
    let mut n = 4;
    while n <= hw.max(4) {
        counts.push(n);
        n *= 2;
    }
    counts.sort_unstable();
    counts.dedup();

    println!("\n[wiatrowe] benchmark rayon - {} wątków sprzętowych", hw);
    println!(
        "{:>9} {:>12} {:>10} {:>12} {:>14}",
        "wątki", "czas [s]", "przysp.", "efektywność", "ewaluacji"
    );

    let mut base = 0.0;
    let evals = all
        .iter()
        .map(|d| d.scales.len())
        .sum::<usize>()
        * (cfg.lambda_steps + 1)
        * repeat;

    for c in counts {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(c)
            .build()
            .expect("pula");
        let t0 = Instant::now();
        let _ = pool.install(|| sweep::workload(all, cfg, repeat));
        let dt = t0.elapsed().as_secs_f64();
        if base == 0.0 {
            base = dt;
        }
        println!(
            "{:>9} {:>12.4} {:>10.2} {:>11.0}% {:>14}",
            c,
            dt,
            base / dt,
            100.0 * (base / dt) / c as f64,
            evals
        );
        println!(
            "BENCH_RESULT lang=rust threads={} time_s={:.4} evals={} evals_per_s={:.1}",
            c,
            dt,
            evals,
            evals as f64 / dt
        );
    }
    println!(
        "\nRazem wykonano ok. {} ewaluacji modelu aerodynamicznego (BEMT/DMST/empiryczne).",
        evals
    );
    println!("Ten sam workload w Pythonie: `python3 tools/reference_sweep.py --repeat {}` (porównanie w CI).", repeat);
}

fn checksum(all: &[wiatrowe::designs::Design], cfg: &SweepCfg, out: &std::path::Path) {
    let lambdas = [1.0, 2.0, 3.0, 4.0, 5.5, 7.0, 9.0, 12.0];
    let mut rows = Vec::new();
    for d in all {
        for &scale in d.scales.iter().take(2) {
            for &l in lambdas.iter() {
                let mut aero = d.instantiate(scale);
                aero.set_design_lambda(l);
                let lam = if d.ctrl.lambda_hi > 0.0 {
                    l.clamp(d.ctrl.lambda_lo, d.ctrl.lambda_hi)
                } else {
                    0.0
                };
                let o = aero.evaluate(8.0, lam, &cfg.grid, cfg.rho);
                rows.push((d.id.to_string(), scale, lam, o.cp));
            }
        }
    }
    if let Err(e) = report::write_checksums(&out.join("checksums.csv"), &rows) {
        eprintln!("checksum: {e}");
    }
    let sum: f64 = rows.iter().map(|r| r.3).filter(|c| c.is_finite()).sum();
    println!(
        "[wiatrowe] checksum: {} punktów, suma Cp = {:.9}",
        rows.len(),
        sum
    );
}
