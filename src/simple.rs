//! Modele "niekonwencjonalne": Savonius, hybryda Darrieus+Savonius,
//! bezwirnikowy rezonans wirowy (VIV), latawiec crosswind (AWE), lej Venturiego
//! (INVELOX) oraz taśma flatterowa (windbelt).
//!
//! Dla maszyn, dla których nie ma sensownego modelu elemetarnego w tej skali
//! (Savonius, windbelt) stosujemy jawną, skalibrowaną krzywą empiryczną
//!
//!   Cp(l) = Cp_max * (l/l_opt) * ((l_fw - l)/(l_fw - l_opt))^p,  p = l_fw/l_opt - 1
//!
//! która z definicji ma zero przy l = 0, maksimum Cp_max przy l = l_opt i zero
//! przy prędkości "luzem" l_fw. Wartości skalibruje się do zakresów podawanych
//! w przeglądach doświadczalnych (Savonius Cp_max ~ 0.15-0.25 przy l ~ 0.8-1.0).
//!
//! Latawiec liczony jest wzorem Loyda (1980), a lej Venturiego - z zasady
//! zachowania masy z ograniczeniem Betza (bez tego model dawałby Cp > Betz).

use crate::{clamp, BETZ, RHO};

/// Uniwersalna, skalibrowana krzywa Cp(lambda) dla maszyn oporowych
/// i niskosprawnych (kształt: zero -> max -> zero przy "luzem").
pub fn empirical_cp(lambda: f64, cp_max: f64, lambda_opt: f64, lambda_fw: f64) -> f64 {
    if lambda <= 0.0 || lambda >= lambda_fw {
        return 0.0;
    }
    let p = lambda_fw / lambda_opt - 1.0;
    let shape = (lambda / lambda_opt) * ((lambda_fw - lambda) / (lambda_fw - lambda_opt)).powf(p);
    (cp_max * shape).max(0.0)
}

// ---------------------------------------------------------------- Savonius ---

/// Savonius - wirnik oporowy o pracy ciągłej, samorozruchowy.
#[derive(Clone, Debug)]
pub struct SavoniusSpec {
    pub radius: f64,
    pub height: f64,
    /// Liczba "łyżek" (2 lub 3).
    pub n_buckets: usize,
    /// Kąt łuku łyżki [deg] (optimum ~ 135).
    pub arc_deg: f64,
    /// Przekrycie (overlap) jako ułamek średnicy łyżki.
    pub overlap: f64,
    /// Zysk z tarcz końcowych (end plates).
    pub endplate_gain: f64,
}

impl SavoniusSpec {
    /// Pole zakreślane [m^2].
    pub fn swept(&self) -> f64 {
        2.0 * self.radius * self.height
    }
    /// Powierzchnia materiału łyżek [m^2] (łuki walca).
    pub fn blade_area(&self) -> f64 {
        let rb = 0.5 * self.radius;
        let arc = self.arc_deg.to_radians();
        rb * arc * self.height * self.n_buckets as f64
    }

    /// Szczytowy Cp (kalibracja: ~0.16-0.22 dla klasycznego Savoniusa z tarczami).
    fn cp_max(&self) -> f64 {
        // Optimum kąta łuku ~ 135 deg; 3 łopaty nieco gorsze od 2.
        let arc_f = 1.0 - 0.35 * ((self.arc_deg - 135.0) / 60.0).powi(2);
        let n_f = if self.n_buckets == 2 { 1.0 } else { 0.92 };
        let ov_f = 1.0 + 0.18 * clamp(self.overlap / 0.15, 0.0, 1.0);
        0.16 * arc_f.max(0.6) * n_f * ov_f * self.endplate_gain
    }

    /// Wolna prędkość ("luzem") - Savonius "ucieka" przy lambda ~ 2.
    fn lambda_freewheel(&self) -> f64 {
        2.0 + 0.15 * self.overlap / 0.15
    }

    pub fn cp(&self, lambda: f64) -> f64 {
        empirical_cp(lambda, self.cp_max(), 0.85, self.lambda_freewheel())
    }

    /// Przybliżony współczynnik siły nacisku (Savonius ma bardzo duży opór).
    pub fn ct(&self) -> f64 {
        0.9
    }
}

// ------------------------------------------------------------ hybryda D+S ---

/// Hybryda: Darrieus na zewnątrz + Savonius w środku (samorozruch).
/// Model: strata szczytowego Cp Darrieusa (opory wirnika wewnętrznego) +
/// dodatek momentu przy niskim lambda od Savoniusa.
#[derive(Clone, Debug)]
pub struct HybridSpec {
    pub darrieus_cp: f64,        // Cp Darrieusa w danym punkcie (z DMST)
    pub savonius_cp: f64,        // Cp Savoniusa w danym punkcie (z krzywej)
    /// Strata od interferencji (0.12 = -12% Cp Darrieusa).
    pub interference: f64,
    /// Udział Savoniusa w mocy całkowitej.
    pub sav_share: f64,
}

impl HybridSpec {
    pub fn cp(&self) -> f64 {
        self.darrieus_cp * (1.0 - self.interference) + self.savonius_cp * self.sav_share
    }
}

// ------------------------------------------------------------------ VIV ------

/// Bezwirnikowy rezonans wirowy (vortex induced vibration, np. Vortex Bladeless).
///
/// Moc bierze się z wymuszonego drgania walca w paśmie synchronizacji
/// (lock-in) przy częstości Strouhala f_s = St*V/D. Poza wąskim pasmem
/// sprawność gwałtownie spada - to jest główna wada tej koncepcji
/// (bardzo niski wskaźnik wykorzystania rocznego).
#[derive(Clone, Debug)]
pub struct VivSpec {
    /// Średnica walca [m].
    pub diameter: f64,
    /// Wysokość walca [m].
    pub height: f64,
    /// Liczba Strouhala.
    pub strouhal: f64,
    /// Prędkość, na którą dostrojony jest rezonans [m/s].
    pub v_lock: f64,
    /// Względna szerokość pasma synchronizacji (0.15 = +/- 15% V).
    pub bandwidth: f64,
    /// Szczytowy współczynnik mocy (kalibracja: ~0.09).
    pub cp_peak: f64,
    /// Amplituda drgań jako ułamek średnicy.
    pub amplitude_frac: f64,
    /// Czy urządzenie jest strojone (poszerza pasmo).
    pub tunable: bool,
}

impl VivSpec {
    /// Pole odniesienia: pasmo zakreślane przez drgający walec [m^2].
    pub fn swept(&self) -> f64 {
        self.height * (self.diameter + 2.0 * self.amplitude_frac * self.diameter)
    }

    /// Cp(V) - gaussowskie pasmo synchronizacji wokół prędkości rezonansowej.
    pub fn cp(&self, v: f64) -> f64 {
        let bw = if self.tunable {
            self.bandwidth * 2.0
        } else {
            self.bandwidth
        };
        let sigma = (bw * self.v_lock).max(0.2);
        let d = (v - self.v_lock) / sigma;
        self.cp_peak * (-0.5 * d * d).exp()
    }

    /// Częstość drgań [Hz] (tylko do raportu).
    pub fn frequency(&self) -> f64 {
        self.strouhal * self.v_lock / self.diameter
    }
}

// ------------------------------------------------------------------ AWE ------

/// Latawiec crosswind / airborne wind energy (wzór Loyda, 1980).
///
///   P = (2/27) * rho * A * C_L * (C_L/C_D)^2 * V^3
///
/// z jawnie wydzielonymi współczynnikami cyklu (reel-in/out), oporu liny
/// i dyspozycyjności.
#[derive(Clone, Debug)]
pub struct KiteSpec {
    /// Powierzchnia skrzydła [m^2].
    pub wing_area: f64,
    /// Współczynnik siły nośnej w locie crosswind.
    pub cl: f64,
    /// Doskonałość aerodynamiczna skrzydła (L/D).
    pub glide: f64,
    /// Wysokość operacyjna [m].
    pub altitude: f64,
    /// Długość liny [m].
    pub tether_len: f64,
    /// Sprawność cyklu (reel-out/reel-in).
    pub cycle_eff: f64,
    /// Sprawność przeniesienia mocy (opór liny, przetwornica).
    pub tether_eff: f64,
    /// Dyspozycyjność (wiatr, serwis, start/lądowanie).
    pub availability: f64,
    /// Masa skrzydła na m^2 [kg/m^2] (miękki latawiec ~2, sztywny ~8).
    pub wing_areal_mass: f64,
}

impl KiteSpec {
    /// Moc mechaniczna [W] przy prędkości v na wysokości operacyjnej.
    pub fn power(&self, v: f64, rho: f64) -> f64 {
        (2.0 / 27.0) * rho * self.wing_area * self.cl * self.glide * self.glide * v * v * v
            * self.cycle_eff
            * self.tether_eff
            * self.availability
    }

    /// "Równoważne" pole zakreślane: takie A, przy którym konwencjonalny
    /// wirnik o Cp = BETZ*dawałby tę samą moc.
    pub fn equivalent_swept(&self, rho: f64, v: f64) -> f64 {
        let p = self.power(v, rho);
        let q = 0.5 * rho * v * v * v * BETZ;
        if q <= 0.0 {
            0.0
        } else {
            p / q
        }
    }

    /// Powierzchnia terenu zajmowana przez kołowe "niebo" latawca [m^2].
    pub fn footprint(&self) -> f64 {
        let r = self.tether_len * 0.7;
        std::f64::consts::PI * r * r
    }
}

// --------------------------------------------------------------- Venturi ----

/// Lej Venturiego (np. INVELOX): duży wlot, przewężenie z wirnikiem, dyfuzor.
///
/// Kluczowe: prędkość w gardzieli nie może rosnąć dowolnie - ogranicza ją
/// bilans energii (model docięty ograniczeniem Betza odniesionym do pola wlotu),
/// inaczej wychodziłoby Cp > Betz, co jest fałszem fizycznym, a nie "rewolucją".
#[derive(Clone, Debug)]
pub struct VenturiSpec {
    /// Pole wlotu [m^2].
    pub inlet_area: f64,
    /// Pole gardzieli [m^2] (tu siedzi wirnik).
    pub throat_area: f64,
    /// Skuteczność koncentracji (0.08-0.12; 1.0 = idealna, nierealna).
    pub concentration_eff: f64,
    /// Sprawność kanału (straty tarciowe, separacja w dyfuzorze).
    pub duct_eff: f64,
    /// Wysokość wlotu [m].
    pub inlet_height: f64,
}

impl VenturiSpec {
    /// Stosunek koncentracji.
    pub fn cr(&self) -> f64 {
        self.inlet_area / self.throat_area
    }

    /// Przyrost prędkości w gardzieli: r_v = 1 + e*(CR - 1), docięty Betzem.
    pub fn velocity_ratio(&self, cp_rotor: f64) -> f64 {
        let ideal = 1.0 + self.concentration_eff * (self.cr() - 1.0);
        // Ograniczenie: Cp(inlet) = Cp_rotor * r_v^3 / CR <= BETZ
        let max_rv = (BETZ * self.cr() / cp_rotor.max(0.05)).powf(1.0 / 3.0);
        ideal.min(max_rv)
    }

    /// Cp odniesione do pola wlotu.
    pub fn cp_inlet(&self, cp_rotor: f64, r_v: f64) -> f64 {
        let raw = cp_rotor * r_v.powi(3) / self.cr() * self.duct_eff;
        raw.min(BETZ * self.duct_eff)
    }
}

// ---------------------------------------------------------------- windbelt ---

/// Taśma flatterowa (windbelt): bardzo tania, bardzo niskosprawna.
#[derive(Clone, Debug)]
pub struct BeltSpec {
    /// Długość taśmy [m].
    pub length: f64,
    /// Amplituda drgań [m].
    pub amplitude: f64,
    /// Szerokość taśmy [m].
    pub width: f64,
    /// Szczytowy Cp (kalibracja z doniesień: ~0.03).
    pub cp_max: f64,
    /// lambda_opt (odniesione do prędkości taśmy / prędkości wiatru).
    pub lambda_opt: f64,
    pub lambda_fw: f64,
}

impl BeltSpec {
    pub fn swept(&self) -> f64 {
        self.length * self.amplitude * 2.0
    }
    pub fn cp(&self, lambda: f64) -> f64 {
        empirical_cp(lambda, self.cp_max, self.lambda_opt, self.lambda_fw)
    }
}

/// Moc z pola i Cp (wspólna utilitka).
#[inline]
pub fn power_from_cp(cp: f64, area: f64, v: f64, rho: f64) -> f64 {
    0.5 * rho * area * v * v * v * cp
}

/// Domyślna gęstość (utilitka).
#[inline]
pub fn rho_default() -> f64 {
    RHO
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empirical_curve_peaks_at_lambda_opt() {
        let (cp_max, lo, lf) = (0.19, 0.85, 2.0);
        let peak = empirical_cp(lo, cp_max, lo, lf);
        assert!((peak - cp_max).abs() < 1e-9, "peak={peak}");
        for l in [0.1, 0.3, 0.5, 0.7, 1.2, 1.6, 1.9] {
            let c = empirical_cp(l, cp_max, lo, lf);
            assert!(c < cp_max + 1e-12 && c >= 0.0, "l={l} cp={c}");
        }
        assert_eq!(empirical_cp(0.0, cp_max, lo, lf), 0.0);
        assert_eq!(empirical_cp(2.5, cp_max, lo, lf), 0.0);
    }

    #[test]
    fn savonius_is_in_literature_range() {
        let s = SavoniusSpec {
            radius: 2.0,
            height: 4.0,
            n_buckets: 2,
            arc_deg: 135.0,
            overlap: 0.15,
            endplate_gain: 1.15,
        };
        let mut best = 0.0;
        let mut l = 0.0;
        let mut x = 0.05;
        while x < 2.0 {
            let c = s.cp(x);
            if c > best {
                best = c;
                l = x;
            }
            x += 0.01;
        }
        assert!(best > 0.13 && best < 0.28, "Cp_max={best}");
        assert!(l > 0.6 && l < 1.2, "lambda_opt={l}");
        assert!(s.cp(0.05) > 0.0, "Savonius musi miec moment rozruchowy");
    }

    #[test]
    fn kite_loyd_matches_reference() {
        // Loyd: A=100 m^2, CL=1.5, G=15, V=12 m/s
        let k = KiteSpec {
            wing_area: 100.0,
            cl: 1.5,
            glide: 15.0,
            altitude: 300.0,
            tether_len: 400.0,
            cycle_eff: 1.0,
            tether_eff: 1.0,
            availability: 1.0,
            wing_areal_mass: 3.0,
        };
        let p = k.power(12.0, 1.225);
        let expected = (2.0 / 27.0) * 1.225 * 100.0 * 1.5 * 225.0 * 12f64.powi(3);
        assert!((p - expected).abs() / expected < 1e-9, "p={p}");
        // rząd wielkości: kilka MW ze 100 m^2 skrzydła
        assert!(p > 1.0e6 && p < 1.0e7, "p={p}");
    }

    #[test]
    fn venturi_never_beats_betz_per_inlet_area() {
        let v = VenturiSpec {
            inlet_area: 100.0,
            throat_area: 10.0,
            concentration_eff: 0.6, // absurdalnie optymistycznie
            duct_eff: 1.0,
            inlet_height: 20.0,
        };
        let r = v.velocity_ratio(0.45);
        let cp = v.cp_inlet(0.45, r);
        assert!(cp <= BETZ + 1e-9, "Cp={cp}");
    }

    #[test]
    fn viv_has_narrow_band() {
        let s = VivSpec {
            diameter: 0.5,
            height: 6.0,
            strouhal: 0.2,
            v_lock: 11.0,
            bandwidth: 0.15,
            cp_peak: 0.09,
            amplitude_frac: 0.6,
            tunable: false,
        };
        assert!(s.cp(11.0) > 0.85 * 0.09);
        assert!(s.cp(6.0) < 0.02, "poza pasmem powinno byc prawie zero");
        assert!(s.cp(20.0) < 0.02);
    }
}
