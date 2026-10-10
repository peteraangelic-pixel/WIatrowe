//! Blade Element Momentum Theory (BEMT) dla wirników o poziomej osi (HAWT),
//! wirników w dyfuzorze (DAWT) oraz wirników Magnus (cylindry jako "łopaty").
//!
//! Sformułowanie wg Burton et al., "Wind Energy Handbook":
//!   * strata końcówki łopaty (Prandtl) oraz piasty,
//!   * poprawka Glauerta/Buhla dla dużej indukcji osiowej (a > 0.4),
//!   * relaksacja 0.3, ~80 iteracji na element.
//!
//! Wielkości są bezwymiarowe względem *lokalnej* prędkości napływu V i promienia R,
//! dzięki czemu ten sam solver można użyć w dyfuzorze (V = a_v * V_inf).

use crate::airfoil::Polar;
use crate::{clamp, DEG2RAD};

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
pub fn solve(rotor: &HawtRotor, cfg: &BemtCfg) -> BemtOut {
    let magnus = is_magnus(&rotor.polar);
    let n = cfg.n_stations.max(8);
    let x0 = rotor.hub_frac.max(0.02);
    let dx = (1.0 - x0) / n as f64;
    let lambda = cfg.lambda.max(1e-3);
    let b = rotor.n_blades as f64;

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

        let mut a: f64 = 0.30;
        let mut ap: f64 = 0.01;
        let mut phi = 0.2;
        let mut ok = false;

        for _it in 0..90 {
            // Kąt napływu z uwzględnieniem indukcji osiowej i obwodowej.
            let phi_new = (1.0 - a).atan2(lambda_r * (1.0 + ap));
            let alpha_deg = phi_new * crate::RAD2DEG - rotor.twist_rad(x) * crate::RAD2DEG
                - cfg.pitch_deg;

            // Dla Magnusa: k = omega_spin*r_cyl / W; z definicji k zadajemy,
            // więc nie trzeba liczyć W (moc obrotu cylindrów policzymy niżej).
            let cl = rotor.polar.cl(alpha_deg, cfg.spin_ratio);
            let cd = rotor.polar.cd(alpha_deg, cfg.spin_ratio);

            let sin_p = phi_new.sin().max(1e-4);
            let cos_p = phi_new.cos();
            let cn = cl * cos_p + cd * sin_p;
            let ctan = cl * sin_p - cd * cos_p;

            // Straty końcówki (Prandtl) i piasty.
            let f_tip = if b < 100.0 {
                let arg = -(b * (1.0 - x) / (2.0 * x * sin_p)).exp().min(1.0);
                (2.0 / std::f64::consts::PI) * arg.acos()
            } else {
                1.0
            };
            let f_hub = if rotor.hub_frac > 0.01 {
                let arg = -(b * (x - rotor.hub_frac) / (2.0 * rotor.hub_frac * sin_p))
                    .exp()
                    .min(1.0);
                (2.0 / std::f64::consts::PI) * arg.acos()
            } else {
                1.0
            };
            let f = (f_tip * f_hub).clamp(0.05, 1.0);

            // Indukcja osiowa z poprawką Glauerta dla a > a_c.
            let ct_local = sigma * cn * (1.0 - a) * (1.0 - a) / (sin_p * sin_p);
            let a_new = if ct_local <= 0.96 {
                0.5 * (1.0 - (1.0 - ct_local.max(0.0)).max(0.0).sqrt())
            } else {
                let ac = 0.4;
                (ct_local / 4.0 - ac * ac) / (1.0 - 2.0 * ac)
            };
            let a_new = clamp(a_new, -0.6, 0.92);

            let denom = 4.0 * f * sin_p * cos_p;
            let ap_new = if denom.abs() > 1e-9 && ctan.abs() > 1e-12 {
                let d = denom / (sigma * ctan);
                if d > 1.0 {
                    1.0 / (d - 1.0)
                } else {
                    // Element hamujący / odwrócony znak - ograniczamy.
                    -0.5
                }
            } else {
                0.0
            };
            let ap_new = clamp(ap_new, -0.5, 3.0);

            let da = (a_new - a).abs() + (ap_new - ap).abs();
            // Relaksacja (0.3) - standard dla stabilności BEM.
            a += 0.3 * (a_new - a);
            ap += 0.3 * (ap_new - ap);
            phi = phi_new;

            if da < 1e-7 {
                ok = true;
                break;
            }
        }
        if !ok || a > 0.5 {
            // a > 0.5 oznacza wyjście poza zakres ważności teorii pędu
            // (stan "turbulent wake") - wynik tego elementu jest niewiarygodny.
            converged = false;
        }

        let sin_p = phi.sin().max(1e-4);
        let cos_p = phi.cos();
        let f_tip = if b < 100.0 {
            let arg = -(b * (1.0 - x) / (2.0 * x * sin_p)).exp().min(1.0);
            (2.0 / std::f64::consts::PI) * arg.acos()
        } else {
            1.0
        };
        let f_hub = if rotor.hub_frac > 0.01 {
            let arg = -(b * (x - rotor.hub_frac) / (2.0 * rotor.hub_frac * sin_p))
                .exp()
                .min(1.0);
            (2.0 / std::f64::consts::PI) * arg.acos()
        } else {
            1.0
        };
        let f = (f_tip * f_hub).clamp(0.05, 1.0);

        // Prędkość względna elementu (w jednostkach V): W = (1-a)/sin(phi)
        let w = ((1.0 - a) / sin_p).abs();

        cp += 8.0 * lambda * lambda * x * x * x * ap * (1.0 - a) * f * dx;
        ct += 8.0 * a * (1.0 - a) * f * x * dx;
        cp_swirl += 8.0 * lambda * lambda * x * x * x * (1.0 - a) * ap * ap * f * dx;

        if magnus {
            // Moc potrzebna na utrzymanie obrotu cylindrów:
            // P = 0.5*rho*(k*W)^3 * (pi*d*dr) * cf  (tarcie powierzchniowe)
            // znormalizowana do 0.5*rho*V^3*pi*R^2:
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
    n_stations: usize,
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
