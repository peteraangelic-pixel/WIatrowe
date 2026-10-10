//! Przeliczenie całego katalogu: optymalne sterowanie, krzywa mocy, AEP, koszt.
//!
//! Wszystko jest zrównoleglone rayonem:
//!   * etap 1 - płaska siatka (wariant x skala x lambda [x skręcenie cylindrów]),
//!   * etap 2 - płaska siatka (wariant x skala x prędkość wiatru),
//!   * redukcje (`fold`/`reduce`) oraz sortowanie wyników (`par_sort_unstable_by`).

use rayon::prelude::*;

use crate::climate::Climate;
use crate::cost::{self, Economics};
use crate::designs::{Aero, Ctrl, Design, Grid};
use crate::{clamp, RHO};

/// Sprawność układu przeniesienia + generatora + przetwornicy (mechaniczne -> sieć).
pub fn drivetrain_eff(design_id: &str, family: crate::designs::Family) -> f64 {
    use crate::designs::Family::*;
    if design_id == "farm18" {
        return 0.86; // prosta przekładnia mechaniczna, bez elektroniki mocy
    }
    match family {
        Hawt => 0.93,
        Ducted => 0.92,
        Venturi => 0.90,
        VawtLift => 0.90,
        VawtDrag => 0.88,
        Magnus => 0.88,
        Bladeless => 0.90,
        Airborne => 0.90,
        Belt => 0.80,
    }
}

/// Wynik jednego wariantu (konkretna skala).
#[derive(Clone, Debug)]
pub struct Variant {
    pub id: String,
    pub name: String,
    pub family: String,
    pub scale: f64,
    pub scale_kind: String,
    pub area: f64,
    pub hub: f64,
    pub cp_max: f64,
    pub lambda_opt: f64,
    pub spin_opt: f64,
    pub rated_kw: f64,
    pub peak_kw: f64,
    pub thrust_n: f64,
    pub aep_kwh: f64,
    pub capacity_factor: f64,
    pub curve: Vec<(f64, f64)>,
    pub cp_curve: Vec<(f64, f64)>,
    pub econ: Economics,
    pub note: String,
    pub confidence: String,
}

/// Konfiguracja przeliczenia.
#[derive(Clone, Copy, Debug)]
pub struct SweepCfg {
    pub grid: Grid,
    /// Liczba punktów w przeszukiwaniu lambda.
    pub lambda_steps: usize,
    /// Liczba punktów w przeszukiwaniu współczynnika skręcenia (Magnus).
    pub spin_steps: usize,
    /// Krok siatki prędkości [m/s].
    pub dv: f64,
    /// Maksymalna prędkość w krzywej mocy [m/s].
    pub v_max: f64,
    pub climate: Climate,
    pub rho: f64,
    /// Prędkość referencyjna do poszukiwania optymalnego lambda [m/s].
    pub v_ref: f64,
}

impl SweepCfg {
    pub fn new(quality: Grid) -> Self {
        SweepCfg {
            grid: quality,
            lambda_steps: 48,
            spin_steps: 10,
            dv: 0.25,
            v_max: 35.0,
            climate: Climate::poland(),
            rho: RHO,
            v_ref: 8.0,
        }
    }
}

#[derive(Clone, Copy)]
struct Job {
    di: usize,
    si: usize,
    lambda: f64,
    spin: f64,
}

/// Czy wariant ma w ogóle sterowanie lambdą.
fn uses_lambda(id: &str) -> bool {
    !matches!(id, "bladeless" | "kite")
}

fn lambda_grid(d: &Design) -> (f64, f64) {
    (d.ctrl.lambda_lo, d.ctrl.lambda_hi)
}

/// Krok indeksowania płaskiej tablicy wyników (wariant x skala).
fn stride(designs: &[Design]) -> usize {
    designs.iter().map(|d| d.scales.len()).max().unwrap_or(1)
}

/// Etap 1: optymalny punkt sterowania (lambda*, k*) dla każdego wariantu i skali.
fn optimise_control(designs: &[Design], cfg: &SweepCfg) -> Vec<(Option<f64>, Option<f64>, f64)> {
    let mut jobs = Vec::new();
    for (di, d) in designs.iter().enumerate() {
        for si in 0..d.scales.len() {
            if !uses_lambda(d.id) {
                continue;
            }
            let (lo, hi) = lambda_grid(d);
            let ns = if d.ctrl.spin_hi > d.ctrl.spin_lo {
                cfg.spin_steps
            } else {
                1
            };
            for i in 0..=cfg.lambda_steps {
                let l = lo + (hi - lo) * i as f64 / cfg.lambda_steps as f64;
                for j in 0..ns {
                    let spin = if ns == 1 {
                        d.ctrl.spin_lo
                    } else {
                        d.ctrl.spin_lo
                            + (d.ctrl.spin_hi - d.ctrl.spin_lo) * j as f64 / (ns - 1) as f64
                    };
                    jobs.push(Job {
                        di,
                        si,
                        lambda: l,
                        spin,
                    });
                }
            }
        }
    }

    let raw: Vec<(usize, usize, f64, f64, f64)> = jobs
        .par_iter()
        .map(|j| {
            let d = &designs[j.di];
            let scale = d.scales[j.si];
            let mut aero = d.instantiate(scale);
            aero.set_design_lambda(j.lambda);
            if let Aero::Hawt(ref mut s) = aero {
                if s.magnus {
                    s.spin_ratio = j.spin;
                }
            }
            let o = aero.evaluate(cfg.v_ref, j.lambda, &cfg.grid, cfg.rho);
            let val = if o.converged {
                o.cp
            } else {
                o.cp - 1.0 // kara za niezbieżność / wyjście poza zakres momentum
            };
            (j.di, j.si, j.lambda, j.spin, val)
        })
        .collect();

    let stride = stride(designs);
    let mut best: Vec<(Option<f64>, Option<f64>, f64)> =
        vec![(None, None, f64::MIN); designs.len() * stride];
    for (di, si, l, spin, cp) in raw {
        let idx = di * stride + si;
        if cp.is_finite() && cp > best[idx].2 {
            best[idx] = (Some(l), Some(spin), cp);
        }
    }
    best
}

/// Krzywa mocy: (v, P_elektr [W]) na siatce prędkości.
fn power_curve(
    d: &Design,
    aero: &Aero,
    lambda_opt: Option<f64>,
    cfg: &SweepCfg,
) -> Vec<(f64, f64)> {
    let n = (cfg.v_max / cfg.dv) as usize + 1;
    let ctrl: &Ctrl = &d.ctrl;
    let eta = drivetrain_eff(d.id, d.family);
    let out: Vec<(f64, f64)> = (0..n)
        .into_par_iter()
        .map(|i| {
            let v = (i as f64 + 0.5) * cfg.dv;
            if v < ctrl.v_in || v > ctrl.v_out {
                return (v, 0.0);
            }
            let lam = match lambda_opt {
                Some(l) => {
                    if ctrl.tip_speed_max > 0.0 {
                        l.min(ctrl.tip_speed_max / v)
                    } else {
                        l
                    }
                }
                None => 0.0,
            };
            let o = aero.evaluate(v, lam, &cfg.grid, cfg.rho);
            let p = (o.power * eta).max(0.0);
            (v, p)
        })
        .collect();
    out
}

fn interpolate(curve: &[(f64, f64)], v: f64) -> f64 {
    if curve.is_empty() || v <= curve[0].0 || v >= curve[curve.len() - 1].0 {
        return 0.0;
    }
    let idx = ((v - curve[0].0) / (curve[1].0 - curve[0].0)) as usize;
    let i = idx.min(curve.len() - 2);
    let (v0, p0) = curve[i];
    let (v1, p1) = curve[i + 1];
    let t = (v - v0) / (v1 - v0);
    p0 + (p1 - p0) * t
}

/// Zastosowanie ograniczenia mocy znamionowej.
///
/// Maszyny z regulacją kąta łopat trzymają moc znamionową powyżej V_nom.
/// Maszyny bez regulacji (Savonius, wiatrak farmerski, latawiec, windbelt)
/// w rzeczywistości są chronione przed przeciążeniem odchyleniem z wiatru,
/// przeciągnięciem albo hamulcem generatora - modelujemy to jako ograniczenie
/// mocy do 1.20 * P(V_nom) (typowy przester maszyny stall-regulated).
fn apply_rating(d: &Design, curve: &mut [(f64, f64)]) -> (f64, f64) {
    let p_rated = interpolate(curve, d.ctrl.v_rated);
    let cap = if d.ctrl.pitch_control {
        p_rated
    } else {
        p_rated * 1.20
    };
    for (_, p) in curve.iter_mut() {
        if *p > cap {
            *p = cap;
        }
    }
    let peak = curve
        .iter()
        .filter(|(v, _)| *v <= d.ctrl.v_out)
        .map(|(_, p)| *p)
        .fold(0.0, f64::max);
    (p_rated.max(0.0), peak.max(p_rated))
}

/// Główne przeliczenie całego katalogu.
pub fn run(designs: &[Design], cfg: &SweepCfg) -> Vec<Variant> {
    let ctrl = optimise_control(designs, cfg);
    let stride = stride(designs);

    // Etap 2: krzywe mocy (równolegle po wszystkich wariantach i skalach).
    let mut jobs = Vec::new();
    for (di, d) in designs.iter().enumerate() {
        for (si, _) in d.scales.iter().enumerate() {
            jobs.push((di, si));
        }
    }

    let mut variants: Vec<Variant> = jobs
        .par_iter()
        .map(|&(di, si)| {
            let d = &designs[di];
            let scale = d.scales[si];
            let (lam, spin, cp_max) = ctrl[di * stride + si];
            let mut aero = d.instantiate(scale);
            if let Some(l) = lam {
                aero.set_design_lambda(l);
            }
            if let Aero::Hawt(ref mut s) = aero {
                if s.magnus {
                    if let Some(k) = spin {
                        s.spin_ratio = k;
                    }
                }
            }
            let mut curve = power_curve(d, &aero, lam, cfg);
            let (rated_w, peak_w) = apply_rating(d, &mut curve);

            // Nacisk w punkcie znamionowym (do wieży i fundamentu).
            let v_thrust = if d.ctrl.pitch_control {
                d.ctrl.v_rated
            } else {
                d.ctrl.v_out.min(20.0)
            };
            let lam_t = match lam {
                Some(l) => {
                    if d.ctrl.tip_speed_max > 0.0 {
                        l.min(d.ctrl.tip_speed_max / v_thrust.max(0.5))
                    } else {
                        l
                    }
                }
                None => 0.0,
            };
            let o_t = aero.evaluate(v_thrust, lam_t, &cfg.grid, cfg.rho);
            let thrust_n = o_t.thrust;

            // AEP: całka po rozkładzie Weibulla z uwzględnieniem ścinania.
            let hub = d.hub_height(scale).max(4.0);
            let f = cfg.climate.shear_factor(hub);
            let mut aep_w = 0.0;
            let n = (cfg.v_max / cfg.dv) as usize;
            for i in 0..n {
                let v_ref = (i as f64 + 0.5) * cfg.dv;
                let p = interpolate(&curve, v_ref * f);
                aep_w += p * cfg.climate.pdf(v_ref) * cfg.dv;
            }
            let aep_kwh = aep_w * 8760.0 / 1000.0;

            // Krzywa Cp(lambda) do raportu.
            let cp_curve: Vec<(f64, f64)> = if uses_lambda(d.id) {
                let (lo, hi) = lambda_grid(d);
                (0..=40)
                    .map(|i| {
                        let l = lo + (hi - lo) * i as f64 / 40.0;
                        let mut a = d.instantiate(scale);
                        a.set_design_lambda(l);
                        if let Aero::Hawt(ref mut s) = a {
                            if s.magnus {
                                s.spin_ratio = spin.unwrap_or(2.0);
                            }
                        }
                        (l, a.evaluate(cfg.v_ref, l, &cfg.grid, cfg.rho).cp)
                    })
                    .collect()
            } else {
                (0..=40)
                    .map(|i| {
                        let v = 1.0 + 28.0 * i as f64 / 40.0;
                        (v, aero.evaluate(v, 0.0, &cfg.grid, cfg.rho).cp)
                    })
                    .collect()
            };

            let geom = aero.geometry(hub, d.spacing_x, d.spacing_y);
            let geom = if d.family == crate::designs::Family::Airborne {
                let mut gg = geom;
                gg.footprint = match &aero {
                    Aero::Kite(k) => k.footprint(),
                    _ => gg.footprint,
                };
                gg
            } else {
                geom
            };
            let econ = cost::economics(d, &aero, &geom, rated_w, thrust_n, aep_kwh);

            // Dla maszyn bez lambda (latawiec, VIV) szczytowe Cp bierzemy
            // wprost z przebiegu Cp(v).
            let cp_peak = if uses_lambda(d.id) {
                cp_max
            } else {
                cp_curve.iter().map(|(_, c)| *c).fold(0.0, f64::max)
            };

            Variant {
                id: d.id.to_string(),
                name: d.name.to_string(),
                family: d.family.label().to_string(),
                scale,
                scale_kind: match d.scale_kind {
                    crate::designs::ScaleKind::Radius => "R [m]".to_string(),
                    crate::designs::ScaleKind::WingArea => "A_skrzydła [m²]".to_string(),
                },
                area: aero.area(),
                hub,
                cp_max: if cp_peak.is_finite() { cp_peak } else { 0.0 },
                lambda_opt: lam.unwrap_or(0.0),
                spin_opt: spin.unwrap_or(0.0),
                rated_kw: rated_w / 1000.0,
                peak_kw: peak_w / 1000.0,
                thrust_n,
                aep_kwh,
                capacity_factor: econ.capacity_factor,
                curve,
                cp_curve,
                econ,
                note: d.note.to_string(),
                confidence: d.confidence.to_string(),
            }
        })
        .collect();

    // Ranking: najpierw po LCOE (rosnąco), potem po AEP (malejąco).
    variants.par_sort_unstable_by(|a, b| {
        a.econ
            .lcoe
            .partial_cmp(&b.econ.lcoe)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                b.aep_kwh
                    .partial_cmp(&a.aep_kwh)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    variants
}

/// Przykładowa "ciężka" paczka robocza do benchmarku (ten sam kod co wyżej,
/// powtarzany `--repeat` razy), zwraca liczbę wykonanych ewaluacji.
pub fn workload(designs: &[Design], cfg: &SweepCfg, repeat: usize) -> usize {
    let jobs: Vec<Job> = designs
        .iter()
        .enumerate()
        .flat_map(|(di, d)| {
            let (lo, hi) = lambda_grid(d);
            (0..=cfg.lambda_steps).flat_map(move |i| {
                let l = lo + (hi - lo) * i as f64 / cfg.lambda_steps as f64;
                (0..d.scales.len()).map(move |si| Job {
                    di,
                    si,
                    lambda: l,
                    spin: 2.0,
                })
            })
        })
        .collect();
    let per = jobs.len();
    let total: usize = (0..repeat)
        .into_par_iter()
        .map(|_| {
            jobs.par_iter()
                .map(|j| {
                    let d = &designs[j.di];
                    let mut aero = d.instantiate(d.scales[j.si]);
                    aero.set_design_lambda(j.lambda);
                    let lam = if uses_lambda(d.id) { j.lambda } else { 0.0 };
                    let o = aero.evaluate(cfg.v_ref, lam, &cfg.grid, cfg.rho);
                    if o.cp.is_finite() {
                        1
                    } else {
                        0
                    }
                })
                .sum::<usize>()
        })
        .sum();
    let _ = total;
    per * repeat
}

/// Znormalizowany "score" do szybkiego rankingu (0-100).
pub fn score(v: &Variant) -> f64 {
    let lcoe = clamp(v.econ.lcoe, 50.0, 5000.0);
    let s_lcoe: f64 = 100.0 * (5000.0f64 / lcoe).ln() / (5000.0f64 / 50.0f64).ln();
    let cf = clamp(v.capacity_factor, 0.0, 0.9);
    let s_cf = 100.0 * cf / 0.9;
    let mat = clamp(v.econ.kwh_per_kg, 0.0, 400.0);
    let s_mat = 100.0 * mat / 400.0;
    0.5 * s_lcoe + 0.3 * s_cf + 0.2 * s_mat
}
