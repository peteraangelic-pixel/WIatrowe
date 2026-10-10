//! Katalog badanych "kształtów" elektrowni wiatrowych.
//!
//! Każdy wariant opisany jest wspólnym zestawem parametrów (geometria,
//! sterowanie, zakres skalowania) i funkcją `build`, która tworzy model
//! aerodynamiczny dla zadanej skali. Dzięki temu przeliczenie wszystkich
//! wariantów w jednej pętli (równolegle) jest trywialne.

use crate::airfoil::{Foil, Magnus, Polar};
use crate::bemt::{self, BemtCfg, HawtRotor};
use crate::dmst::{self, DarrieusRotor};
use crate::simple::{BeltSpec, KiteSpec, SavoniusSpec, VenturiSpec, VivSpec};
use crate::{clamp, BETZ, RHO};

/// Maksymalny *realny* przyrost mocy z układu dwuwirnikowego (oba wirniki
/// dzielą tę samą strugę, więc nie ma "darmowej" mocy; literatura podaje
/// typowo +5...+15% dla przeciwbieżnych układów).
pub const COUNTER_MAX_GAIN: f64 = 0.12;

// ----------------------------------------------------------- typy pomocnicze --

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Family {
    Hawt,      // pozioma oś, wirnik łopatowy
    Ducted,    // dyfuzor / kołnierz
    Venturi,   // lej zasysający
    VawtLift,  // pionowa oś, nośne (Darrieus)
    VawtDrag,  // pionowa oś, oporowe (Savonius)
    Magnus,    // wirujące cylindry
    Bladeless, // rezonans wirowy
    Airborne,  // latawiec / AWE
    Belt,      // taśma flatterowa
}

impl Family {
    pub fn label(&self) -> &'static str {
        match self {
            Family::Hawt => "HAWT (pozioma oś)",
            Family::Ducted => "DAWT (dyfuzor)",
            Family::Venturi => "Lej Venturiego",
            Family::VawtLift => "VAWT nośny (Darrieus)",
            Family::VawtDrag => "VAWT oporowy (Savonius)",
            Family::Magnus => "Magnus (cylindry)",
            Family::Bladeless => "Bezwirnikowy (VIV)",
            Family::Airborne => "Latawiec (AWE)",
            Family::Belt => "Taśma flatterowa",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScaleKind {
    /// Skala = promień wirnika [m].
    Radius,
    /// Skala = powierzchnia skrzydła [m^2] (AWE).
    WingArea,
}

/// Sterowanie / ograniczenia eksploatacyjne.
#[derive(Clone, Copy, Debug)]
pub struct Ctrl {
    /// Zakres przeszukiwanego lambda.
    pub lambda_lo: f64,
    pub lambda_hi: f64,
    /// Maksymalna prędkość końcówki [m/s] (hałas, erozja).
    pub tip_speed_max: f64,
    /// Czy wirnik ma regulację kąta łopat (odciążanie powyżej mocy nominalnej).
    pub pitch_control: bool,
    /// Prędkość nominalna [m/s].
    pub v_rated: f64,
    /// Prędkość załączenia [m/s].
    pub v_in: f64,
    /// Prędkość wyłączenia [m/s].
    pub v_out: f64,
    /// Dodatkowy parametr (Magnus: współczynnik skręcenia k).
    pub spin_lo: f64,
    pub spin_hi: f64,
}

impl Ctrl {
    pub fn rotor(lo: f64, hi: f64, tip: f64, v_rated: f64) -> Self {
        Self {
            lambda_lo: lo,
            lambda_hi: hi,
            tip_speed_max: tip,
            pitch_control: true,
            v_rated,
            v_in: 3.0,
            v_out: 25.0,
            spin_lo: 0.0,
            spin_hi: 0.0,
        }
    }
}

// --------------------------------------------------------------- geometria ----

/// Dane geometryczne potrzebne do wyceny i do raportu.
#[derive(Clone, Copy, Debug, Default)]
pub struct Geometry {
    /// Pole odniesienia dla Cp [m^2].
    pub area: f64,
    /// Powierzchnia "łopat" (rzut) [m^2].
    pub blade_area: f64,
    /// Wysokość piasty / wysokość operacyjna [m].
    pub hub: f64,
    /// Wysokość konstrukcji (wieża, maszt) [m].
    pub structure_height: f64,
    /// Powierzchnia poszycia dyfuzora/leja [m^2].
    pub duct_area: f64,
    /// Zajętość terenu [m^2].
    pub footprint: f64,
    /// Pole skrzydła (AWE) [m^2].
    pub wing_area: f64,
    /// Długość liny (AWE) [m].
    pub tether_len: f64,
    /// Średnica wirnika (do odstępów) [m].
    pub diameter: f64,
}

// ------------------------------------------------------------------ modele ----

#[derive(Clone, Debug)]
pub struct HawtSpec {
    pub rotor: HawtRotor,
    /// Czy to wirnik Magnus (cylindry zamiast profili).
    pub magnus: bool,
    /// Współczynnik skręcenia cylindrów k (Magnus).
    pub spin_ratio: f64,
    /// Drugi, przeciwbieżny wirnik w śladzie.
    pub counter_rotating: bool,
    /// Udział energii wiru odzyskany przez drugi wirnik.
    pub counter_gain: f64,
    /// Straty pasożytnicze (piasta, obudowa, łoża) jako ułamek Cp.
    pub parasitic: f64,
}

#[derive(Clone, Debug)]
pub struct DuctedSpec {
    pub rotor: HawtRotor,
    /// Stosunek pola wylotu do pola wirnika (beta).
    pub beta: f64,
    pub flange: bool,
    /// Długość dyfuzora jako ułamek R.
    pub length_frac: f64,
    /// Skuteczność koncentracji prędkości.
    pub concentration_eff: f64,
    /// Sprawność dyfuzora (straty, opór bazy).
    pub duct_eff: f64,
    pub parasitic: f64,
}

#[derive(Clone, Debug)]
pub struct VawtSpec {
    pub rotor: DarrieusRotor,
    /// Straty pasożytnicze (łożyska, wał centralny).
    pub parasitic: f64,
}

#[derive(Clone, Debug)]
pub struct HybridConfig {
    pub darrieus: VawtSpec,
    pub savonius: SavoniusSpec,
    /// Strata interferencyjna Darrieusa.
    pub interference: f64,
    /// Udział Savoniusa.
    pub sav_share: f64,
}

#[derive(Clone, Debug)]
pub enum Aero {
    Hawt(HawtSpec),
    Ducted(DuctedSpec),
    Venturi(VenturiSpec),
    Darrieus(VawtSpec),
    Savonius(SavoniusSpec),
    Hybrid(HybridConfig),
    Bladeless(VivSpec),
    Kite(KiteSpec),
    Belt(BeltSpec),
}

/// Wynik punktu pracy.
#[derive(Clone, Copy, Debug, Default)]
pub struct AeroOut {
    /// Moc mechaniczna na wale [W] (przed przekładnią/generatorem).
    pub power: f64,
    /// Współczynnik mocy względem pola odniesienia.
    pub cp: f64,
    /// Współczynnik naporu (ciągu) względem pola odniesienia.
    pub ct: f64,
    /// Siła pozioma [N].
    pub thrust: f64,
    pub converged: bool,
}

/// Jakość siatki obliczeniowej.
#[derive(Clone, Copy, Debug)]
pub struct Grid {
    pub n_stations: usize,
    pub n_tubes: usize,
    pub n_az: usize,
}

impl Grid {
    pub fn coarse() -> Self {
        Grid { n_stations: 18, n_tubes: 10, n_az: 60 }
    }
    pub fn normal() -> Self {
        Grid { n_stations: 32, n_tubes: 18, n_az: 120 }
    }
    pub fn fine() -> Self {
        Grid { n_stations: 48, n_tubes: 28, n_az: 200 }
    }
}

impl Aero {
    /// Ustawienie "projektowego" lambda (skręcenie łopaty idealne dla danego
    /// lambda) - dzięki temu krzywa Cp(lambda) jest obwiednią najlepszych
    /// łopat dla każdego lambda, a nie jedną łopatą zbadaną w całym zakresie.
    pub fn set_design_lambda(&mut self, l: f64) {
        match self {
            Aero::Hawt(s) => s.rotor.lambda_design = l,
            Aero::Ducted(s) => s.rotor.lambda_design = l,
            _ => {}
        }
    }

    /// Pole odniesienia dla Cp [m^2].
    pub fn area(&self) -> f64 {
        match self {
            Aero::Hawt(s) => std::f64::consts::PI * s.rotor.radius * s.rotor.radius,
            Aero::Ducted(s) => std::f64::consts::PI * s.rotor.radius * s.rotor.radius,
            Aero::Venturi(s) => s.inlet_area,
            Aero::Darrieus(s) => s.rotor.swept_area(),
            Aero::Savonius(s) => s.swept(),
            Aero::Hybrid(h) => h.darrieus.rotor.swept_area(),
            Aero::Bladeless(s) => s.swept(),
            Aero::Kite(k) => k.equivalent_swept(RHO, 11.0),
            Aero::Belt(s) => s.swept(),
        }
    }

    pub fn geometry(&self, hub: f64, spacing_x: f64, spacing_y: f64) -> Geometry {
        let d = match self {
            Aero::Hawt(s) => 2.0 * s.rotor.radius,
            Aero::Ducted(s) => 2.0 * s.rotor.radius,
            Aero::Venturi(s) => 2.0 * (s.inlet_area / std::f64::consts::PI).sqrt(),
            Aero::Darrieus(s) => 2.0 * s.rotor.radius,
            Aero::Savonius(s) => 2.0 * s.radius,
            Aero::Hybrid(h) => 2.0 * h.darrieus.rotor.radius,
            Aero::Bladeless(s) => s.diameter,
            Aero::Kite(_) => 0.0,
            Aero::Belt(s) => s.length,
        };
        let blade_area = match self {
            Aero::Hawt(s) => s.rotor.total_blade_area(),
            Aero::Ducted(s) => s.rotor.total_blade_area(),
            Aero::Darrieus(s) => s.rotor.blade_area(),
            Aero::Savonius(s) => s.blade_area(),
            Aero::Hybrid(h) => h.darrieus.rotor.blade_area() + h.savonius.blade_area(),
            Aero::Bladeless(s) => std::f64::consts::PI * s.diameter * s.height,
            Aero::Kite(k) => k.wing_area,
            Aero::Belt(s) => s.length * s.width,
            Aero::Venturi(_) => 0.0,
        };
        let duct_area = match self {
            Aero::Ducted(s) => {
                let r = s.rotor.radius;
                let l = s.length_frac * r;
                // powierzchnia stożka + kołnierza (gruby szacunek)
                std::f64::consts::PI * (2.0 * r + 2.0 * r * s.beta.sqrt()) * l * 0.5
                    + std::f64::consts::PI * r * r * (s.beta - 1.0)
            }
            Aero::Venturi(s) => 4.0 * s.inlet_area + 2.0 * s.throat_area,
            _ => 0.0,
        };
        Geometry {
            area: self.area(),
            blade_area,
            hub,
            structure_height: hub,
            duct_area,
            footprint: (spacing_x * d).max(1.0) * (spacing_y * d).max(1.0),
            wing_area: match self {
                Aero::Kite(k) => k.wing_area,
                _ => 0.0,
            },
            tether_len: match self {
                Aero::Kite(k) => k.tether_len,
                _ => 0.0,
            },
            diameter: d,
        }
    }

    /// Punkt pracy: moc mechaniczna [W] dla prędkości v i lambda.
    pub fn evaluate(&self, v: f64, lambda: f64, g: &Grid, rho: f64) -> AeroOut {
        let q = 0.5 * rho * v * v * v;
        match self {
            Aero::Hawt(s) => {
                let cfg = BemtCfg {
                    lambda,
                    pitch_deg: 0.0,
                    n_stations: g.n_stations,
                    spin_ratio: s.spin_ratio,
                };
                let o = bemt::solve(&s.rotor, &cfg);
                let mut cp = (o.cp - o.cp_spin) * (1.0 - s.parasitic);
                let mut ct = o.ct;
                if s.counter_rotating {
                    // Drugi wirnik pracuje w dalekim śladzie: V2 = V*(1 - 2a).
                    // Ma własne przełożenie, więc dobieramy mu sensowne lambda
                    // (nie musi kręcić się z tą samą prędkością co pierwszy).
                    let a = 0.5 * (1.0 - (1.0 - o.ct.min(0.96)).max(0.0).sqrt());
                    let vr = (1.0 - 2.0 * a).max(0.15);
                    let lam2 = clamp(lambda / vr, 3.0, 8.0);
                    let mut rot2 = s.rotor.clone();
                    rot2.lambda_design = lam2;
                    let cfg2 = BemtCfg { lambda: lam2, ..cfg };
                    let o2 = bemt::solve(&rot2, &cfg2);
                    let base = (o.cp - o.cp_spin) * (1.0 - s.parasitic);
                    let bonus = if o2.converged {
                        o2.cp.clamp(-0.5, BETZ) * vr.powi(3)
                    } else {
                        0.0
                    };
                    cp += bonus + s.counter_gain * o.cp_swirl;
                    cp = cp.min(base * (1.0 + COUNTER_MAX_GAIN));
                    ct = (ct + o2.ct * vr * vr).min(1.6);
                }
                cp = cp.min(BETZ).clamp(-0.5, BETZ);
                let area = std::f64::consts::PI * s.rotor.radius * s.rotor.radius;
                AeroOut {
                    power: q * area * cp,
                    cp,
                    ct,
                    thrust: 0.5 * rho * v * v * area * ct,
                    converged: o.converged,
                }
            }
            Aero::Ducted(s) => {
                let mut rv = 1.0 + s.concentration_eff * (s.beta - 1.0);
                if s.flange {
                    rv += 0.15;
                }
                let rv = clamp(rv, 1.0, 2.6);
                // Łopatę skręcamy dla przepływu, który faktycznie widzi
                // (V_rot = rv * V_inf), a nie dla prędkości niezakłóconej.
                let mut rotor = s.rotor.clone();
                rotor.lambda_design = lambda / rv;
                let cfg = BemtCfg {
                    lambda: lambda / rv,
                    pitch_deg: 0.0,
                    n_stations: g.n_stations,
                    spin_ratio: 0.0,
                };
                let o = bemt::solve(&rotor, &cfg);
                let cp_local = (o.cp - o.cp_spin) * (1.0 - s.parasitic);
                // Suﬁt: Cp(inlet) <= BETZ * beta * sprawnosc dyfuzora
                let cap = BETZ * s.beta * s.duct_eff;
                let cp = (cp_local * rv.powi(3)).min(cap).clamp(-0.5, 3.0);
                let area = std::f64::consts::PI * s.rotor.radius * s.rotor.radius;
                // Dyfuzor przenosi dużą część naporu całego układu.
                let ct = (o.ct * rv * rv * 1.7).min(2.2);
                AeroOut {
                    power: q * area * cp,
                    cp,
                    ct,
                    thrust: 0.5 * rho * v * v * area * ct,
                    converged: o.converged,
                }
            }
            Aero::Venturi(s) => {
                // Wirnik w gardzieli (lokalny Cp policzony BEM-tem dla małego
                // wirnika w gardzieli) - uproszczenie: Cp_rotor z modelu quasi-2D.
                let r_t = (s.throat_area / std::f64::consts::PI).sqrt();
                let rotor = HawtRotor {
                    radius: r_t,
                    hub_frac: 0.2,
                    n_blades: 3,
                    chord_root_frac: 0.12,
                    chord_tip_frac: 0.03,
                    lambda_design: lambda.max(2.0),
                    alpha_design_deg: 6.0,
                    polar: Polar::Foil(Foil::modern()),
                    thickness_frac: 0.18,
                };
                let cp_guess = 0.45;
                let rv = s.velocity_ratio(cp_guess);
                let rotor = HawtRotor {
                    lambda_design: (lambda / rv).max(0.5),
                    ..rotor
                };
                let cfg = BemtCfg {
                    lambda: lambda / rv,
                    pitch_deg: 0.0,
                    n_stations: g.n_stations,
                    spin_ratio: 0.0,
                };
                let o = bemt::solve(&rotor, &cfg);
                let cp_rotor = o.cp.max(0.05);
                let cp = s.cp_inlet(cp_rotor, s.velocity_ratio(cp_rotor));
                let ct = 0.75;
                AeroOut {
                    power: q * s.inlet_area * cp,
                    cp,
                    ct,
                    thrust: 0.5 * rho * v * v * s.inlet_area * ct,
                    converged: o.converged,
                }
            }
            Aero::Darrieus(s) => {
                let o = dmst::solve(&s.rotor, lambda, g.n_tubes, g.n_az);
                let cp = (o.cp * (1.0 - s.parasitic)).clamp(-0.5, BETZ);
                let area = s.rotor.swept_area();
                AeroOut {
                    power: q * area * cp,
                    cp,
                    ct: o.ct,
                    thrust: 0.5 * rho * v * v * area * o.ct,
                    converged: o.converged,
                }
            }
            Aero::Savonius(s) => {
                let cp = s.cp(lambda).clamp(-0.5, BETZ);
                let area = s.swept();
                AeroOut {
                    power: q * area * cp,
                    cp,
                    ct: s.ct(),
                    thrust: 0.5 * rho * v * v * area * s.ct(),
                    converged: true,
                }
            }
            Aero::Hybrid(h) => {
                let o = dmst::solve(&h.darrieus.rotor, lambda, g.n_tubes, g.n_az);
                let cp_d = o.cp * (1.0 - h.darrieus.parasitic);
                let cp_s = h.savonius.cp(lambda);
                let cp = (cp_d * (1.0 - h.interference) + cp_s * h.sav_share).clamp(-0.5, BETZ);
                let area = h.darrieus.rotor.swept_area();
                AeroOut {
                    power: q * area * cp,
                    cp,
                    ct: o.ct,
                    thrust: 0.5 * rho * v * v * area * o.ct,
                    converged: o.converged,
                }
            }
            Aero::Bladeless(s) => {
                let cp = s.cp(v).clamp(0.0, BETZ);
                let area = s.swept();
                let ct = 0.60;
                AeroOut {
                    power: q * area * cp,
                    cp,
                    ct,
                    thrust: 0.5 * rho * v * v * area * ct,
                    converged: true,
                }
            }
            Aero::Kite(k) => {
                let power = k.power(v, rho);
                let area = k.equivalent_swept(rho, v.max(0.5));
                let cp = if area > 0.0 {
                    power / (q * area)
                } else {
                    0.0
                };
                // Naciąg liny: rząd kilku P/V (typowe dla AWE).
                let thrust = 3.0 * power / v.max(0.5);
                AeroOut {
                    power,
                    cp,
                    ct: if area > 0.0 {
                        thrust / (0.5 * rho * v * v * area)
                    } else {
                        0.0
                    },
                    thrust,
                    converged: true,
                }
            }
            Aero::Belt(s) => {
                let cp = s.cp(lambda).clamp(-0.5, BETZ);
                let area = s.swept();
                let ct = 0.3;
                AeroOut {
                    power: q * area * cp,
                    cp,
                    ct,
                    thrust: 0.5 * rho * v * v * area * ct,
                    converged: true,
                }
            }
        }
    }
}

// ------------------------------------------------------------------ katalog ---

/// Opis wariantu w katalogu.
pub struct Design {
    pub id: &'static str,
    pub name: &'static str,
    pub en: &'static str,
    pub family: Family,
    pub scale_kind: ScaleKind,
    /// Skale (promień [m] lub powierzchnia skrzydła [m^2]) do przeliczenia.
    pub scales: &'static [f64],
    /// Wysokość piasty = hub_base + hub_per_scale * skala.
    pub hub_base: f64,
    pub hub_per_scale: f64,
    pub ctrl: Ctrl,
    /// Odstępy w parku (w średnicach).
    pub spacing_x: f64,
    pub spacing_y: f64,
    /// Pewność modelu (do raportu).
    pub confidence: &'static str,
    pub note: &'static str,
    pub build: fn(f64) -> Aero,
}

impl Design {
    pub fn instantiate(&self, scale: f64) -> Aero {
        (self.build)(scale)
    }
    pub fn hub_height(&self, scale: f64) -> f64 {
        self.hub_base + self.hub_per_scale * scale
    }
    pub fn geometry(&self, scale: f64) -> Geometry {
        let aero = self.instantiate(scale);
        aero.geometry(
            self.hub_height(scale),
            self.spacing_x,
            self.spacing_y,
        )
    }
}

// ------------------------------------------------------------- konstruktory ---

fn hawt(radius: f64, n: usize, c_root: f64, c_tip: f64, lam: f64, foil: Foil) -> HawtRotor {
    HawtRotor {
        radius,
        hub_frac: 0.15,
        n_blades: n,
        chord_root_frac: c_root,
        chord_tip_frac: c_tip,
        lambda_design: lam,
        alpha_design_deg: 6.0,
        polar: Polar::Foil(foil),
        thickness_frac: 0.18,
    }
}

fn naca0018() -> Foil {
    Foil {
        cl0: 0.0,
        alpha0_deg: 0.0,
        cl_slope: 6.0,
        alpha_stall_deg: 13.0,
        stall_width_deg: 8.0,
        cd0: 0.010,
        k_cd: 0.015,
        cd_stall: 1.20,
    }
}

// --- 1. bazowy HAWT 3-łopatowy
fn build_hawt3(r: f64) -> Aero {
    Aero::Hawt(HawtSpec {
        rotor: hawt(r, 3, 0.075, 0.018, 7.5, Foil::modern()),
        magnus: false,
        spin_ratio: 0.0,
        counter_rotating: false,
        counter_gain: 0.0,
        parasitic: 0.02,
    })
}

// --- 2. HAWT 2-łopatowy (mniej materiału, wyższe lambda, gorsza dynamika)
fn build_hawt2(r: f64) -> Aero {
    Aero::Hawt(HawtSpec {
        rotor: hawt(r, 2, 0.112, 0.027, 9.5, Foil::modern()),
        magnus: false,
        spin_ratio: 0.0,
        counter_rotating: false,
        counter_gain: 0.0,
        parasitic: 0.025,
    })
}

// --- 3. HAWT 1-łopatowy z przeciwwagą
fn build_hawt1(r: f64) -> Aero {
    Aero::Hawt(HawtSpec {
        rotor: hawt(r, 1, 0.21, 0.05, 13.0, Foil::modern()),
        magnus: false,
        spin_ratio: 0.0,
        counter_rotating: false,
        counter_gain: 0.0,
        parasitic: 0.03,
    })
}

// --- 4. HAWT 6-łopatowy (wolnoobrotowy)
fn build_hawt6(r: f64) -> Aero {
    Aero::Hawt(HawtSpec {
        rotor: hawt(r, 6, 0.045, 0.011, 4.5, Foil::modern()),
        magnus: false,
        spin_ratio: 0.0,
        counter_rotating: false,
        counter_gain: 0.0,
        parasitic: 0.03,
    })
}

// --- 5. wiatrak farmerski (18 blaszanych łopat)
fn build_farm18(r: f64) -> Aero {
    Aero::Hawt(HawtSpec {
        rotor: hawt(r, 18, 0.26, 0.19, 1.7, Foil::sheet_metal()),
        magnus: false,
        spin_ratio: 0.0,
        counter_rotating: false,
        counter_gain: 0.0,
        parasitic: 0.06,
    })
}

// --- 6. dyfuzor z kołnierzem (DAWT)
fn build_dawt(r: f64) -> Aero {
    Aero::Ducted(DuctedSpec {
        rotor: HawtRotor {
            radius: r,
            hub_frac: 0.18,
            n_blades: 5,
            chord_root_frac: 0.10,
            chord_tip_frac: 0.055,
            lambda_design: 5.0,
            alpha_design_deg: 6.0,
            polar: Polar::Foil(Foil::modern()),
            thickness_frac: 0.18,
        },
        beta: 2.6,
        flange: true,
        length_frac: 0.9,
        concentration_eff: 0.18,
        duct_eff: 0.45,
        parasitic: 0.03,
    })
}

// --- 7. przeciwbieżny dwuwirnikowy
fn build_counter(r: f64) -> Aero {
    Aero::Hawt(HawtSpec {
        rotor: hawt(r, 3, 0.075, 0.018, 7.0, Foil::modern()),
        magnus: false,
        spin_ratio: 0.0,
        counter_rotating: true,
        counter_gain: 0.5,
        parasitic: 0.03,
    })
}

// --- 8. lej Venturiego (INVELOX-like)
fn build_venturi(scale: f64) -> Aero {
    // scale = promień równoważny pola wlotu
    let inlet = std::f64::consts::PI * scale * scale;
    Aero::Venturi(VenturiSpec {
        inlet_area: inlet,
        throat_area: inlet / 9.0,
        concentration_eff: 0.12,
        duct_eff: 0.75,
        inlet_height: scale * 1.6,
    })
}

// --- 9. Darrieus H-rotor
fn build_hrotor(r: f64) -> Aero {
    let h = 1.5 * r;
    Aero::Darrieus(VawtSpec {
        rotor: DarrieusRotor {
            radius: r,
            height: h,
            n_blades: 3,
            chord: 0.06 * r,
            polar: Polar::Foil(naca0018()),
            strut_chord: 0.0125 * r,
            strut_cd: 0.06,
            n_struts: 6,
            oswald: 0.9,
            helical: false,
            troposkien: false,
        },
        parasitic: 0.05,
    })
}

// --- 10. Darrieus helikalny
fn build_helical(r: f64) -> Aero {
    let h = 1.5 * r;
    Aero::Darrieus(VawtSpec {
        rotor: DarrieusRotor {
            radius: r,
            height: h,
            n_blades: 3,
            chord: 0.065 * r,
            polar: Polar::Foil(naca0018()),
            strut_chord: 0.0,
            strut_cd: 0.06,
            n_struts: 2,
            oswald: 0.9,
            helical: true,
            troposkien: false,
        },
        parasitic: 0.05,
    })
}

// --- 11. Darrieus troposkien
fn build_troposkien(r: f64) -> Aero {
    let h = 1.6 * r;
    Aero::Darrieus(VawtSpec {
        rotor: DarrieusRotor {
            radius: r,
            height: h,
            n_blades: 2,
            chord: 0.075 * r,
            polar: Polar::Foil(naca0018()),
            strut_chord: 0.0,
            strut_cd: 0.06,
            n_struts: 0,
            oswald: 0.9,
            helical: false,
            troposkien: true,
        },
        parasitic: 0.05,
    })
}

// --- 12. Savonius
fn build_savonius(r: f64) -> Aero {
    Aero::Savonius(SavoniusSpec {
        radius: r,
        height: 2.0 * r,
        n_buckets: 2,
        arc_deg: 135.0,
        overlap: 0.15,
        endplate_gain: 1.15,
    })
}

// --- 13. hybryda Darrieus + Savonius
fn build_hybrid(r: f64) -> Aero {
    let h = 1.5 * r;
    let d = DarrieusRotor {
        radius: r,
        height: h,
        n_blades: 3,
        chord: 0.06 * r,
        polar: Polar::Foil(naca0018()),
        strut_chord: 0.0125 * r,
        strut_cd: 0.06,
        n_struts: 6,
        oswald: 0.9,
        helical: false,
        troposkien: false,
    };
    let s = SavoniusSpec {
        radius: 0.35 * r,
        height: 0.7 * h,
        n_buckets: 2,
        arc_deg: 135.0,
        overlap: 0.15,
        endplate_gain: 1.1,
    };
    Aero::Hybrid(HybridConfig {
        darrieus: VawtSpec {
            rotor: d,
            parasitic: 0.05,
        },
        savonius: s,
        interference: 0.12,
        sav_share: 0.55,
    })
}

// --- 14. Magnus (wirujące cylindry)
fn build_magnus(r: f64) -> Aero {
    Aero::Hawt(HawtSpec {
        rotor: HawtRotor {
            radius: r,
            hub_frac: 0.2,
            n_blades: 3,
            chord_root_frac: 0.12,
            chord_tip_frac: 0.10,
            lambda_design: 3.5,
            alpha_design_deg: 0.0,
            polar: Polar::Magnus(Magnus::typical()),
            thickness_frac: 1.0, // cylinder: "grubość" = średnica
        },
        magnus: true,
        spin_ratio: 1.2,
        counter_rotating: false,
        counter_gain: 0.0,
        parasitic: 0.04,
    })
}

// --- 15. bezwirnikowy (VIV)
fn build_bladeless(scale: f64) -> Aero {
    Aero::Bladeless(VivSpec {
        diameter: 0.12 * scale,
        height: 2.6 * scale,
        strouhal: 0.2,
        v_lock: 11.0,
        bandwidth: 0.15,
        cp_peak: 0.09,
        amplitude_frac: 0.6,
        tunable: true,
    })
}

// --- 16. latawiec crosswind (AWE)
fn build_kite(wing: f64) -> Aero {
    Aero::Kite(KiteSpec {
        wing_area: wing,
        cl: 1.4,
        glide: 14.0,
        altitude: 300.0,
        tether_len: 1.35 * 300.0,
        cycle_eff: 0.62,
        tether_eff: 0.85,
        availability: 0.72,
        wing_areal_mass: 3.2,
    })
}

// --- 17. taśma flatterowa
fn build_belt(scale: f64) -> Aero {
    Aero::Belt(BeltSpec {
        length: scale,
        amplitude: 0.06 * scale,
        width: 0.08 * scale,
        cp_max: 0.035,
        lambda_opt: 0.9,
        lambda_fw: 2.2,
    })
}

/// Pełny katalog badanych wariantów.
pub fn catalog() -> Vec<Design> {
    vec![
        Design {
            id: "hawt3",
            name: "Trójłopatowy HAWT (punkt odniesienia)",
            en: "3-blade upwind HAWT",
            family: Family::Hawt,
            scale_kind: ScaleKind::Radius,
            scales: &[5.0, 15.0, 40.0, 63.0, 90.0],
            hub_base: 10.0,
            hub_per_scale: 1.25,
            ctrl: Ctrl::rotor(3.0, 12.0, 90.0, 12.0),
            spacing_x: 5.0,
            spacing_y: 3.0,
            confidence: "wysoka (BEMT + dane referencyjne NREL 5 MW)",
            note: "Standard branżowy: dobry kompromis Cp/koszt/hałas.",
            build: build_hawt3,
        },
        Design {
            id: "hawt2",
            name: "Dwułopatowy HAWT",
            en: "2-blade HAWT",
            family: Family::Hawt,
            scale_kind: ScaleKind::Radius,
            scales: &[5.0, 15.0, 40.0, 63.0, 90.0],
            hub_base: 10.0,
            hub_per_scale: 1.25,
            ctrl: Ctrl::rotor(4.0, 15.0, 95.0, 12.0),
            spacing_x: 5.0,
            spacing_y: 3.0,
            confidence: "wysoka (BEMT)",
            note: "O 1/3 mniej łopat i lżejsza piasta; wymaga wyższego lambda i ma gorszą dynamikę (wahadłowość).",
            build: build_hawt2,
        },
        Design {
            id: "hawt1",
            name: "Jednołopatowy z przeciwwagą",
            en: "1-blade + counterweight",
            family: Family::Hawt,
            scale_kind: ScaleKind::Radius,
            scales: &[2.0, 5.0, 15.0, 40.0],
            hub_base: 8.0,
            hub_per_scale: 1.3,
            ctrl: Ctrl::rotor(6.0, 20.0, 100.0, 12.5),
            spacing_x: 5.0,
            spacing_y: 3.0,
            confidence: "średnia (BEMT + kara za niewyważenie)",
            note: "Najmniej materiału w wirniku, ale bardzo wysokie lambda i problemy z hałasem/zmęczeniem.",
            build: build_hawt1,
        },
        Design {
            id: "hawt6",
            name: "Sześciołopatowy (wolnoobrotowy)",
            en: "6-blade low-speed HAWT",
            family: Family::Hawt,
            scale_kind: ScaleKind::Radius,
            scales: &[5.0, 15.0, 40.0, 63.0],
            hub_base: 8.0,
            hub_per_scale: 1.1,
            ctrl: Ctrl::rotor(2.0, 7.0, 70.0, 12.0),
            spacing_x: 4.0,
            spacing_y: 3.0,
            confidence: "wysoka (BEMT)",
            note: "Większy moment rozruchowy, cichsza praca; więcej łopat do kupienia i serwisu.",
            build: build_hawt6,
        },
        Design {
            id: "farm18",
            name: "Wiatrak farmerski (18 łopat z blachy)",
            en: "18-blade multi-blade windmill",
            family: Family::Hawt,
            scale_kind: ScaleKind::Radius,
            scales: &[1.5, 3.0, 5.0, 8.0],
            hub_base: 6.0,
            hub_per_scale: 1.6,
            ctrl: Ctrl {
                lambda_lo: 0.6,
                lambda_hi: 3.0,
                tip_speed_max: 45.0,
                pitch_control: false,
                v_rated: 11.0,
                v_in: 2.5,
                v_out: 20.0,
                spin_lo: 0.0,
                spin_hi: 0.0,
            },
            spacing_x: 3.0,
            spacing_y: 2.0,
            confidence: "średnia (BEMT, duża solidność)",
            note: "Bardzo tanie łopaty i duży moment rozruchowy, ale niskie lambda i marna sprawność przy dużej skali.",
            build: build_farm18,
        },
        Design {
            id: "dawt",
            name: "Wirnik w dyfuzorze z kołnierzem (DAWT)",
            en: "Flanged diffuser-augmented turbine",
            family: Family::Ducted,
            scale_kind: ScaleKind::Radius,
            scales: &[1.5, 3.0, 5.0, 10.0, 20.0],
            hub_base: 4.0,
            hub_per_scale: 1.4,
            ctrl: Ctrl::rotor(2.0, 9.0, 80.0, 12.0),
            spacing_x: 3.0,
            spacing_y: 3.0,
            confidence: "średnia (BEMT + model augmentacji)",
            note: "Może przekroczyć 'Betz na pole wirnika' (duże beta), ale dyfuzor waży i kosztuje jak druga wieża.",
            build: build_dawt,
        },
        Design {
            id: "counter",
            name: "Przeciwbieżny dwuwirnikowy",
            en: "Counter-rotating dual rotor",
            family: Family::Hawt,
            scale_kind: ScaleKind::Radius,
            scales: &[15.0, 40.0, 63.0, 90.0],
            hub_base: 10.0,
            hub_per_scale: 1.25,
            ctrl: Ctrl::rotor(3.0, 12.0, 90.0, 12.0),
            spacing_x: 5.0,
            spacing_y: 3.0,
            confidence: "średnia (BEMT + odzysk wiru)",
            note: "Drugi wirnik odzyskuje część energii ruchu wirowego: +kilkanaście procent mocy za cenę przekładni.",
            build: build_counter,
        },
        Design {
            id: "venturi",
            name: "Lej Venturiego (kolektor zasysający)",
            en: "Venturi / INVELOX-type collector",
            family: Family::Venturi,
            scale_kind: ScaleKind::Radius,
            scales: &[3.0, 8.0, 15.0, 30.0],
            hub_base: 0.0,
            hub_per_scale: 1.2,
            ctrl: Ctrl::rotor(2.0, 9.0, 80.0, 12.0),
            spacing_x: 3.0,
            spacing_y: 3.0,
            confidence: "niska (model koncentracji z ograniczeniem Betza)",
            note: "Kuszący marketingowo; w praktyce ogranicza go bilans energii - sprawność liczona na pole wlotu jest niska, a konstrukcja ciężka.",
            build: build_venturi,
        },
        Design {
            id: "hrotor",
            name: "Darrieus H-rotor (3 proste łopaty)",
            en: "H-rotor Darrieus VAWT",
            family: Family::VawtLift,
            scale_kind: ScaleKind::Radius,
            scales: &[3.0, 8.0, 20.0, 40.0],
            hub_base: 3.0,
            hub_per_scale: 0.75,
            ctrl: Ctrl::rotor(1.5, 7.0, 80.0, 13.0),
            spacing_x: 2.5,
            spacing_y: 1.5,
            confidence: "średnia/wysoka (DMST)",
            note: "Generator przy ziemi, brak yawowania, możliwe gęste ustawienie; kosztem niższego Cp i problemów ze startem.",
            build: build_hrotor,
        },
        Design {
            id: "helical",
            name: "Darrieus helikalny (skręcone łopaty)",
            en: "Helical Darrieus",
            family: Family::VawtLift,
            scale_kind: ScaleKind::Radius,
            scales: &[3.0, 8.0, 20.0, 40.0],
            hub_base: 3.0,
            hub_per_scale: 0.75,
            ctrl: Ctrl::rotor(1.5, 7.0, 80.0, 13.0),
            spacing_x: 2.5,
            spacing_y: 1.5,
            confidence: "średnia (DMST + czynnik skosu)",
            note: "Równy moment i dużo mniejsze zmęczenie; nieco niższy Cp szczytowy i droższe łopaty.",
            build: build_helical,
        },
        Design {
            id: "troposkien",
            name: "Darrieus troposkien („ubijaczka”)",
            en: "Troposkien (egg-beater) Darrieus",
            family: Family::VawtLift,
            scale_kind: ScaleKind::Radius,
            scales: &[3.0, 8.0, 20.0, 40.0],
            hub_base: 3.0,
            hub_per_scale: 0.8,
            ctrl: Ctrl::rotor(1.5, 7.0, 80.0, 13.0),
            spacing_x: 2.5,
            spacing_y: 1.5,
            confidence: "średnia (DMST + czynnik kształtu)",
            note: "Łopata pracuje głównie na rozciąganie (tańsza struktura), ale produkcja giętych łopat jest droga.",
            build: build_troposkien,
        },
        Design {
            id: "savonius",
            name: "Savonius (dwie łyżki)",
            en: "Savonius drag rotor",
            family: Family::VawtDrag,
            scale_kind: ScaleKind::Radius,
            scales: &[0.5, 1.5, 3.0, 6.0],
            hub_base: 1.5,
            hub_per_scale: 1.2,
            ctrl: Ctrl {
                lambda_lo: 0.05,
                lambda_hi: 2.2,
                tip_speed_max: 40.0,
                pitch_control: false,
                v_rated: 11.0,
                v_in: 2.0,
                v_out: 22.0,
                spin_lo: 0.0,
                spin_hi: 0.0,
            },
            spacing_x: 2.0,
            spacing_y: 1.5,
            confidence: "średnia (krzywa empiryczna z literatury)",
            note: "Najprostszy możliwy wirnik: sam startuje, ogromny moment, ale Cp rzędu 0.15-0.25 i duża masa przy skali.",
            build: build_savonius,
        },
        Design {
            id: "hybrid",
            name: "Hybryda Darrieus + Savonius",
            en: "Darrieus-Savonius combined rotor",
            family: Family::VawtLift,
            scale_kind: ScaleKind::Radius,
            scales: &[1.5, 3.0, 8.0, 20.0],
            hub_base: 3.0,
            hub_per_scale: 0.75,
            ctrl: Ctrl::rotor(0.3, 6.0, 80.0, 13.0),
            spacing_x: 2.5,
            spacing_y: 1.5,
            confidence: "średnia (DMST + składnik oporowy)",
            note: "Samorozruch bez silnika; okupione stratą kilkunastu procent Cp i większą masą.",
            build: build_hybrid,
        },
        Design {
            id: "magnus",
            name: "Wirnik Magnusa (obracane cylindry)",
            en: "Magnus-effect rotor",
            family: Family::Magnus,
            scale_kind: ScaleKind::Radius,
            scales: &[2.0, 6.0, 15.0, 30.0],
            hub_base: 6.0,
            hub_per_scale: 1.2,
            ctrl: Ctrl {
                lambda_lo: 0.5,
                lambda_hi: 6.0,
                tip_speed_max: 80.0,
                pitch_control: true,
                v_rated: 12.0,
                v_in: 2.5,
                v_out: 25.0,
                spin_lo: 0.4,
                spin_hi: 2.5,
            },
            spacing_x: 4.0,
            spacing_y: 3.0,
            confidence: "niska/średnia (BEMT + biegunowa cylindra)",
            note: "Bardzo duża siła nośna cylindra, ale też duży opór i koszt napędu obrotu: netto wychodzi słabo.",
            build: build_magnus,
        },
        Design {
            id: "bladeless",
            name: "Bezwirnikowy rezonans wirowy (VIV)",
            en: "Bladeless vortex-induced vibration",
            family: Family::Bladeless,
            scale_kind: ScaleKind::Radius,
            scales: &[1.0, 2.0, 4.0, 6.0],
            hub_base: 0.0,
            hub_per_scale: 1.6,
            ctrl: Ctrl {
                lambda_lo: 0.0,
                lambda_hi: 0.0,
                tip_speed_max: 0.0,
                pitch_control: false,
                v_rated: 12.0,
                v_in: 2.0,
                v_out: 25.0,
                spin_lo: 0.0,
                spin_hi: 0.0,
            },
            spacing_x: 1.5,
            spacing_y: 1.5,
            confidence: "niska (model pasma synchronizacji Strouhala)",
            note: "Zero łopat i minimalny koszt wytworzenia, ale wąskie pasmo wiatru: bardzo niskie wykorzystanie roczne.",
            build: build_bladeless,
        },
        Design {
            id: "kite",
            name: "Latawiec crosswind (AWE)",
            en: "Crosswind kite (airborne wind energy)",
            family: Family::Airborne,
            scale_kind: ScaleKind::WingArea,
            scales: &[10.0, 25.0, 50.0, 100.0],
            hub_base: 0.0,
            hub_per_scale: 0.0,
            ctrl: Ctrl {
                lambda_lo: 0.0,
                lambda_hi: 0.0,
                tip_speed_max: 0.0,
                pitch_control: false,
                v_rated: 14.0,
                v_in: 4.0,
                v_out: 28.0,
                spin_lo: 0.0,
                spin_hi: 0.0,
            },
            spacing_x: 0.0,
            spacing_y: 0.0,
            confidence: "średnia (wzór Loyda + czynniki cyklu)",
            note: "Moc z powierzchni skrzydła jest ogromna (lot crosswind + wysoki wiatr na 300 m); wyzwaniem jest sterowanie i dyspozycyjność.",
            build: build_kite,
        },
        Design {
            id: "belt",
            name: "Taśma flatterowa (windbelt)",
            en: "Aeroelastic flutter belt",
            family: Family::Belt,
            scale_kind: ScaleKind::Radius,
            scales: &[0.5, 1.0, 2.0],
            hub_base: 1.0,
            hub_per_scale: 0.5,
            ctrl: Ctrl {
                lambda_lo: 0.1,
                lambda_hi: 2.5,
                tip_speed_max: 0.0,
                pitch_control: false,
                v_rated: 10.0,
                v_in: 2.0,
                v_out: 20.0,
                spin_lo: 0.0,
                spin_hi: 0.0,
            },
            spacing_x: 1.2,
            spacing_y: 1.0,
            confidence: "niska (krzywa empiryczna)",
            note: "Ekstremalnie tanie, ale sprawność rzędu pojedynczych procent - sens tylko w mikroinstalacjach.",
            build: build_belt,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_design_builds_and_produces_sane_area() {
        for d in catalog() {
            for &s in d.scales {
                let a = d.instantiate(s);
                let area = a.area();
                assert!(area > 0.01 && area < 1.0e7, "{}: area={}", d.id, area);
                let g = d.geometry(s);
                assert!(g.hub > 0.0 || d.family == Family::Airborne);
            }
        }
    }

    #[test]
    fn three_blade_beats_one_blade_on_cp() {
        let a = build_hawt3(50.0);
        let b = build_hawt1(50.0);
        let g = Grid::normal();
        let f = |sp: &Aero, l: f64, des: f64| {
            let mut sp = sp.clone();
            sp.set_design_lambda(des);
            sp.evaluate(10.0, l, &g, RHO).cp
        };
        let cp3 = f(&a, 7.0, 7.0);
        let cp1 = f(&b, 13.0, 13.0);
        assert!(cp3 > cp1, "cp3={cp3} cp1={cp1}");
        assert!(cp3 > 0.40 && cp3 < 0.60, "cp3={cp3}");
    }

    #[test]
    fn darrieus_catalog_variant_beats_savonius_on_cp() {
        let g = Grid::normal();
        let darr = build_hrotor(20.0);
        let sav = build_savonius(20.0);
        let mut best_d = f64::MIN;
        let mut l = 1.5;
        while l < 7.0 {
            best_d = best_d.max(darr.evaluate(10.0, l, &g, RHO).cp);
            l += 0.1;
        }
        let mut best_s = f64::MIN;
        let mut l = 0.05;
        while l < 2.2 {
            best_s = best_s.max(sav.evaluate(10.0, l, &g, RHO).cp);
            l += 0.05;
        }
        assert!(best_d > best_s, "darrieus={best_d} savonius={best_s}");
        assert!(best_d > 0.25 && best_d < 0.55, "darrieus={best_d}");
        assert!(best_s > 0.08 && best_s < 0.30, "savonius={best_s}");
    }
}
