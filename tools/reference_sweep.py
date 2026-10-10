#!/usr/bin/env python3
"""
Referencyjna implementacja modelu `wiatrowe` w czystym Pythonie.

Po co:
  1) walidacja krzyżowa - ten sam katalog, te same wzory, te same stałe co w
     Rust; wyniki (Cp na wspólnej siatce) porównywane są w CI
     (`tools/compare_checksums.py`), co wyłapuje literówki w obu wersjach,
  2) benchmark - pokazuje realny zysk z Rust + rayon (Python jest tu
     jednowątkowy, bez numpy, czyli najgorszy realistyczny przypadek).

Uruchomienie:
    python3 tools/reference_sweep.py --mode checksum --out results
    python3 tools/reference_sweep.py --mode bench --repeat 5
"""

import argparse
import math
import os
import time

D2R = math.pi / 180.0
R2D = 180.0 / math.pi
RHO = 1.225
BETZ = 16.0 / 27.0
COUNTER_MAX_GAIN = 0.12  # maksymalny realny przyrost z dwuwirnikowego układu


# ------------------------------------------------------------------ narzędzia --
def clamp(x, lo, hi):
    return lo if x < lo else (hi if x > hi else x)


def smoothstep(t):
    t = clamp(t, 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


# ------------------------------------------------------------------ profile ---
class Foil:
    def __init__(self, cl0=0.35, a0=-3.5, slope=6.0, stall=14.0, sw=8.0,
                 cd0=0.0085, kcd=0.012, cdstall=1.15):
        self.cl0, self.a0, self.slope = cl0, a0, slope
        self.stall, self.sw = stall, sw
        self.cd0, self.kcd, self.cdstall = cd0, kcd, cdstall

    def cl(self, a, k=0.0):
        attached = self.cl0 + self.slope * (a - self.a0) * D2R
        if abs(a) <= self.stall:
            return attached
        t = smoothstep((abs(a) - self.stall) / self.sw)
        return (1.0 - t) * attached + t * 1.1 * math.sin(2.0 * a * D2R)

    def cd(self, a, k=0.0):
        cl = self.cl(a)
        t = smoothstep((abs(a) - self.stall) / self.sw)
        d = cl - self.cl0
        return self.cd0 + self.kcd * d * d + self.cdstall * t * t


def modern():
    return Foil(0.35, -3.5, 6.0, 14.0, 8.0, 0.0085, 0.012, 1.15)


def sheet_metal():
    return Foil(0.20, -2.0, 4.6, 10.0, 7.0, 0.022, 0.030, 1.35)


def naca0018():
    return Foil(0.0, 0.0, 6.0, 13.0, 8.0, 0.010, 0.015, 1.20)


class Magnus:
    def __init__(self, cl_max=5.0, k0=1.5, cd0=0.35, cd_k=0.30, cf=0.010):
        self.cl_max, self.k0 = cl_max, k0
        self.cd0, self.cd_k, self.cf = cd0, cd_k, cf

    def cl(self, a, k):
        return self.cl_max * (1.0 - math.exp(-clamp(k, 0.0, 8.0) / self.k0))

    def cd(self, a, k):
        return self.cd0 + self.cd_k * clamp(k, 0.0, 8.0)


# --------------------------------------------------------------------- BEMT ---
class HawtRotor:
    def __init__(self, radius, hub, n, c_root, c_tip, lam_d, alpha_d, polar,
                 thickness=0.18):
        self.R, self.hub, self.B = radius, hub, n
        self.c_root, self.c_tip = c_root, c_tip
        self.lam_d, self.alpha_d = lam_d, alpha_d
        self.polar = polar
        self.thickness = thickness

    def chord(self, x):
        cr, ct = self.c_root * self.R, self.c_tip * self.R
        return cr if x <= 0.2 else cr + (ct - cr) * (x - 0.2) / 0.8

    def twist(self, x):
        phi = math.atan(2.0 / (3.0 * self.lam_d * max(x, 0.15)))
        return clamp(phi - self.alpha_d * D2R, 0.0, 32.0 * D2R)


class BemtOut:
    __slots__ = ("cp", "ct", "cp_swirl", "cp_spin", "ok")

    def __init__(self, cp=0.0, ct=0.0, sw=0.0, spin=0.0, ok=True):
        self.cp, self.ct, self.cp_swirl, self.cp_spin, self.ok = cp, ct, sw, spin, ok


def _prandtl(arg):
    """Czynnik straty końcówki/piasty (Prandtla)."""
    return (2.0 / math.pi) * math.acos(min(1.0, math.exp(-max(arg, 0.0))))


def bemt(rotor, lam, pitch_deg=0.0, n_st=32, spin=0.0):
    """BEMT: równanie równowagi rozwiązywane w kącie napływu (bisekcja).

    Klasyczna iteracja punktu stałego (a, a') nie zbiega przy dużym
    obciążeniu elementu (wysoka solidność, niskie lambda, głęboki stall) -
    wtedy wynik zależy od kolejności zaokrągleń i jest niepowtarzalny.
    Zamiast tego szukamy kąta napływu phi z równania trójkąta prędkości

        (1 - a) * cos(phi) - lam_r * (1 + ap) * sin(phi) = 0

    przy czym a i ap dla danego phi wyznaczamy wprost z teorii pędu
    (z poprawką Glauerta na stan turbulent wake). Funkcja jest monotoniczna
    w przybliżeniu, więc bisekcja daje wynik deterministyczny i identyczny
    w obu implementacjach (Rust / Python).
    """
    R = rotor.R
    x0 = max(rotor.hub, 0.02)
    dx = (1.0 - x0) / n_st
    B = float(rotor.B)
    lam = max(lam, 1e-3)
    magnus = isinstance(rotor.polar, Magnus)
    pitch = pitch_deg * D2R
    spin_k = spin if magnus else 0.0
    polar = rotor.polar
    hub = rotor.hub
    cp = ct = sw = spin_cp = 0.0
    ok = True

    for i in range(n_st):
        x = x0 + (i + 0.5) * dx
        lam_r = lam * x
        chord = rotor.chord(x)
        sigma = B * chord / (2.0 * math.pi * x * R)
        twist = rotor.twist(x)

        def stan(phi):
            s = max(math.sin(phi), 1e-6)
            c = math.cos(phi)
            alpha = (phi - twist - pitch) * R2D
            cl = polar.cl(alpha, spin_k)
            cd = polar.cd(alpha, spin_k)
            cn = cl * c + cd * s
            ctan = cl * s - cd * c
            ft = _prandtl(B * (1.0 - x) / (2.0 * x * s))
            fh = _prandtl(B * (x - hub) / (2.0 * hub * s)) if hub > 0.01 else 1.0
            f = clamp(ft * fh, 0.05, 1.0)
            # Relacja pędu: a/(1-a) = sigma*cn/(4*F*sin^2(phi))
            den = 4.0 * f * s * s + sigma * cn
            a = sigma * cn / den if abs(den) > 1e-12 else 0.0
            ct_loc = 4.0 * a * (1.0 - a) * f
            if ct_loc > 0.96:                 # Glauert: stan turbulent wake
                a = (ct_loc / 4.0 - 0.16) / 0.2
            a = clamp(a, -0.6, 0.92)
            if abs(sigma * ctan) > 1e-12 and abs(c) > 1e-9:
                d = 4.0 * f * s * c / (sigma * ctan)
                ap = 1.0 / (d - 1.0) if abs(d - 1.0) > 1e-12 else 3.0
            else:
                ap = 0.0
            ap = clamp(ap, -0.5, 3.0)
            return a, ap, f, s

        def residuum(phi):
            a, ap, f, s = stan(phi)
            return (1.0 - a) * math.cos(phi) - lam_r * (1.0 + ap) * s

        lo, hi = 1e-4, math.pi / 2.0 - 1e-4
        rlo = residuum(lo)
        if (rlo > 0.0) == (residuum(hi) > 0.0):
            ok = False          # brak zmiany znaku - element nie oddaje mocy
        for _ in range(46):
            mid = 0.5 * (lo + hi)
            if (residuum(mid) > 0.0) == (rlo > 0.0):
                lo = mid
            else:
                hi = mid
        phi = 0.5 * (lo + hi)
        a, ap, f, s = stan(phi)
        w = abs((1.0 - a) / s)
        # Moc na wale z momentu elementu (poprawna także przy oporze profilu).
        cp += 8.0 * lam * lam * x ** 3 * ap * (1.0 - a) * f * dx
        ct += 8.0 * a * (1.0 - a) * f * x * dx
        sw += 8.0 * lam * lam * x ** 3 * (1.0 - a) * ap * ap * f * dx
        if magnus:
            surf = math.pi * (chord / R) * dx
            spin_cp += (spin_k * w) ** 3 * surf * polar.cf * B
    return BemtOut(cp, ct, sw, spin_cp, ok)


# --------------------------------------------------------------------- DMST ---
class DarrieusRotor:
    def __init__(self, radius, height, n, chord, polar, strut_chord=0.0,
                 strut_cd=0.06, n_struts=0, oswald=0.9,
                 helical=False, troposkien=False):
        self.R, self.H, self.B = radius, height, n
        self.chord = chord
        self.polar = polar
        self.strut_chord = strut_chord
        self.strut_cd = strut_cd
        self.n_struts = n_struts
        self.oswald = oswald
        self.helical = helical
        self.troposkien = troposkien

    @property
    def ar(self):
        return self.H / self.chord if self.chord > 0 else 10.0

    def area(self):
        return 2.0 * self.R * self.H


def _forces(polar, theta, x, ar, oswald):
    s, c = math.sin(theta), math.cos(theta)
    xs = x + s
    w = math.sqrt(xs * xs + c * c)
    if w < 1e-6:
        w = 1e-6
    alpha = math.atan2(c, xs)
    cl = polar.cl(alpha * R2D, 0.0)
    cd = polar.cd(alpha * R2D, 0.0) + cl * cl / (math.pi * max(ar, 1.0) * oswald)
    cn = (cl * xs + cd * c) / w
    ct = (cl * c - cd * xs) / w
    cx = c * cn - s * ct
    return cn, ct, cx, w


class DmstOut:
    __slots__ = ("cp", "ct", "ok")

    def __init__(self, cp=0.0, ct=0.0, ok=True):
        self.cp, self.ct, self.ok = cp, ct, ok


def dmst(rotor, lam, n_tubes=18, n_az=120):
    R, ar = rotor.R, rotor.ar
    dy = 2.0 * R / n_tubes
    k = rotor.B * rotor.chord / (8.0 * math.pi * R)
    au = [0.08] * n_tubes
    ad = [0.08] * n_tubes
    ok = True
    for it in range(70):
        max_delta = 0.0
        nau, nad = [], []
        for j in range(n_tubes):
            y = -R + (j + 0.5) * dy
            s = clamp(y / R, -0.999, 0.999)
            th_u = math.pi - math.asin(s)
            th_d = math.asin(s)
            cu = max(abs(math.cos(th_u)), 0.15)
            cz = max(abs(math.cos(th_d)), 0.15)
            vu = 1.0 - au[j]
            x_u = lam / max(vu, 0.05)
            _, _, cx_u, w_u = _forces(rotor.polar, th_u, x_u, ar, rotor.oswald)
            t = k * w_u * w_u * cx_u / cu
            a_un = clamp(t / (1.0 + t), -0.4, 0.6)
            ve = max(1.0 - 2.0 * au[j], 0.05)
            vd = ve * (1.0 - ad[j])
            x_d = lam / max(vd, 0.05)
            _, _, cx_d, w_d = _forces(rotor.polar, th_d, x_d, ar, rotor.oswald)
            t2 = k * w_d * w_d * cx_d / cz
            a_dn = clamp(t2 / (1.0 + t2), -0.4, 0.6)
            max_delta = max(max_delta, abs(a_un - au[j]) + abs(a_dn - ad[j]))
            nau.append(au[j] + 0.35 * (a_un - au[j]))
            nad.append(ad[j] + 0.35 * (a_dn - ad[j]))
        au, ad = nau, nad
        if max_delta < 1e-7:
            break
        if it == 69:
            ok = False

    dth = 2.0 * math.pi / n_az
    tq = 0.0
    for i in range(n_az):
        th = (i + 0.5) * dth
        s = math.sin(th)
        j = int(clamp((s * R + R) / dy, 0, n_tubes - 1))
        vloc = (1.0 - au[j]) if math.cos(th) < 0 else max(1.0 - 2.0 * au[j], 0.05) * (1.0 - ad[j])
        vloc = max(vloc, 0.02)
        x = lam / vloc
        _, ct, _, w = _forces(rotor.polar, th, x, ar, rotor.oswald)
        tq += vloc * vloc * w * w * ct * dth

    pref = rotor.B * rotor.chord / (4.0 * math.pi * R)
    cp = pref * lam * tq
    if rotor.n_struts > 0 and rotor.strut_chord > 0:
        cp -= rotor.n_struts * rotor.strut_cd * rotor.strut_chord * lam ** 3 / (8.0 * rotor.H)
    if rotor.helical:
        cp *= 0.95
    if rotor.troposkien:
        cp *= 0.96
    ct = 0.0
    for j in range(n_tubes):
        q = au[j] * (1.0 - au[j]) + max(1.0 - 2.0 * au[j], 0.0) ** 2 * ad[j] * (1.0 - ad[j])
        ct += 2.0 * (dy / R) * q
    return DmstOut(max(cp, -1.0), ct, ok)


# ------------------------------------------------------- modele proste -------
def empirical_cp(lam, cp_max, lam_opt, lam_fw):
    if lam <= 0.0 or lam >= lam_fw:
        return 0.0
    p = lam_fw / lam_opt - 1.0
    return max(cp_max * (lam / lam_opt) * ((lam_fw - lam) / (lam_fw - lam_opt)) ** p, 0.0)


class Savonius:
    def __init__(self, radius, height, n_buckets=2, arc=135.0, overlap=0.15,
                 endplate=1.15):
        self.radius, self.height = radius, height
        self.n_buckets, self.arc = n_buckets, arc
        self.overlap, self.endplate = overlap, endplate

    def area(self):
        return 2.0 * self.radius * self.height

    def cp_max(self):
        arc_f = max(1.0 - 0.35 * ((self.arc - 135.0) / 60.0) ** 2, 0.6)
        n_f = 1.0 if self.n_buckets == 2 else 0.92
        ov_f = 1.0 + 0.18 * clamp(self.overlap / 0.15, 0.0, 1.0)
        return 0.16 * arc_f * n_f * ov_f * self.endplate

    def lam_fw(self):
        return 2.0 + 0.15 * self.overlap / 0.15

    def cp(self, lam):
        return empirical_cp(lam, self.cp_max(), 0.85, self.lam_fw())

    def ct(self):
        return 0.9


class Viv:
    def __init__(self, diameter, height, strouhal=0.2, v_lock=11.0,
                 bandwidth=0.15, cp_peak=0.09, amp=0.6, tunable=True):
        self.diameter, self.height = diameter, height
        self.strouhal, self.v_lock = strouhal, v_lock
        self.bandwidth, self.cp_peak, self.amp = bandwidth, cp_peak, amp
        self.tunable = tunable

    def area(self):
        return self.height * (self.diameter + 2.0 * self.amp * self.diameter)

    def cp(self, v):
        bw = self.bandwidth * 2.0 if self.tunable else self.bandwidth
        sigma = max(bw * self.v_lock, 0.2)
        d = (v - self.v_lock) / sigma
        return self.cp_peak * math.exp(-0.5 * d * d)

    def ct(self):
        return 0.60


class Kite:
    def __init__(self, wing, cl=1.4, glide=14.0, alt=300.0, tether=405.0,
                 cycle=0.62, tether_eff=0.85, avail=0.72, areal=3.2):
        self.wing, self.cl, self.glide = wing, cl, glide
        self.alt, self.tether = alt, tether
        self.cycle, self.tether_eff, self.avail = cycle, tether_eff, avail
        self.areal = areal

    def power(self, v, rho=RHO):
        return (2.0 / 27.0) * rho * self.wing * self.cl * self.glide ** 2 * v ** 3 \
            * self.cycle * self.tether_eff * self.avail

    def equiv_area(self, v, rho=RHO):
        q = 0.5 * rho * v ** 3 * BETZ
        return self.power(v, rho) / q if q > 0 else 0.0


class Venturi:
    def __init__(self, inlet, throat, eff=0.12, duct_eff=0.75):
        self.inlet, self.throat = inlet, throat
        self.eff, self.duct_eff = eff, duct_eff

    def cr(self):
        return self.inlet / self.throat

    def velocity_ratio(self, cp_rotor):
        ideal = 1.0 + self.eff * (self.cr() - 1.0)
        max_rv = (BETZ * self.cr() / max(cp_rotor, 0.05)) ** (1.0 / 3.0)
        return min(ideal, max_rv)

    def cp_inlet(self, cp_rotor, rv):
        return min(cp_rotor * rv ** 3 / self.cr() * self.duct_eff, BETZ * self.duct_eff)


class Belt:
    def __init__(self, length, amp, width, cp_max=0.035, lo=0.9, fw=2.2):
        self.length, self.amp, self.width = length, amp, width
        self.cpmax, self.lo, self.fw = cp_max, lo, fw

    def area(self):
        return self.length * self.amp * 2.0

    def cp(self, lam):
        return empirical_cp(lam, self.cpmax, self.lo, self.fw)

    def ct(self):
        return 0.3


# ------------------------------------------------------------------ warianty --
class Hawt:
    def __init__(self, rotor, magnus=False, spin=0.0, counter=False, gain=0.5,
                 parasitic=0.02):
        self.rotor = rotor
        self.magnus, self.spin = magnus, spin
        self.counter, self.gain, self.parasitic = counter, gain, parasitic

    def set_design_lambda(self, l):
        self.rotor.lam_d = l

    def area(self):
        return math.pi * self.rotor.R ** 2

    def cp(self, v, lam, grid):
        o = bemt(self.rotor, lam, 0.0, grid[0], self.spin if self.magnus else 0.0)
        cp = (o.cp - o.cp_spin) * (1.0 - self.parasitic)
        ct = o.ct
        if self.counter:
            a = 0.5 * (1.0 - math.sqrt(max(0.0, 1.0 - min(o.ct, 0.96))))
            vr = max(1.0 - 2.0 * a, 0.15)
            # Drugi wirnik ma własne przełożenie: pracuje przy sensownym lambda
            # w przepływie, który faktycznie widzi (V2 = V*(1-2a)).
            lam2 = clamp(lam / vr, 3.0, 8.0)
            rot2 = HawtRotor(self.rotor.R, self.rotor.hub, self.rotor.B,
                             self.rotor.c_root, self.rotor.c_tip, lam2,
                             self.rotor.alpha_d, self.rotor.polar)
            o2 = bemt(rot2, lam2, 0.0, grid[0], 0.0)
            bonus = clamp(o2.cp, -0.5, BETZ) * vr ** 3 if o2.ok else 0.0
            cp += bonus + self.gain * o.cp_swirl
            ct = min(ct + o2.ct * vr * vr, 1.6)
            # Oba wirniki dzielą tę samą strugę - ograniczamy przyrost tak, by
            # model nie dawał "darmowej" mocy (literatura: realnie +5...15%).
            cp = min(cp, (o.cp * (1.0 - self.parasitic)) * (1.0 + COUNTER_MAX_GAIN))
        # Suﬁt fizyczny: Betz.
        return clamp(min(cp, BETZ), -0.5, BETZ), ct


class Ducted:
    def __init__(self, rotor, beta, flange, length_frac, conc, duct_eff, par):
        self.rotor, self.beta, self.flange = rotor, beta, flange
        self.length_frac, self.conc = length_frac, conc
        self.duct_eff, self.parasitic = duct_eff, par

    def set_design_lambda(self, l):
        self.rotor.lam_d = l

    def area(self):
        return math.pi * self.rotor.R ** 2

    def cp(self, v, lam, grid):
        rv = clamp(1.0 + self.conc * (self.beta - 1.0) + (0.15 if self.flange else 0.0), 1.0, 2.6)
        # łopatę skręcamy dla przepływu, który faktycznie widzi (V_rot = rv * V)
        self.rotor.lam_d = lam / rv
        o = bemt(self.rotor, lam / rv, 0.0, grid[0], 0.0)
        cp_local = (o.cp - o.cp_spin) * (1.0 - self.parasitic)
        cap = BETZ * self.beta * self.duct_eff
        cp = min(cp_local * rv ** 3, cap)
        return clamp(cp, -0.5, 3.0), min(o.ct * rv * rv * 1.7, 2.2)


class VenturiModel:
    def __init__(self, inlet, throat, eff=0.12, duct_eff=0.75):
        self.v = Venturi(inlet, throat, eff, duct_eff)

    def set_design_lambda(self, l):
        pass

    def area(self):
        return self.v.inlet

    def cp(self, v, lam, grid):
        r_t = math.sqrt(self.v.throat / math.pi)
        rv0 = self.v.velocity_ratio(0.45)
        rotor = HawtRotor(r_t, 0.2, 3, 0.12, 0.03, lam / rv0, 6.0, modern())
        o = bemt(rotor, lam / rv0, 0.0, grid[0], 0.0)
        cp_rotor = max(o.cp, 0.05)
        return self.v.cp_inlet(cp_rotor, self.v.velocity_ratio(cp_rotor)), 0.75


class Darr:
    def __init__(self, rotor, parasitic=0.05):
        self.rotor, self.parasitic = rotor, parasitic

    def set_design_lambda(self, l):
        pass

    def area(self):
        return self.rotor.area()

    def cp(self, v, lam, grid):
        o = dmst(self.rotor, lam, grid[1], grid[2])
        return clamp(o.cp * (1.0 - self.parasitic), -0.5, BETZ), o.ct


class Hybrid:
    def __init__(self, darr, sav, interference=0.12, share=0.55):
        self.d, self.s = darr, sav
        self.interference, self.share = interference, share

    def set_design_lambda(self, l):
        pass

    def area(self):
        return self.d.area()

    def cp(self, v, lam, grid):
        cpd, ctd = self.d.cp(v, lam, grid)
        return clamp(cpd * (1.0 - self.interference) + self.s.cp(lam) * self.share, -0.5, BETZ), ctd


class SavoniusModel:
    def __init__(self, s):
        self.s = s

    def set_design_lambda(self, l):
        pass

    def area(self):
        return self.s.area()

    def cp(self, v, lam, grid):
        return clamp(self.s.cp(lam), -0.5, BETZ), self.s.ct()


class VivModel:
    def __init__(self, v):
        self.v = v

    def set_design_lambda(self, l):
        pass

    def area(self):
        return self.v.area()

    def cp(self, v, lam, grid):
        return clamp(self.v.cp(v), 0.0, BETZ), self.v.ct()


class KiteModel:
    def __init__(self, k):
        self.k = k

    def set_design_lambda(self, l):
        pass

    def area(self):
        return self.k.equiv_area(11.0)

    def cp(self, v, lam, grid):
        p = self.k.power(v)
        a = self.k.equiv_area(max(v, 0.5))
        return (p / (0.5 * RHO * v ** 3 * a) if a > 0 else 0.0), 0.0


class BeltModel:
    def __init__(self, b):
        self.b = b

    def set_design_lambda(self, l):
        pass

    def area(self):
        return self.b.area()

    def cp(self, v, lam, grid):
        return clamp(self.b.cp(lam), -0.5, BETZ), self.b.ct()


# ------------------------------------------------------------------- katalog --
def build(did, scale):
    if did == "hawt3":
        return Hawt(HawtRotor(scale, 0.15, 3, 0.075, 0.018, 7.5, 6.0, modern()), parasitic=0.02)
    if did == "hawt2":
        return Hawt(HawtRotor(scale, 0.15, 2, 0.112, 0.027, 9.5, 6.0, modern()), parasitic=0.025)
    if did == "hawt1":
        return Hawt(HawtRotor(scale, 0.15, 1, 0.21, 0.05, 13.0, 6.0, modern()), parasitic=0.03)
    if did == "hawt6":
        return Hawt(HawtRotor(scale, 0.15, 6, 0.045, 0.011, 4.5, 6.0, modern()), parasitic=0.03)
    if did == "farm18":
        return Hawt(HawtRotor(scale, 0.15, 18, 0.26, 0.19, 1.7, 6.0, sheet_metal()), parasitic=0.06)
    if did == "dawt":
        return Ducted(HawtRotor(scale, 0.18, 5, 0.10, 0.055, 5.0, 6.0, modern()),
                      2.6, True, 0.9, 0.18, 0.45, 0.03)
    if did == "counter":
        return Hawt(HawtRotor(scale, 0.15, 3, 0.075, 0.018, 7.0, 6.0, modern()),
                    counter=True, gain=0.5, parasitic=0.03)
    if did == "venturi":
        inlet = math.pi * scale * scale
        return VenturiModel(inlet, inlet / 9.0)
    if did == "hrotor":
        r = DarrieusRotor(scale, 1.5 * scale, 3, 0.06 * scale, naca0018(),
                          strut_chord=0.0125 * scale, n_struts=6)
        return Darr(r, 0.05)
    if did == "helical":
        r = DarrieusRotor(scale, 1.5 * scale, 3, 0.065 * scale, naca0018(),
                          strut_chord=0.0, n_struts=2, helical=True)
        return Darr(r, 0.05)
    if did == "troposkien":
        r = DarrieusRotor(scale, 1.6 * scale, 2, 0.075 * scale, naca0018(),
                          strut_chord=0.0, n_struts=0, troposkien=True)
        return Darr(r, 0.05)
    if did == "savonius":
        return SavoniusModel(Savonius(scale, 2.0 * scale))
    if did == "hybrid":
        d = Darr(DarrieusRotor(scale, 1.5 * scale, 3, 0.06 * scale, naca0018(),
                               strut_chord=0.0125 * scale, n_struts=6), 0.05)
        s = Savonius(0.35 * scale, 0.7 * 1.5 * scale, endplate=1.1)
        return Hybrid(d, s)
    if did == "magnus":
        return Hawt(HawtRotor(scale, 0.2, 3, 0.12, 0.10, 3.5, 0.0, Magnus()),
                    magnus=True, spin=1.2, parasitic=0.04)
    if did == "bladeless":
        return VivModel(Viv(0.12 * scale, 2.6 * scale))
    if did == "kite":
        return KiteModel(Kite(scale))
    if did == "belt":
        return BeltModel(Belt(scale, 0.06 * scale, 0.08 * scale))
    raise ValueError(did)


IDS = ["hawt3", "hawt2", "hawt1", "hawt6", "farm18", "dawt", "counter", "venturi",
       "hrotor", "helical", "troposkien", "savonius", "hybrid", "magnus",
       "bladeless", "kite", "belt"]

SCALES = {
    "hawt3": [5.0, 15.0, 40.0, 63.0, 90.0],
    "hawt2": [5.0, 15.0, 40.0, 63.0, 90.0],
    "hawt1": [2.0, 5.0, 15.0, 40.0],
    "hawt6": [5.0, 15.0, 40.0, 63.0],
    "farm18": [1.5, 3.0, 5.0, 8.0],
    "dawt": [1.5, 3.0, 5.0, 10.0, 20.0],
    "counter": [15.0, 40.0, 63.0, 90.0],
    "venturi": [3.0, 8.0, 15.0, 30.0],
    "hrotor": [3.0, 8.0, 20.0, 40.0],
    "helical": [3.0, 8.0, 20.0, 40.0],
    "troposkien": [3.0, 8.0, 20.0, 40.0],
    "savonius": [0.5, 1.5, 3.0, 6.0],
    "hybrid": [1.5, 3.0, 8.0, 20.0],
    "magnus": [2.0, 6.0, 15.0, 30.0],
    "bladeless": [1.0, 2.0, 4.0, 6.0],
    "kite": [10.0, 25.0, 50.0, 100.0],
    "belt": [0.5, 1.0, 2.0],
}

LAMBDA_RANGE = {
    "hawt3": (3.0, 12.0), "hawt2": (4.0, 15.0), "hawt1": (6.0, 20.0),
    "hawt6": (2.0, 7.0), "farm18": (0.6, 3.0), "dawt": (2.0, 9.0),
    "counter": (3.0, 12.0), "venturi": (2.0, 9.0), "hrotor": (1.5, 7.0),
    "helical": (1.5, 7.0), "troposkien": (1.5, 7.0), "savonius": (0.05, 2.2),
    "hybrid": (0.3, 6.0), "magnus": (0.5, 6.0), "bladeless": (0.0, 0.0),
    "kite": (0.0, 0.0), "belt": (0.1, 2.5),
}

GRID = {"fast": (18, 10, 60), "normal": (32, 18, 120), "fine": (48, 28, 200)}


def evaluate(did, scale, v, lam, grid):
    m = build(did, scale)
    lo, hi = LAMBDA_RANGE[did]
    if hi > 0.0:
        m.set_design_lambda(lam)
        lam_eval = clamp(lam, lo, hi)
    else:
        lam_eval = 0.0
    cp, ct = m.cp(v, lam_eval, grid)
    return cp


def mode_checksum(out_dir, grid):
    rows = []
    lambdas = [1.0, 2.0, 3.0, 4.0, 5.5, 7.0, 9.0, 12.0]
    for did in IDS:
        for scale in SCALES[did][:2]:
            for l in lambdas:
                cp = evaluate(did, scale, 8.0, l, grid)
                rows.append((did, scale, clamp(l, *LAMBDA_RANGE[did])
                             if LAMBDA_RANGE[did][1] > 0 else 0.0, cp))
    os.makedirs(out_dir, exist_ok=True)
    path = os.path.join(out_dir, "checksums_python.csv")
    with open(path, "w") as f:
        f.write("id,scale,lambda,cp\n")
        for (i, s, l, cp) in rows:
            f.write("%s,%s,%.6f,%.9f\n" % (i, s, l, cp))
    print("[python] checksum: %d punktów, suma Cp = %.9f -> %s"
          % (len(rows), sum(r[3] for r in rows), path))


def mode_bench(repeat, grid):
    """Ten sam workload co `cargo run -- --mode bench` (jednowątkowo)."""
    steps = 48
    evals = 0
    t0 = time.perf_counter()
    for _ in range(repeat):
        for did in IDS:
            lo, hi = LAMBDA_RANGE[did]
            for scale in SCALES[did]:
                for i in range(steps + 1):
                    l = lo + (hi - lo) * i / steps
                    evaluate(did, scale, 8.0, l, grid)
                    evals += 1
    dt = time.perf_counter() - t0
    print("BENCH_RESULT lang=python threads=1 time_s=%.4f evals=%d evals_per_s=%.1f"
          % (dt, evals, evals / dt))
    print("[python] %d ewaluacji w %.2f s (%.1f ev/s)"
          % (evals, dt, evals / dt))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mode", default="checksum", choices=["checksum", "bench"])
    ap.add_argument("--repeat", type=int, default=1)
    ap.add_argument("--quality", default="normal", choices=["fast", "normal", "fine"])
    ap.add_argument("--out", default="results")
    a = ap.parse_args()
    grid = GRID[a.quality]
    if a.mode == "checksum":
        mode_checksum(a.out, grid)
    else:
        mode_bench(a.repeat, grid)


if __name__ == "__main__":
    main()
