//! Modele biegunowych (polar) profili oraz "profilów" niekonwencjonalnych.
//!
//! Zamiast tabelarycznych danych (których nie mamy w repo) używamy dwóch
//! analitycznych modeli, skalibrowanych tak, by w zakresie roboczym dawały
//! wartości typowe dla profili klasy DU / NACA 63-4xx oraz dla wirujących
//! cylindrów (efekt Magnusa).
//!
//! * profil lotniczy: narastanie cl zgodnie z nachyleniem 2*PI*eta,
//!   łagodne przejście do modelu płaskiej płyty (Viterna) za kątem przeciągnięcia,
//! * cylinder Magnusa: cl zależy tylko od współczynnika skręcenia `k = ω r / V`,
//!   siła nośna jest prostopadła do prędkości względnej (stąd cl "bez alfą").

use crate::{clamp, smoothstep, DEG2RAD};

/// Uproszczona biegunowa profilu lotniczego.
#[derive(Clone, Copy, Debug)]
pub struct Foil {
    /// cl przy alfa = 0 (wygięcie profilu).
    pub cl0: f64,
    /// Kąt zerowej siły nośnej [deg].
    pub alpha0_deg: f64,
    /// Nachylenie krzywej cl [1/rad].
    pub cl_slope: f64,
    /// Kąt przeciągnięcia [deg].
    pub alpha_stall_deg: f64,
    /// Szerokość strefy przejścia do modelu płaskiej płyty [deg].
    pub stall_width_deg: f64,
    /// Oporność profilowa przy cl = cl0.
    pub cd0: f64,
    /// Współczynnik oporności indukowanej/profilowej od cl.
    pub k_cd: f64,
    /// Dodatkowa oporność w pełnym przeciągnięciu.
    pub cd_stall: f64,
}

impl Foil {
    /// Profil klasy DU/NACA 63-4xx (łopata kompozytowa).
    pub fn modern() -> Self {
        Self {
            cl0: 0.35,
            alpha0_deg: -3.5,
            cl_slope: 6.0,
            alpha_stall_deg: 14.0,
            stall_width_deg: 8.0,
            cd0: 0.0085,
            k_cd: 0.012,
            cd_stall: 1.15,
        }
    }

    /// Profil z blachy / wygiętej płyty (tanie wirniki, wiatraki farmerskie).
    pub fn sheet_metal() -> Self {
        Self {
            cl0: 0.20,
            alpha0_deg: -2.0,
            cl_slope: 4.6,
            alpha_stall_deg: 10.0,
            stall_width_deg: 7.0,
            cd0: 0.022,
            k_cd: 0.030,
            cd_stall: 1.35,
        }
    }

    /// Szybki profil dla wirników, w których kluczowy jest koszt, nie sprawność.
    pub fn crude() -> Self {
        Self {
            cl0: 0.15,
            alpha0_deg: -1.5,
            cl_slope: 4.0,
            alpha_stall_deg: 9.0,
            stall_width_deg: 6.0,
            cd0: 0.035,
            k_cd: 0.045,
            cd_stall: 1.5,
        }
    }

    /// cl(alfa) z przejściem w model płaskiej płyty (Viterna).
    pub fn cl(&self, alpha_deg: f64) -> f64 {
        let attached = self.cl0 + self.cl_slope * (alpha_deg - self.alpha0_deg) * DEG2RAD;
        let a = alpha_deg.abs();
        let s0 = self.alpha_stall_deg;
        if a <= s0 {
            return attached;
        }
        let t = smoothstep((a - s0) / self.stall_width_deg);
        // Płaska płyta: cl = 1.1 * sin(2*alfa) (model Viterny dla AR ~ 5-8).
        let stalled = 1.1 * (2.0 * alpha_deg * DEG2RAD).sin();
        (1.0 - t) * attached + t * stalled
    }

    /// cd(alfa): oporność profilowa + rosnąca z cl + separacja.
    pub fn cd(&self, alpha_deg: f64) -> f64 {
        let cl = self.cl(alpha_deg);
        let a = alpha_deg.abs();
        let t = smoothstep((a - self.alpha_stall_deg) / self.stall_width_deg);
        let dcl = cl - self.cl0;
        self.cd0 + self.k_cd * dcl * dcl + self.cd_stall * t * t
    }

    /// Maksymalna jakość aerodynamiczna cl/cd (do raportowania).
    pub fn max_glide(&self) -> f64 {
        let mut best: f64 = 0.0;
        let mut i = -10.0;
        while i <= 20.0 {
            let a = i;
            let cd = self.cd(a).max(1e-6);
            best = best.max(self.cl(a) / cd);
            i += 0.25;
        }
        best
    }
}

/// "Biegunowa" wirującego cylindra (efekt Magnusa).
///
/// Siła nośna cylindra rośnie ze współczynnikiem skręcenia `k = omega*r/V`
/// i nasyca się przy k ~ 2-3 (zależnie od końcówek / tarcz).
/// Oporność rośnie liniowo z k (cylinder ma zawsze duży opór).
#[derive(Clone, Copy, Debug)]
pub struct Magnus {
    /// Nasycone cl (typowo 4.5-6.0 z tarczami końcowymi).
    pub cl_max: f64,
    /// Skala nasycenia (k, przy którym cl ~ 0.63*cl_max).
    pub k0: f64,
    /// Oporność cylindra bez skręcenia.
    pub cd0: f64,
    /// Narastanie oporności ze skręceniem.
    pub cd_k: f64,
    /// Współczynnik momentu (tarcie) do policzenia mocy potrzebnej na obrót.
    pub cf_spin: f64,
}

impl Magnus {
    pub fn typical() -> Self {
        Self {
            cl_max: 5.0,
            k0: 1.5,
            cd0: 0.35,
            cd_k: 0.30,
            cf_spin: 0.010,
        }
    }

    pub fn cl(&self, _alpha_deg: f64, k: f64) -> f64 {
        // Dla cylindra siła nośna jest (w przybliżeniu) prostopadła do prędkości
        // względnej niezależnie od kąta natarcia, więc cl nie zależy od alfa.
        self.cl_max * (1.0 - (-clamp(k, 0.0, 8.0) / self.k0).exp())
    }

    pub fn cd(&self, _alpha_deg: f64, k: f64) -> f64 {
        self.cd0 + self.cd_k * clamp(k, 0.0, 8.0)
    }
}

/// Polimorficzna biegunowa używana przez BEMT.
#[derive(Clone, Copy, Debug)]
pub enum Polar {
    Foil(Foil),
    Magnus(Magnus),
}

impl Polar {
    /// cl(alfa, k) - `k` jest ignorowane dla profili lotniczych.
    #[inline]
    pub fn cl(&self, alpha_deg: f64, k: f64) -> f64 {
        match self {
            Polar::Foil(f) => f.cl(alpha_deg),
            Polar::Magnus(m) => m.cl(alpha_deg, k),
        }
    }

    /// cd(alfa, k).
    #[inline]
    pub fn cd(&self, alpha_deg: f64, k: f64) -> f64 {
        match self {
            Polar::Foil(f) => f.cd(alpha_deg),
            Polar::Magnus(m) => m.cd(alpha_deg, k),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foil_attached_slope_is_sane() {
        let f = Foil::modern();
        let c1 = f.cl(4.0);
        let c2 = f.cl(5.0);
        // ~6 / rad => ~0.105 na stopień
        assert!((c2 - c1 - 6.0 * DEG2RAD).abs() < 1e-6);
    }

    #[test]
    fn foil_stalls() {
        let f = Foil::modern();
        assert!(f.cl(14.0) > f.cl(30.0));
        assert!(f.cd(30.0) > 0.5);
    }

    #[test]
    fn foil_glide_ratio_in_range() {
        let g = Foil::modern().max_glide();
        assert!(g > 40.0 && g < 200.0, "cl/cd = {g}");
    }

    #[test]
    fn magnus_saturates() {
        let m = Magnus::typical();
        assert!(m.cl(0.0, 0.5) > 1.0);
        assert!(m.cl(0.0, 3.0) > m.cl(0.0, 1.0));
        assert!(m.cl(0.0, 3.0) <= m.cl_max + 1e-9);
        assert!(m.cd(0.0, 2.0) > 0.5, "cylinder musi miec duzy opor");
    }
}
