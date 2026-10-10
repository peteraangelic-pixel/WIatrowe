//! Double-Multiple Stream Tube (DMST) dla wirników pionowych typu Darrieusa.
//!
//! Sformułowanie wg Paraschivoiu (równowagowy ślad):
//!   * wirnik dzielony jest na pionowe "rury prądu" (streamtubes) o szerokości dy,
//!   * każda rura zawiera dwa dyski aktuacyjne: nawietrzny i zawietrzny,
//!   * prędkość w śladzie między nimi: V_e = V_inf * (1 - 2 a_u),
//!   * moment całkowany jest po azymucie z lokalną prędkością indukowaną.
//!
//! Dodane straty pomijane w klasycznym DMST:
//!   * opór indukowany łopaty skończonej: cd_i = cl^2 / (pi * AR * e),
//!   * opór obrotowy wsporników/ramion (P ~ Omega^3),
//!   * współczynnik "helical"/"troposkien" (kształt łopaty).

use crate::airfoil::Polar;
use crate::{clamp, RAD2DEG};

/// Wirnik Darrieusa (H-rotor, troposkien, helikalny).
#[derive(Clone, Debug)]
pub struct DarrieusRotor {
    /// Promień wirnika [m].
    pub radius: f64,
    /// Wysokość łopat [m].
    pub height: f64,
    /// Liczba łopat.
    pub n_blades: usize,
    /// Cieciwa łopaty [m].
    pub chord: f64,
    /// Biegunowa profilu (zwykle symetryczny NACA 0018).
    pub polar: Polar,
    /// Cieciwa wspornika/ramienia [m].
    pub strut_chord: f64,
    /// Oporność wspornika.
    pub strut_cd: f64,
    /// Liczba wsporników (ramion).
    pub n_struts: usize,
    /// Wydłużenie efektywne do oporu indukowanego (AR = H/c).
    pub oswald: f64,
    /// Łopaty helikalne (skręcone): lepszy rozkład obciążeń, niższy Cp szczytowy.
    pub helical: bool,
    /// Łopaty typu troposkien ("ubijaczka do jajek").
    pub troposkien: bool,
}

impl DarrieusRotor {
    /// Wydłużenie łopaty AR = H / c.
    pub fn aspect_ratio(&self) -> f64 {
        if self.chord > 0.0 {
            self.height / self.chord
        } else {
            10.0
        }
    }

    /// Pole zakreślane (2R * H) [m^2].
    pub fn swept_area(&self) -> f64 {
        2.0 * self.radius * self.height
    }

    /// Powierzchnia łopat [m^2].
    pub fn blade_area(&self) -> f64 {
        self.chord * self.height * self.n_blades as f64
    }

    /// Wypełnienie sigma = N*c/R.
    pub fn solidity(&self) -> f64 {
        self.n_blades as f64 * self.chord / self.radius
    }
}

/// Wynik DMST.
#[derive(Clone, Copy, Debug, Default)]
pub struct DmstOut {
    pub cp: f64,
    pub ct: f64,
    pub converged: bool,
}

/// Siły elementu łopaty w bazie (normalna n, styczna t) w jednostkach
/// 0.5*rho*c*H*W^2. Zwraca (Cn, Ct, Cx, w).
#[inline]
fn blade_forces(polar: &Polar, theta: f64, x: f64, ar: f64, oswald: f64) -> (f64, f64, f64, f64) {
    let s = theta.sin();
    let c = theta.cos();
    let xs = x + s;
    let w = (xs * xs + c * c).sqrt().max(1e-6);
    let alpha_deg = c.atan2(xs) * RAD2DEG;
    let cl = polar.cl(alpha_deg, 0.0);
    let cd = polar.cd(alpha_deg, 0.0) + cl * cl / (std::f64::consts::PI * ar.max(1.0) * oswald);
    // Jednostkowe wektory: w = (c, -xs)/w, lift = (xs, c)/w
    let cn = (cl * xs + cd * c) / w;
    let ctan = (cl * c - cd * xs) / w;
    let cx = c * cn - s * ctan;
    (cn, ctan, cx, w)
}

/// Główny solver DMST. `lambda = Omega*R/V_inf`.
pub fn solve(rotor: &DarrieusRotor, lambda: f64, n_tubes: usize, n_az: usize) -> DmstOut {
    let r = rotor;
    let ar = r.aspect_ratio();
    let dy = 2.0 * r.radius / n_tubes as f64;
    let k = r.n_blades as f64 * r.chord / (8.0 * std::f64::consts::PI * r.radius);
    let mut au = vec![0.08f64; n_tubes];
    let mut ad = vec![0.08f64; n_tubes];
    let mut converged = true;

    for _it in 0..70 {
        let mut max_delta = 0.0f64;
        let mut new_au = Vec::with_capacity(n_tubes);
        let mut new_ad = Vec::with_capacity(n_tubes);
        for j in 0..n_tubes {
            let y = -r.radius + (j as f64 + 0.5) * dy;
            let s = clamp(y / r.radius, -0.999, 0.999);
            let th_u = std::f64::consts::PI - s.asin(); // nawietrzna (cos < 0)
            let th_d = s.asin(); // zawietrzna (cos > 0)
            let cu = th_u.cos().abs().max(0.15);
            let cdz = th_d.cos().abs().max(0.15);

            // --- dysk nawietrzny
            let vu = 1.0 - au[j];
            let x_u = lambda / vu.max(0.05);
            let (_, _, cx_u, w_u) = blade_forces(&r.polar, th_u, x_u, ar, r.oswald);
            let t_u = k * w_u * w_u * cx_u / cu;
            let au_new = clamp(t_u / (1.0 + t_u), -0.4, 0.6);

            // --- dysk zawietrzny (napyw Ve = 1 - 2 au)
            let ve = (1.0 - 2.0 * au[j]).max(0.05);
            let vd = ve * (1.0 - ad[j]);
            let x_d = lambda / vd.max(0.05);
            let (_, _, cx_d, w_d) = blade_forces(&r.polar, th_d, x_d, ar, r.oswald);
            let t_d = k * w_d * w_d * cx_d / cdz;
            let ad_new = clamp(t_d / (1.0 + t_d), -0.4, 0.6);

            max_delta = max_delta.max((au_new - au[j]).abs() + (ad_new - ad[j]).abs());
            new_au.push(au[j] + 0.35 * (au_new - au[j]));
            new_ad.push(ad[j] + 0.35 * (ad_new - ad[j]));
        }
        au = new_au;
        ad = new_ad;
        if max_delta < 1e-7 {
            break;
        }
        if _it == 69 {
            converged = false;
        }
    }

    // --- całka momentu po azymucie
    let dth = 2.0 * std::f64::consts::PI / n_az as f64;
    let mut torque_int = 0.0;
    for i in 0..n_az {
        let th = (i as f64 + 0.5) * dth;
        let s = th.sin();
        let j = (((s * r.radius + r.radius) / dy) as isize).clamp(0, n_tubes as isize - 1) as usize;
        let vloc = if th.cos() < 0.0 {
            1.0 - au[j]
        } else {
            ((1.0 - 2.0 * au[j]).max(0.05)) * (1.0 - ad[j])
        };
        let vloc = vloc.max(0.02);
        let x = lambda / vloc;
        let (_, ctan, _, w) = blade_forces(&r.polar, th, x, ar, r.oswald);
        torque_int += vloc * vloc * w * w * ctan * dth;
    }

    let pref = r.n_blades as f64 * r.chord / (4.0 * std::f64::consts::PI * r.radius);
    let mut cp = pref * lambda * torque_int;

    // Wsporniki (ramiona) - opór obrotowy ~ Omega^3.
    if r.n_struts > 0 && r.strut_chord > 0.0 {
        cp -= r.n_struts as f64
            * r.strut_cd
            * r.strut_chord
            * lambda.powi(3)
            / (8.0 * r.height);
    }
    // Kształt łopat: helikalne tracą trochę szczytowego Cp (skos),
    // ale mają znacznie lepszy rozkład momentu (uwzględnione w koszcie/żywotności).
    if r.helical {
        cp *= 0.95;
    }
    if r.troposkien {
        cp *= 0.96;
    }

    // Siła nacisku (do fundowania) z obu dysków.
    let mut ct = 0.0;
    for j in 0..n_tubes {
        let q = au[j] * (1.0 - au[j])
            + (1.0 - 2.0 * au[j]).max(0.0).powi(2) * ad[j] * (1.0 - ad[j]);
        ct += 2.0 * (dy / r.radius) * q;
    }

    DmstOut {
        cp: cp.max(-1.0),
        ct,
        converged,
    }
}

/// Skan lambdy z dokładniejszym przeszukaniem (złoty podział).
pub fn best_lambda(
    rotor: &DarrieusRotor,
    lo: f64,
    hi: f64,
    steps: usize,
    n_tubes: usize,
    n_az: usize,
) -> (f64, f64) {
    let mut best_l = lo;
    let mut best_cp = f64::MIN;
    for i in 0..=steps {
        let l = lo + (hi - lo) * i as f64 / steps as f64;
        let o = solve(rotor, l, n_tubes, n_az);
        if o.cp.is_finite() && o.cp > best_cp {
            best_cp = o.cp;
            best_l = l;
        }
    }
    let step = (hi - lo) / steps as f64;
    let (mut a, mut b) = ((best_l - step).max(lo), (best_l + step).min(hi));
    let phi = 0.6180339887;
    let mut x1 = b - phi * (b - a);
    let mut x2 = a + phi * (b - a);
    let eval = |l: f64| solve(rotor, l, n_tubes, n_az).cp;
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

    /// Symetryczny profil klasy NACA 0018 (typowy dla Darrieusa).
    fn naca0018() -> Polar {
        Polar::Foil(Foil {
            cl0: 0.0,
            alpha0_deg: 0.0,
            cl_slope: 6.0,
            alpha_stall_deg: 13.0,
            stall_width_deg: 8.0,
            cd0: 0.010,
            k_cd: 0.015,
            cd_stall: 1.20,
        })
    }

    fn rotor() -> DarrieusRotor {
        DarrieusRotor {
            radius: 20.0,
            height: 30.0,
            n_blades: 3,
            chord: 1.2,
            polar: naca0018(),
            strut_chord: 0.25,
            strut_cd: 0.06,
            n_struts: 6,
            oswald: 0.9,
            helical: false,
            troposkien: false,
        }
    }

    #[test]
    fn darrieus_peak_is_literature_like() {
        let r = rotor();
        let (l, cp) = best_lambda(&r, 1.0, 7.0, 40, 24, 180);
        // Literatura: Cp_max ~ 0.30-0.45 przy lambda ~ 3-5.
        assert!(cp > 0.28 && cp < 0.50, "Cp_max={cp} przy lam={l}");
        assert!(l > 2.5 && l < 5.5, "lambda_opt={l}");
    }

    #[test]
    fn darrieus_does_not_self_start() {
        let r = rotor();
        let o = solve(&r, 1.2, 24, 180);
        assert!(o.cp < 0.05, "przy niskim lambda Cp powinno byc marne: {}", o.cp);
    }

    #[test]
    fn higher_solidity_lowers_optimal_tsr() {
        let mut small = rotor();
        small.chord = 0.8;
        let mut big = rotor();
        big.chord = 1.8;
        let (l1, _) = best_lambda(&small, 2.0, 8.0, 30, 16, 120);
        let (l2, _) = best_lambda(&big, 1.5, 6.0, 30, 16, 120);
        assert!(l1 > l2, "{l1} vs {l2}");
    }
}
