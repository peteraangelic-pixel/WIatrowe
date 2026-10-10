//! Model wiatru: rozkład Weibulla + profil pionowy (prawo potęgowe).
//!
//! AEP liczone jest przez całkowanie krzywej mocy po rozkładzie prędkości
//! wiatru odniesionym do wysokości piasty danej maszyny (ścinanie wiatru jest
//! więc uwzględnione: wysoki maszt i latawiec na 300 m dostają realną premię).

/// Rozkład Weibulla prędkości wiatru na wysokości odniesienia.
#[derive(Clone, Copy, Debug)]
pub struct Climate {
    /// Parametr kształtu.
    pub k: f64,
    /// Średnia prędkość na wysokości odniesienia [m/s].
    pub mean_v: f64,
    /// Wysokość odniesienia [m].
    pub ref_height: f64,
    /// Wykładnik ścinania wiatru (0.14 morze, 0.20-0.25 ląd z przeszkodami).
    pub shear: f64,
}

impl Climate {
    /// Warunki typowe dla nizinnej Polski (k ~ 2.0, ~6.5 m/s na 100 m).
    pub fn poland() -> Self {
        Climate {
            k: 2.0,
            mean_v: 6.5,
            ref_height: 100.0,
            shear: 0.20,
        }
    }

    /// Dobre stanowisko (wybrzeże / wyższa wieża).
    pub fn coastal() -> Self {
        Climate {
            k: 2.2,
            mean_v: 8.0,
            ref_height: 100.0,
            shear: 0.14,
        }
    }

    /// Skala Weibulla (lambda) ze średniej: mean = lambda * Gamma(1 + 1/k).
    pub fn scale(&self) -> f64 {
        // Gamma(1 + 1/k) policzone wprost (całka) - bezpieczne dla k > 0.5.
        let n = 2000;
        let upper = 60.0;
        let dh = upper / n as f64;
        let mut s = 0.0;
        for i in 0..n {
            let x = (i as f64 + 0.5) * dh;
            s += x.powf(1.0 / self.k) * (-x).exp() * dh;
        }
        if s <= 0.0 {
            self.mean_v
        } else {
            self.mean_v / s
        }
    }

    /// Gęstość prawdopodobieństwa prędkości na wysokości odniesienia [1/(m/s)].
    pub fn pdf(&self, v: f64) -> f64 {
        if v <= 0.0 {
            return 0.0;
        }
        let l = self.scale();
        let k = self.k;
        (k / l) * (v / l).powf(k - 1.0) * (-(v / l).powf(k)).exp()
    }

    /// Mnożnik prędkości dla wysokości h (prawo potęgowe).
    pub fn shear_factor(&self, h: f64) -> f64 {
        let h = h.max(2.0);
        (h / self.ref_height).powf(self.shear)
    }

    /// Prędkość na wysokości h dla danej prędkości na wysokości odniesienia.
    pub fn at_height(&self, v_ref: f64, h: f64) -> f64 {
        v_ref * self.shear_factor(h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_integrates_to_one() {
        let c = Climate::poland();
        let mut s = 0.0;
        let dv = 0.01;
        let mut v = dv / 2.0;
        while v < 60.0 {
            s += c.pdf(v) * dv;
            v += dv;
        }
        assert!((s - 1.0).abs() < 0.02, "calka pdf = {s}");
    }

    #[test]
    fn mean_of_distribution_matches_input() {
        let c = Climate::poland();
        let mut m = 0.0;
        let dv = 0.01;
        let mut v = dv / 2.0;
        while v < 60.0 {
            m += v * c.pdf(v) * dv;
            v += dv;
        }
        assert!((m - c.mean_v).abs() < 0.1, "mean = {m}");
    }

    #[test]
    fn shear_increases_with_height() {
        let c = Climate::poland();
        assert!(c.shear_factor(150.0) > c.shear_factor(100.0));
        assert!(c.at_height(7.0, 150.0) > 7.0);
    }
}
