//! Blade Element Momentum Theory (BEMT) dla wirników o poziomej osi (HAWT),
//! wirników w dyfuzorze (DAWT) oraz wirników Magnus (cylindry jako "łopaty").
//!
//! Sformułowanie wg Burton et al., "Wind Energy Handbook":
//!   * strata końcówki łopaty (Prandtl) oraz piasty,
//!   * poprawka Glauerta dla dużej indukcji osiowej (CT > 0.96),
//!   * równanie równowagi rozwiązywane w kącie napływu (bisekcja).
//!
//! Wielkości są bezwymiarowe względem *lokalnej* prędkości napływu V i promienia R,
//! dzięki czemu ten sam solver można użyć w dyfuzorze (V = a_v * V_inf).

use crate::airfoil::Polar;
use crate::{clamp, DEG2RAD, RAD2DEG};

/// Geometria wirnika o poziomej osi (lub "łopatowego" wirnika Magnusa).
#[derive(Clone, Debug)]
pub struct HawtRotor {
    /// Promień wirnika [m].
    pub radius: f64,
    /// Promień piasty jako ułamek R.
    pub hub_frac: f64,
    /// Liczba łopat.
    pub n_blades: usize,
    /// Cieciwa przy x = 0.2 jako ułamek R.
    pub chord_root_frac: f64,
    /// Cieciwa na końcówce (x = 1) jako ułamek R.
    pub chord_tip_frac: f64,
    /// Prędkość obrotowa projektowa (do skręcenia łopaty).
    pub lambda_design: f64,
    /// Projektowy kąt natarcia [deg].
    pub alpha_design_deg: f64,
    /// Biegunowa profilu.
    pub polar: Polar,
    /// Grubość względna łopaty (do oszacowania masy) - ułamek cięciwy.
    pub thickness_frac: f64,
}

impl HawtRotor {
    /// Cieciwa w zadanym promieniu względnym x = r/R.
    pub fn chord(&self, x: f64) -> f64 {
        let c_root = self.chord_root_frac * self.radius;
        let c_tip = self.chord_tip_frac * self.radius;
        if x <= 0.2 {
            c_root
        } else {
            c_root + (c_tip - c_root) * (x - 0.2) / 0.8
        }
    }

    /// Skręcenie łopaty [rad] - idealne skręcenie Betza dla lambda_design,
    /// ograniczone przy piaście (jak w rzeczywistych łopatach).
    pub fn twist_rad(&self, x: f64) -> f64 {
        let xr = x.max(0.15);
        let phi = (2.0 / (3.0 * self.lambda_design * xr)).atan();
        let twist = phi - self.alpha_design_deg * DEG2RAD;
        clamp(twist, 0.0, 32.0 * DEG2RAD)
    }

    /// Powierzchnia jednej łopaty (planform) [m^2].
    pub fn blade_area(&self) -> f64 {
        let n = 48;
        let x0 = self.hub_frac;
        let dx = (1.0 - x0) / n as f64;
        let mut s = 0.0;
        for i in 0..n {
            let x = x0 + (i as f64 + 0.5) * dx;
            s += self.chord(x) * dx * self.radius;
        }
        s
    }

    /// Powierzchnia wszystkich łopat [m^2].
    pub fn total_blade_area(&self) -> f64 {
        self.blade_area() * self.n_blades as f64
    }

    /// Wypełnienie (solidity) = powierzchnia łopat / pole zakreślane.
    pub fn solidity(&self) -> f64 {
        self.total_blade_area() / (std::f64::consts::PI * self.radius * self.radius)
    }
}

/// Wynik BEMT (bezwymiarowy, względem lokalnej prędkości napływu).
#[derive(Clone, Copy, Debug, Default)]
pub struct BemtOut {
    /// Współczynnik mocy na wale (aerodynamiczny, bez strat przekładni/generatora).
    pub cp: f64,
    /// Współczynnik siły ciągu (nacisku osiowego).
    pub ct: f64,
    /// Strata energii w ruchu wirowym śladu (do wykorzystania przez wirnik przeciwbieżny).
    pub cp_swirl: f64,
    /// Moc pochłonięta przez napęd obrotu cylindrów Magnusa (jako ułamek Cp).
    pub cp_spin: f64,
    /// Czy wszystkie elementy zbiegły.
    pub converged: bool,
}

/// Konfiguracja punktu pracy BEMT.
#[derive(Clone, Copy, Debug)]
pub struct BemtCfg {
    /// Współczynnik prędkości obrotowej lambda = Omega*R/V (V = prędkość lokalna).
    pub lambda: f64,
    /// Kąt nastawienia łopat (pitch) [deg] - dodatni = "w flagę" (feather).
    pub pitch_deg: f64,
    /// Liczba elementów wzdłuż promienia.
    pub n_stations: usize,
    /// Współczynnik skręcenia cylindrów Magnusa k = omega_spin*r_cyl/W (0 dla profili).
    pub spin_ratio: f64,
}

impl BemtCfg {
    pub fn new(lambda: f64, n_stations: usize) -> Self {
        Self {
            lambda,
            pitch_deg: 0.0,
            n_stations,
            spin_ratio: 0.0,
        }
    }
}

fn is_magnus(p: &Polar) -> bool {
    matches!(p, Polar::Magnus(_))
}

/// Główny solver BEMT.
/// Czynnik straty końcówki / piasty (Prandtla).
#[inline]
fn prandtl(arg: f64) -> f64 {
    let t = (-arg.max(0.0)).exp().min(1.0);
    (2.0 / std::f64::consts::PI) * t.acos()
}

/// Równanie równowagi elementu łopaty rozwiązywane w kącie napływu.
///
/// Szukamy `phi`, dla którego trójkąt prędkości i teoria pędu dają ten sam
/// stan elementu:
///
/// ```text
/// (1 - a) * cos(phi) - lambda_r * (1 + ap) * sin(phi) = 0
/// ```
///
/// przy czym `a` i `ap` dla danego `phi` wyznaczamy wprost z teorii pędu
/// (z poprawką Glauerta na stan turbulent wake). Klasyczna iteracja punktu
/// stałego (a, ap) nie zbiega przy dużym obciążeniu elementu (duża
/// solidność, niskie lambda, głęboki stall) - wtedy wynik zależy od kolejności
/// zaokrągleń i nie da się go powtórzyć w drugiej implementacji. Bisekcja po
/// `phi` jest deterministyczna, więc implementacja w Rust i w Pythonie dają
/// te same liczby (co sprawdzamy w CI).
pub fn solve(rotor: &HawtRotor, cfg: &BemtCfg) -> BemtOut {
    let magnus = is_magnus(&rotor.polar);
    let n = cfg.n_stations.max(8);
    let x0 = rotor.hub_frac.max(0.02);
    let dx = (1.0 - x0) / n as f64;
    let lambda = cfg.lambda.max(1e-3);
    let b = rotor.n_blades as f64;
    let pitch = cfg.pitch_deg * DEG2RAD;

    let mut cp = 0.0;
    let mut ct = 0.0;
    let mut cp_swirl = 0.0;
    let mut cp_spin = 0.0;
    let mut converged = true;

    for i in 0..n {
        let x = x0 + (i as f64 + 0.5) * dx;
        let lambda_r = lambda * x;
        let chord = rotor.chord(x);
        let sigma = b * chord / (2.0 * std::f64::consts::PI * x * rotor.radius);
        let twist = rotor.twist_rad(x);
        let spin = if magnus { cfg.spin_ratio } else { 0.0 };

        // Stan elementu (a, a', F) dla zadanego kąta napływu.
        let state = |phi: f64| -> (f64, f64, f64, f64) {
            let s = phi.sin().max(1e-6);
            let c = phi.cos();
            let alpha_deg = (phi - twist - pitch) * RAD2DEG;
            let cl = rotor.polar.cl(alpha_deg, spin);
            let cd = rotor.polar.cd(alpha_deg, spin);
            let cn = cl * c + cd * s;
            let ctan = cl * s - cd * c;
            let f_tip = prandtl(b * (1.0 - x) / (2.0 * x * s));
            let f_hub = if rotor.hub_frac > 0.01 {
                prandtl(b * (x - rotor.hub_frac) / (2.0 * rotor.hub_frac * s))
            } else {
                1.0
            };
            let f = clamp(f_tip * f_hub, 0.05, 1.0);

            // Relacja pędu: a/(1-a) = sigma*cn/(4*F*sin^2(phi))
            let den = 4.0 * f * s * s + sigma * cn;
            let mut a = if den.abs() > 1e-12 { sigma * cn / den } else { 0.0 };
            let ct_loc = 4.0 * a * (1.0 - a) * f;
            if ct_loc > 0.96 {
                // Poprawka Glauerta: stan "turbulent wake" (CT > 0.96).
                a = (ct_loc / 4.0 - 0.16) / 0.2;
            }
            let a = clamp(a, -0.6, 0.92);

            // Relacja pędu obwodowego: ap/(1+ap) = sigma*ctan/(4*F*sin*cos)
            let ap = if (sigma * ctan).abs() > 1e-12 && c.abs() > 1e-9 {
                let d = 4.0 * f * s * c / (sigma * ctan);
                if (d - 1.0).abs() > 1e-12 {
                    1.0 / (d - 1.0)
                } else {
                    3.0
                }
            } else {
                0.0
            };
            let ap = clamp(ap, -0.5, 3.0);
            (a, ap, f, s)
        };

        let residual = |phi: f64| -> f64 {
            let (a, ap, _f, s) = state(phi);
            (1.0 - a) * phi.cos() - lambda_r * (1.0 + ap) * s
        };

        let mut lo = 1e-4_f64;
        let mut hi = std::f64::consts::FRAC_PI_2 - 1e-4;
        let rlo = residual(lo);
        if (rlo > 0.0) == (residual(hi) > 0.0) {
            // Brak zmiany znaku: element nie oddaje mocy w tym punkcie pracy.
            converged = false;
        }
        for _ in 0..46 {
            let mid = 0.5 * (lo + hi);
            if (residual(mid) > 0.0) == (rlo > 0.0) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let phi = 0.5 * (lo + hi);
        let (a, ap, f, s) = state(phi);

        // Prędkość względna elementu (w jednostkach V): W = (1-a)/sin(phi)
        let w = ((1.0 - a) / s).abs();

        // Moc na wale z momentu elementu (poprawna przy niezerowym oporze
        // profilu - forma pędowa by ją przeszacowała o straty profilowe).
        cp += 8.0 * lambda * lambda * x * x * x * ap * (1.0 - a) * f * dx;
        ct += 8.0 * a * (1.0 - a) * f * x * dx;
        cp_swirl += 8.0 * lambda * lambda * x * x * x * (1.0 - a) * ap * ap * f * dx;

        if magnus {
            // Moc potrzebna na utrzymanie obrotu cylindrów:
            // P = 0.5*rho*(k*W)^3 * (pi*d*dr) * cf  (tarcie powierzchniowe)
            // znormalizowana do 0.5*rho*V^3*pi*R^2.
            let d_frac = chord / rotor.radius; // średnica cylindra jako ułamek R
            let surf = std::f64::consts::PI * d_frac * dx; // pi*d*dr / R^2
            let v_surf = cfg.spin_ratio * w; // prędkość powierzchni / V
            let cf = match rotor.polar {
                Polar::Magnus(m) => m.cf_spin,
                Polar::Foil(_) => 0.0,
            };
            cp_spin += v_surf * v_surf * v_surf * surf * cf * b;
        }
    }

    BemtOut {
        cp,
        ct,
        cp_swirl,
        cp_spin,
        converged,
    }
}

/// Skan lambdy: znajdź optimum Cp na zadanej siatce (z dokładniejszym
/// przeszukaniem wokół najlepszego punktu - golden section).
pub fn best_lambda(
    rotor: &HawtRotor,
    _n_stations: usize,
    lo: f64,
    hi: f64,
    steps: usize,
    extra: &BemtCfg,
) -> (f64, f64) {
    let mut best_l = lo;
    let mut best_cp = f64::MIN;
    for i in 0..=steps {
        let l = lo + (hi - lo) * i as f64 / steps as f64;
        let mut cfg = *extra;
        cfg.lambda = l;
        let out = solve(rotor, &cfg);
        let val = out.cp - out.cp_spin;
        if val.is_finite() && val > best_cp {
            best_cp = val;
            best_l = l;
        }
    }
    // Dopozycjonowanie złotym podziałem w sąsiedztwie najlepszego punktu siatki.
    let step = (hi - lo) / steps as f64;
    let (mut a, mut b) = ((best_l - step).max(lo), (best_l + step).min(hi));
    let phi = 0.6180339887;
    let mut x1 = b - phi * (b - a);
    let mut x2 = a + phi * (b - a);
    let eval = |l: f64| {
        let mut c = *extra;
        c.lambda = l;
        let o = solve(rotor, &c);
        o.cp - o.cp_spin
    };
    let mut f1 = eval(x1);
    let mut f2 = eval(x2);
    for _ in 0..24 {
        if f1 < f2 {
            a = x1;
            x1 = x2;
            f1 = f2;
            x2 = a + phi * (b - a);
            f2 = eval(x2);
        } else {
            b = x2;
            x2 = x1;
            f2 = f1;
            x1 = b - phi * (b - a);
            f1 = eval(x1);
        }
        if (b - a) < 1e-4 {
            break;
        }
    }
    let l_star = 0.5 * (x1 + x2);
    let cp_star = eval(l_star);
    if cp_star.is_finite() && cp_star > best_cp {
        return (l_star, cp_star);
    }
    (best_l, best_cp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airfoil::Foil;
    use crate::BETZ;

    fn base_rotor() -> HawtRotor {
        HawtRotor {
            radius: 50.0,
            hub_frac: 0.15,
            n_blades: 3,
            chord_root_frac: 0.075,
            chord_tip_frac: 0.018,
            lambda_design: 7.5,
            alpha_design_deg: 6.0,
            polar: Polar::Foil(Foil::modern()),
            thickness_frac: 0.18,
        }
    }

    #[test]
    fn cp_respects_betz() {
        let r = base_rotor();
        for l in [2.0, 4.0, 6.0, 7.5, 9.0, 12.0] {
            let o = solve(&r, &BemtCfg::new(l, 40));
            assert!(o.cp < BETZ + 1e-3, "Cp={} > Betz przy lambda={}", o.cp, l);
        }
    }

    #[test]
    fn cp_peak_is_realistic() {
        let r = base_rotor();
        let (l, cp) = best_lambda(&r, 40, 2.0, 12.0, 40, &BemtCfg::new(7.0, 40));
        // Dobrze zaprojektowany wirnik: 0.45-0.52, optimum lambda 6-9.
        assert!(cp > 0.40 && cp < 0.56, "Cp_max={cp} przy lambda={l}");
        assert!(l > 5.0 && l < 10.0, "lambda_opt={l}");
    }

    #[test]
    fn more_blades_shift_optimum_down() {
        let mut r2 = base_rotor();
        r2.n_blades = 2;
        let mut r6 = base_rotor();
        r6.n_blades = 6;
        let (l2, _) = best_lambda(&r2, 30, 2.0, 14.0, 30, &BemtCfg::new(7.0, 30));
        let (l6, _) = best_lambda(&r6, 30, 1.0, 8.0, 30, &BemtCfg::new(4.0, 30));
        assert!(l2 > l6, "l2={l2} powinno byc wieksze niz l6={l6}");
    }

    #[test]
    fn ct_in_physical_range() {
        let r = base_rotor();
        let o = solve(&r, &BemtCfg::new(7.5, 40));
        assert!(o.ct > 0.3 && o.ct < 1.1, "Ct={}", o.ct);
    }
}
