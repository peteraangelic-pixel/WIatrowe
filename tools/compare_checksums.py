#!/usr/bin/env python3
"""
Porównanie liczb kontrolnych: Rust (`results/checksums.csv`) vs Python
(`results/checksums_python.csv`).

To jest test poprawności całej implementacji: obie wersje liczą Cp na tej
samej siatce (wariant x skala x lambda). Różnica większa niż tolerancja
oznacza błąd w jednym z modeli (lub rozjechanie się stałych).

    python3 tools/compare_checksums.py results/checksums.csv \
        results/checksums_python.csv --tol 1e-6
"""

import argparse
import csv
import sys


def load(path):
    out = {}
    with open(path) as f:
        for row in csv.DictReader(f):
            key = (row["id"], float(row["scale"]), round(float(row["lambda"]), 4))
            out[key] = float(row["cp"])
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("rust")
    ap.add_argument("python")
    ap.add_argument("--tol", type=float, default=1e-6)
    a = ap.parse_args()

    ra, pb = load(a.rust), load(a.python)
    keys = sorted(set(ra) | set(pb))
    missing = [k for k in keys if k not in ra or k not in pb]
    if missing:
        print("BŁĄD: klucze tylko w jednym z plików: %s" % missing[:10])
        return 1

    worst = (0.0, None)
    bad = []
    for k in keys:
        d = abs(ra[k] - pb[k])
        if d > worst[0]:
            worst = (d, k)
        if d > max(a.tol, a.tol * abs(ra[k])):
            bad.append((k, ra[k], pb[k], d))

    print("porównano %d punktów (Rust vs Python)" % len(keys))
    print("największa różnica: %.3e w %s" % (worst[0], worst[1]))
    if bad:
        print("BŁĄD: %d punktów poza tolerancją %g:" % (len(bad), a.tol))
        for (k, x, y, d) in bad[:20]:
            print("   %-12s skala=%-6s lambda=%-6s rust=%.9f python=%.9f diff=%.3e"
                  % (k[0], k[1], k[2], x, y, d))
        return 1
    print("OK: implementacje zgodne w granicach tolerancji")
    return 0


if __name__ == "__main__":
    sys.exit(main())
