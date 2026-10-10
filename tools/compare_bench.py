#!/usr/bin/env python3
"""
Zestawienie benchmarku: Rust (+rayon) vs czysty Python.

Czyta pliki z liniami w formacie:
    BENCH_RESULT lang=rust threads=4 time_s=0.1234 evals=195000 evals_per_s=...

i wypisuje:
    * przepustowość (ewaluacje/s) obu implementacji,
    * przyspieszenie Rust(1 wątek) vs Python  <- "czysty" zysk z kompilacji,
    * przyspieszenie Rust(N wątków) vs Python <- zysk z kompilacji + rayon.

    python3 tools/compare_bench.py results/bench_rust.txt results/bench_python.txt
"""

import argparse
import re
import sys

LINE = re.compile(
    r"BENCH_RESULT lang=(\S+) threads=(\d+) time_s=([\d.eE+-]+) evals=(\d+)")


def load(path):
    out = {}
    try:
        with open(path) as f:
            for line in f:
                m = LINE.search(line)
                if m:
                    lang, th, t, ev = m.group(1), int(m.group(2)), float(m.group(3)), int(m.group(4))
                    out[(lang, th)] = (t, ev)
    except FileNotFoundError:
        pass
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("rust")
    ap.add_argument("python")
    a = ap.parse_args()

    res = {}
    res.update(load(a.python))
    res.update(load(a.rust))

    rust = {th: v for (lang, th), v in res.items() if lang == "rust"}
    py = {th: v for (lang, th), v in res.items() if lang == "python"}

    if not rust or not py:
        print("Brak danych (oczekiwano linii BENCH_RESULT w obu plikach)")
        return 1

    py_th = max(py)
    py_t, py_ev = py[py_th]
    py_rate = py_ev / py_t

    print("%-14s %12s %14s %14s" % ("implementacja", "wątki", "czas [s]", "ewaluacji/s"))
    print("-" * 58)
    print("%-14s %12d %14.3f %14.1f" % ("Python", py_th, py_t, py_rate))
    print("-" * 58)
    for th in sorted(rust):
        t, ev = rust[th]
        rate = ev / t
        print("%-14s %12d %14.3f %14.1f   x%.1f vs Python"
              % ("Rust+rayon", th, t, rate, rate / py_rate))
    print("-" * 58)
    best = max(rust, key=lambda th: rust[th][1] / rust[th][0])
    bt, bev = rust[best]
    print("Najszybszy wariant: Rust + rayon na %d wątkach - %.0f razy szybciej "
          "niż jednowątkowy Python." % (best, (bev / bt) / py_rate))
    print("Uwaga: workload jest identyczny (ta sama liczba ewaluacji modelu "
          "aerodynamicznego), Python jest czysty (bez numpy).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
