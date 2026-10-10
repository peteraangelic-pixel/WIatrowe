//! Model masy i kosztu (CAPEX / OPEX / LCOE).
//!
//! Założenia są jawne i wszystkie stałe są w jednym miejscu (`K`), żeby można
//! było je podważać i skalibrować. Kalibracja została dobrana tak, żeby bazowy
//! HAWT 3-łopatowy (R = 63 m, ~5 MW) wypadał w okolicach 5 000-6 000 PLN/kW
//! CAPEX-u i 200-350 PLN/MWh LCOE, czyli w typowym zakresie dla onshore
//! (2024/2025). Pozostałe warianty liczone są tymi samymi regułami.
//!
//! Prawa skalowania użyte w modelu:
//!   * łopata kompozytowa:  m = 0.55 * R^2.5  [kg]  (NREL 5 MW: ~17.7 t dla R=63),
//!   * wieża: masa z momentu wywracającego  M = T * H  i dopuszczalnego
//!     naprężenia stali (NREL 5 MW -> ~340 t, model daje ~320 t),
//!   * fundament: 8.5 kg betonu na każdy kNm momentu wywracającego
//!     (NREL 5 MW -> ~600 t),
//!   * przekładnia + generator: ~45 kg/kW na wale (48 kg/kW dla NREL 5 MW).

use crate::designs::{Aero, Design, Family, Geometry};

/// Zestaw stałych kosztowych [PLN, kg, lata].
pub struct K;

impl K {
    // ---- ceny jednostkowe [PLN/kg] -----------------------------------------
    /// Łopata kompozytowa (materiał + robocizna + amortyzacja form).
    pub const BLADE_COMPOSITE: f64 = 52.0;
    /// Łopata z blachy / walcowana (tanie wirniki).
    pub const BLADE_STEEL: f64 = 15.0;
    /// Łopata VAWT (wytłaczane aluminium / kompozyt o stałej cięciwie).
    pub const BLADE_VAWT: f64 = 38.0;
    /// Cylinder Magnusa (powłoka + silniki + tarcze końcowe).
    pub const MAGNUS_SHELL: f64 = 46.0;
    /// Piasta, łoża, mechanizm nastawiania łopat.
    pub const HUB: f64 = 18.0;
    /// Przekładnia + generator + łoże (na wale, wysokie wymagania masowe).
    pub const DRIVETRAIN_AIRBORNE: f64 = 30.0;
    /// Przekładnia + generator przy ziemi (cięższe, ale tańsze za kg).
    pub const DRIVETRAIN_GROUND: f64 = 20.0;
    /// Stalowa wieża / maszt (prefabrykacja).
    pub const TOWER: f64 = 8.0;
    /// Poszycie dyfuzora / leja + konstrukcja wsporcza.
    pub const DUCT: f64 = 24.0;
    /// Fundament (beton + zbrojenie + robocizna).
    pub const FOUNDATION: f64 = 0.95;
    /// Lina nośna (Dyneema / UHMWPE).
    pub const TETHER: f64 = 90.0;
    /// Skrzydło latawca (tkanina techniczna, taśmy, sterowanie).
    pub const WING: f64 = 320.0;
    /// Wirnik bezwirnikowy (walec z kompozytu + układ strojenia).
    pub const VIV_BODY: f64 = 34.0;
    /// Taśma flatterowa + przetwornik liniowy.
    pub const BELT: f64 = 60.0;

    // ---- elektronika i BOS -------------------------------------------------
    /// Energoelektronika, transformator, sterowanie [PLN/kW].
    pub const ELECTRONICS: f64 = 320.0;
    /// Przyłącze, projekt, droga dojazdowa - część stała [PLN].
    pub const BOS_FIXED: f64 = 90_000.0;
    /// Część zmienna BOS (montaż, dźwig, przyłącze) [PLN/kW].
    pub const BOS_PER_KW: f64 = 1_050.0;

    // ---- eksploatacja ------------------------------------------------------
    /// Bazowy OPEX [PLN/kW/rok] dla klasycznego HAWT-a.
    pub const OPEX_BASE: f64 = 95.0;
    /// Współczynnik dyskontowy (WACC).
    pub const WACC: f64 = 0.07;
    /// Globalna kalibracja poziomu kosztów (1.0 = czysty model).
    pub const CALIBRATION: f64 = 1.06;

    // ---- materiały / wytrzymałość -----------------------------------------
    /// Gęstość stali [kg/m^3].
    pub const RHO_STEEL: f64 = 7850.0;
    /// Dopuszczalne naprężenie zmęczeniowe stali [Pa].
    pub const SIGMA_STEEL: f64 = 150.0e6;
    /// Masa fundamentu na kNm momentu wywracającego [kg/kNm].
    pub const FOUNDATION_PER_KNM: f64 = 8.5;

    /// Czas życia [lata] w zależności od rodziny.
    pub fn lifetime(f: Family) -> f64 {
        match f {
            Family::Hawt => 25.0,
            Family::Ducted => 20.0,
            Family::Venturi => 20.0,
            Family::VawtLift => 20.0,
            Family::VawtDrag => 15.0,
            Family::Magnus => 15.0,
            Family::Bladeless => 15.0,
            Family::Airborne => 12.0,
            Family::Belt => 8.0,
        }
    }

    /// Mnożnik dostępności serwisowej dla OPEX-u.
    pub fn opex_factor(f: Family) -> f64 {
        match f {
            Family::Hawt => 1.00,
            Family::Ducted => 1.15,
            Family::Venturi => 1.10,
            Family::VawtLift => 0.60, // generator przy ziemi
            Family::VawtDrag => 0.55,
            Family::Magnus => 1.30, // napęd obrotu cylindrów
            Family::Bladeless => 0.35,
            Family::Airborne => 1.50, // operacje lotne, liny
            Family::Belt => 0.30,
        }
    }

    /// Jak trudny jest montaż (1.0 = pełny dźwig wieżowy).
    pub fn install_factor(f: Family) -> f64 {
        match f {
            Family::Hawt => 1.00,
            Family::Ducted => 1.05,
            Family::Venturi => 0.70,
            Family::VawtLift => 0.35,
            Family::VawtDrag => 0.30,
            Family::Magnus => 0.95,
            Family::Bladeless => 0.20,
            Family::Airborne => 0.35,
            Family::Belt => 0.15,
        }
    }
}

/// Podział masy [kg].
#[derive(Clone, Copy, Debug, Default)]
pub struct MassBreakdown {
    pub blades: f64,
    pub hub: f64,
    pub drivetrain: f64,
    pub tower: f64,
    pub duct: f64,
    pub foundation: f64,
    pub tether: f64,
    pub total: f64,
}

/// Wynik ekonomiczny.
#[derive(Clone, Copy, Debug, Default)]
pub struct Economics {
    pub rated_kw: f64,
    pub mass_blades: f64,
    pub mass_total: f64,
    pub capex_equipment: f64,
    pub capex_bos: f64,
    pub capex_total: f64,
    pub opex_year: f64,
    pub lifetime_y: f64,
    pub aep_kwh: f64,
    pub capacity_factor: f64,
    pub lcoe: f64,
    pub pln_per_kw: f64,
    pub kwh_per_kg: f64,
    pub m2_per_mwh: f64,
}

/// Masa łopaty kompozytowej: kalibracja m = 0.55 * R^2.5 kg dla referencyjnej
/// cięciwy; dla innych cięciw skalujemy powierzchnią.
pub fn composite_blade_mass(radius: f64, planform_area: f64) -> f64 {
    let a_ref = 0.0433 * radius * radius;
    let areal = if a_ref > 0.0 {
        (0.55 * radius.powf(2.5)) / a_ref
    } else {
        40.0
    };
    (areal * planform_area).max(0.5)
}

/// Masa wieży/masztu z momentu wywracającego (M = T * H, dopuszczalne
/// naprężenie + promień wieży ~ 0.016 D).
pub fn tower_mass(thrust_n: f64, hub_height: f64, diameter: f64) -> f64 {
    if hub_height <= 0.5 {
        return 0.0;
    }
    let r = (0.016 * diameter).max(0.15);
    let m = 1.25 * thrust_n * hub_height; // +25% na obciążenie wiatrem samej wieży
    let mass = 2.0 * m * hub_height * K::RHO_STEEL / (K::SIGMA_STEEL * r);
    mass.max(8.0 * hub_height.powf(1.2))
}

/// Masy poszczególnych zespołów.
pub fn masses(design: &Design, aero: &Aero, g: &Geometry, rated_w: f64, thrust_n: f64) -> MassBreakdown {
    let rated_kw = rated_w / 1000.0;
    let mut mb = MassBreakdown::default();

    match aero {
        Aero::Hawt(s) => {
            if s.magnus {
                // Cylindry: powłoka z aluminium/kompozytu + silniki + tarcze.
                let d = s.rotor.chord(0.6);
                let span = s.rotor.radius * 0.8;
                let t = 0.006 * (d / 1.0).max(0.2).powf(0.7);
                mb.blades = std::f64::consts::PI * d * span * t * 2700.0 * s.rotor.n_blades as f64
                    * 1.45; // tarcze końcowe + silniki
            } else if design.id == "farm18" {
                mb.blades = g.blade_area * 11.0 * (s.rotor.radius / 3.0).max(0.5).powf(0.35);
            } else {
                mb.blades = composite_blade_mass(s.rotor.radius, g.blade_area);
            }
            // Piasta i łoża; przy jednej łopacie dochodzi ciężka przeciwwaga.
            let hub_f = if s.rotor.n_blades == 1 { 1.55 } else { 0.95 };
            mb.hub = hub_f * mb.blades;
            mb.drivetrain = 45.0 * rated_kw;
            mb.tower = tower_mass(thrust_n, g.hub, g.diameter);
        }
        Aero::Ducted(s) => {
            mb.blades = composite_blade_mass(s.rotor.radius, g.blade_area);
            mb.hub = 0.9 * mb.blades;
            mb.drivetrain = 45.0 * rated_kw;
            let areal = 18.0 * (s.rotor.radius / 3.0).max(0.5).powf(0.5);
            mb.duct = g.duct_area * areal * 1.6;
            // Dyfuzor przenosi dużą część obciążenia - maszt liczymy od naporu.
            mb.tower = tower_mass(thrust_n, g.hub, g.diameter * s.beta.sqrt());
        }
        Aero::Venturi(_) => {
            mb.drivetrain = 45.0 * rated_kw;
            mb.duct = g.duct_area * 22.0;
            mb.tower = tower_mass(thrust_n, g.hub.max(3.0), g.diameter);
        }
        Aero::Darrieus(s) => {
            let areal = 22.0 * (s.rotor.radius / 20.0).max(0.4).powf(0.35);
            mb.blades = g.blade_area * areal;
            // Wał centralny + ramiona
            mb.hub = 30.0 * s.rotor.height * (s.rotor.radius / 20.0).max(0.3).powf(0.8)
                + (s.rotor.strut_chord * s.rotor.radius * s.rotor.n_struts as f64 * 7800.0 * 0.35);
            mb.drivetrain = 50.0 * rated_kw; // cięższe, ale przy ziemi
            mb.tower = tower_mass(thrust_n, (g.hub * 0.6).max(1.0), g.diameter);
        }
        Aero::Savonius(s) => {
            mb.blades = g.blade_area * 10.0 * (s.radius / 2.0).max(0.4).powf(0.4);
            mb.hub = 0.25 * mb.blades + 12.0 * s.height;
            mb.drivetrain = 32.0 * rated_kw;
            mb.tower = tower_mass(thrust_n, (g.hub * 0.6).max(1.0), g.diameter);
        }
        Aero::Hybrid(h) => {
            let areal = 22.0 * (h.darrieus.rotor.radius / 20.0).max(0.4).powf(0.35);
            mb.blades = h.darrieus.rotor.blade_area() * areal
                + h.savonius.blade_area() * 10.0 * (h.savonius.radius / 2.0).max(0.3).powf(0.4);
            mb.hub = 30.0 * h.darrieus.rotor.height
                * (h.darrieus.rotor.radius / 20.0).max(0.3).powf(0.8);
            mb.drivetrain = 50.0 * rated_kw;
            mb.tower = tower_mass(thrust_n, (g.hub * 0.6).max(1.0), g.diameter);
        }
        Aero::Bladeless(s) => {
            mb.blades = std::f64::consts::PI * s.diameter * s.height * 0.010 * 1900.0 * 1.4;
            mb.hub = 0.15 * mb.blades;
            mb.drivetrain = 9.0 * rated_kw; // przetwornik liniowy, mały
            mb.tower = 0.0; // stoi na fundamentach
        }
        Aero::Kite(k) => {
            mb.blades = k.wing_area * k.wing_areal_mass;
            mb.hub = 0.0;
            mb.drivetrain = 26.0 * rated_kw; // wciągarka + generator na ziemi
            mb.tether = k.tether_len * 0.55; // kg/m liny nośnej
            mb.tower = 0.0;
        }
        Aero::Belt(s) => {
            mb.blades = s.length * s.width * 3.5;
            mb.drivetrain = 6.0 * rated_kw;
            mb.tower = 8.0 * g.hub.max(1.0);
        }
    }

    // Fundament: z momentu wywracającego (lub minimalna płyta).
    let moment_knm = thrust_n * g.hub.max(1.0) / 1000.0;
    mb.foundation = match aero {
        Aero::Kite(_) => 6_000.0,
        Aero::Bladeless(_) => (0.6 * thrust_n / 9.81).max(150.0),
        Aero::Belt(_) => 150.0,
        Aero::Darrieus(_) | Aero::Savonius(_) | Aero::Hybrid(_) => {
            (K::FOUNDATION_PER_KNM * moment_knm * 0.35).max(400.0)
        }
        _ => (K::FOUNDATION_PER_KNM * moment_knm).max(2_000.0),
    };

    mb.total = mb.blades + mb.hub + mb.drivetrain + mb.tower + mb.duct + mb.foundation + mb.tether;
    mb
}

/// Koszt z podziałem na wyposażenie i BOS.
pub fn capex(design: &Design, aero: &Aero, g: &Geometry, mb: &MassBreakdown, rated_w: f64) -> (f64, f64, f64) {
    let rated_kw = rated_w / 1000.0;
    let ground_drive = matches!(aero, Aero::Darrieus(_) | Aero::Savonius(_) | Aero::Hybrid(_));
    let drive_price = if ground_drive {
        K::DRIVETRAIN_GROUND
    } else {
        K::DRIVETRAIN_AIRBORNE
    };

    let blade_price = match aero {
        Aero::Hawt(s) => {
            if s.magnus {
                K::MAGNUS_SHELL
            } else if design.id == "farm18" {
                K::BLADE_STEEL
            } else {
                K::BLADE_COMPOSITE
            }
        }
        Aero::Ducted(_) => K::BLADE_COMPOSITE,
        Aero::Darrieus(_) | Aero::Hybrid(_) => K::BLADE_VAWT,
        Aero::Savonius(_) => K::BLADE_STEEL,
        Aero::Bladeless(_) => K::VIV_BODY,
        Aero::Kite(_) => K::WING,
        Aero::Belt(_) => K::BELT,
        Aero::Venturi(_) => K::BLADE_COMPOSITE,
    };

    let mut equipment = mb.blades * blade_price
        + mb.hub * K::HUB
        + mb.drivetrain * drive_price
        + mb.tower * K::TOWER
        + mb.duct * K::DUCT
        + mb.tether * K::TETHER
        + rated_kw * K::ELECTRONICS;

    // Fundament liczony osobno (wchodzi do BOS, bo to robota budowlana).
    let civil = mb.foundation * K::FOUNDATION;

    // Konstrukcje bez regulacji kąta łopat muszą przenosić większe obciążenia
    // ekstremalne - kara masowa została już policzona przez wyższy Ct,
    // tu dodajemy karę za "przewymiarowanie" układu hamowania/odciążania.
    if !design.ctrl.pitch_control {
        equipment *= 1.06;
    }

    let height_factor = 1.0 + 0.006 * (g.hub - 80.0).max(0.0);
    let bos = K::BOS_FIXED.min(0.35 * equipment)
        + K::BOS_PER_KW * rated_kw * K::install_factor(design.family) * height_factor;

    equipment *= K::CALIBRATION;
    (equipment, civil + bos, equipment + civil + bos)
}

/// Kompletny wynik ekonomiczny.
pub fn economics(
    design: &Design,
    aero: &Aero,
    g: &Geometry,
    rated_w: f64,
    thrust_n: f64,
    aep_kwh: f64,
) -> Economics {
    let mb = masses(design, aero, g, rated_w, thrust_n);
    let (eq, bos, total) = capex(design, aero, g, &mb, rated_w);
    let years = K::lifetime(design.family);
    let r = K::WACC;
    let crf = if years > 0.5 {
        r * (1.0 + r).powf(years) / ((1.0 + r).powf(years) - 1.0)
    } else {
        1.0
    };
    let opex = K::OPEX_BASE * K::opex_factor(design.family) * (rated_w / 1000.0);
    let annual_total = total * crf + opex;
    let aep_mwh = aep_kwh / 1000.0;
    let lcoe = if aep_mwh > 1e-6 {
        annual_total / aep_mwh
    } else {
        f64::INFINITY
    };
    let rated_kw = rated_w / 1000.0;
    Economics {
        rated_kw,
        mass_blades: mb.blades,
        mass_total: mb.total,
        capex_equipment: eq,
        capex_bos: bos,
        capex_total: total,
        opex_year: opex,
        lifetime_y: years,
        aep_kwh,
        capacity_factor: if rated_kw > 0.0 {
            aep_kwh / (rated_kw * 8760.0)
        } else {
            0.0
        },
        lcoe,
        pln_per_kw: if rated_kw > 0.0 {
            total / rated_kw
        } else {
            f64::INFINITY
        },
        kwh_per_kg: if mb.total > 0.0 {
            aep_kwh / mb.total
        } else {
            0.0
        },
        m2_per_mwh: if aep_mwh > 1e-6 && g.footprint > 0.0 {
            g.footprint / aep_mwh
        } else {
            0.0
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::designs::catalog;

    fn find(id: &str) -> Design {
        catalog()
            .into_iter()
            .find(|d| d.id == id)
            .unwrap_or_else(|| panic!("brak {id}"))
    }

    #[test]
    fn baseline_utility_hawt_lands_in_known_cost_band() {
        let d = find("hawt3");
        let scale = 63.0;
        let aero = d.instantiate(scale);
        let g = d.geometry(scale);
        // warunki znamionowe: V=11.5 m/s, Cp=0.48, Ct=0.75
        let rated = 0.5 * 1.225 * g.area * 11.5f64.powi(3) * 0.48;
        let thrust = 0.5 * 1.225 * g.area * 11.5f64.powi(2) * 0.75;
        assert!((rated / 1e6 - 5.4).abs() < 1.2, "rated = {} MW", rated / 1e6);
        let e = economics(&d, &aero, &g, rated, thrust, rated * 0.32 * 8760.0 / 1000.0);
        assert!(e.pln_per_kw > 3_000.0 && e.pln_per_kw < 9_000.0, "PLN/kW = {}", e.pln_per_kw);
        assert!(e.mass_blades / 1000.0 > 30.0 && e.mass_blades / 1000.0 < 90.0,
            "masa łopat = {} t", e.mass_blades / 1000.0);
        // LCOE dla dobrego stanowiska (CF ~ 0.35) powinno być 150-400 PLN/MWh
        assert!(e.lcoe > 120.0 && e.lcoe < 500.0, "LCOE = {}", e.lcoe);
    }

    #[test]
    fn tower_mass_matches_nrel_reference() {
        // NREL 5 MW: T ~ 800 kN, H = 90 m, D = 126 m  ->  ~340 t
        let m = tower_mass(800e3, 90.0, 126.0);
        assert!(m > 200e3 && m < 500e3, "wieza = {} t", m / 1000.0);
    }

    #[test]
    fn vawt_uses_much_less_foundation_than_hawt() {
        let d = find("hrotor");
        let scale = 20.0;
        let aero = d.instantiate(scale);
        let g = d.geometry(scale);
        let t = 0.5 * 1.225 * g.area * 12.0 * 12.0 * 0.6;
        let mb = masses(&d, &aero, &g, 300e3, t);
        assert!(mb.foundation < 120e3, "fundament VAWT = {} t", mb.foundation / 1000.0);
    }
}
