#!/usr/bin/env python3
"""
Porównanie liczb kontrolnych: Rust (`results/checksums.csv`) vs Python
(`results/checksums_python.csv`).

To jest test poprawności całej implementacji: obie wersje liczą Cp na tej
samej siatce (wariant x skala x lambda) i w tej samej kolejności, więc
porównujemy wiersz po wierszu.

Zgodność jest możliwa tylko dlatego, że solver BEMT rozwiązuje równanie
równowagi bisekcją po kącie napływu (zamiast klasycznej iteracji punktu
stałego a/a' - ta nie zbiega przy dużym obciążeniu elementu i daje wtedy
wynik zależny od kolejności zaokrągleń, czyli nieporównywalny między
językami).

    python3 tools/compare_checksums.py results/checksums.csv \
        results/checksums_python.csv --tol 1e-9
"""

import argparse
import csv
import sys


def load(path):
    with open(path) as f:
        rows = list(csv.DictReader(f))
    return [
        (r["id"], float(r["scale"]), float(r["lambda"]), float(r["cp"])) for r in rows
    ]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("rust")
    ap.add_argument("python")
    ap.add_argument("--tol", type=float, default=1e-9)
    a = ap.parse_args()

    ra, pb = load(a.rust), load(a.python)
    if len(ra) != len(pb):
        print("BŁĄD: różna liczba wierszy: rust=%d python=%d" % (len(ra), len(pb)))
        return 1

    worst = (0.0, None)
    bad = []
    for (kr, sr, lr, cr), (kp, sp, lp, cp) in zip(ra, pb):
        if (kr, sr, lr) != (kp, sp, lp):
            print("BŁĄD: wiersze nie pasują do siebie: %r vs %r"
                  % ((kr, sr, lr), (kp, sp, lp)))
            return 1
        d = abs(cr - cp)
        if d > worst[0]:
            worst = (d, (kr, sr, lr))
        if d > a.tol:
            bad.append(((kr, sr, lr), cr, cp, d))

    print("porównano %d punktów (Rust vs Python, wiersz po wierszu)" % len(ra))
    print("największa różnica: %.3e%s"
          % (worst[0], "" if worst[1] is None else " w %s" % (worst[1],)))
    if bad:
        print("BŁĄD: %d punktów poza tolerancją %g:" % (len(bad), a.tol))
        for (k, x, y, d) in bad[:20]:
            print("   %-12s skala=%-6s lambda=%-6s rust=%.9f python=%.9f diff=%.3e"
                  % (k[0], k[1], k[2], x, y, d))
        return 1
    print("OK: obie implementacje dają identyczne wyniki (do %g)" % a.tol)
    return 0


if __name__ == "__main__":
    sys.exit(main())
