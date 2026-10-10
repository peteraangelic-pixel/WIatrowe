//! `wiatrowe` - przesiewowy (screening) symulator alternatywnych geometrii elektrowni
//! wiatrowych.
//!
//! Zamiast klasycznego, trójłopatowego HAWT-a przeliczamy kilkanaście różnych
//! "kształtów" maszyny (HAWT 1/2/3/6/18 łopat, DAWT z dyfuzorem, lej Venturiego,
//! Darrieus H-rotor/troposkien/helikalny, Savonius, hybryda, Magnus, bezwirnikowy
//! rezonans wirowy, latawiec crosswind/AWE, taśma flatterowa, przeciwbieżny
//! dwuwirnikowy) i porównujemy je *jednocześnie* pod kątem:
//!
//!   * sprawności aerodynamicznej (Cp, AEP, capacity factor),
//!   * kosztu wytworzenia (masa -> CAPEX, BOS, OPEX),
//!   * LCOE oraz energii uzyskanej z 1 kg materiału.
//!
//! Modele są celowo lekkie (rzędu BEMT / DMST / modele quasi-statyczne), a nie CFD:
//! mają służyć do *przesiewu* koncepcji, nie do certyfikacji łopaty.
//! Wszystkie stałe kalibracyjne są jawne i opisane w `cost.rs` oraz `designs.rs`.
//!
//! Równoległość: rayon (`par_iter`, `par_sort_unstable_by`, `fold/reduce`, `join`).

pub mod airfoil;
pub mod bemt;
pub mod charts;
pub mod climate;
pub mod cost;
pub mod designs;
pub mod dmst;
pub mod report;
pub mod shapes;
pub mod simple;
pub mod sweep;

/// Gęstość powietrza na poziomie morza, 15 °C [kg/m^3].
pub const RHO: f64 = 1.225;
/// Przyspieszenie ziemskie [m/s^2].
pub const G: f64 = 9.80665;
/// Stopnie -> radiany.
pub const DEG2RAD: f64 = std::f64::consts::PI / 180.0;
/// Radians -> stopnie.
pub const RAD2DEG: f64 = 180.0 / std::f64::consts::PI;
/// Limit Betza 16/27.
pub const BETZ: f64 = 16.0 / 27.0;

/// Liczbowo bezpieczne minimum.
pub const EPS: f64 = 1e-12;

#[inline]
pub fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

/// Gładka funkcja przejścia 0..1 (smoothstep) używana m.in. do modelu przeciągnięcia.
#[inline]
pub fn smoothstep(t: f64) -> f64 {
    let t = clamp(t, 0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}
