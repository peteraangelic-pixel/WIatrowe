# WIatrowe

Przesiewowe porównanie **17 kształtów/koncepcji elektrowni wiatrowych** (innych niż
klasyczny trójłopatowy HAWT) pod kątem dwóch kryteriów naraz: **sprawności
aerodynamicznej** i **kosztu wytworzenia energii**.

Kod symulacji jest w **Rust + rayon** (równoległy przesiew po całym katalogu),
a wyniki są liczone i publikowane automatycznie przez **GitHub Actions** do
katalogu `results/` (raport HTML + CSV + JSON).

---

## 1. Wynik w jednym akapicie

Najtańszą energię w modelu daje **latawiec crosswind (AWE)** – 134–156 PLN/MWh
przy 90 % mniejszej masie konstrukcji na kW niż klasyczny HAWT – ale to też
koncepcja o największym ryzyku technologicznym (dyspozycyjność 72 %, brak
dojrzałej certyfikacji). Z konstrukcji "twardych" (stojących na wieży)
najlepszy pozostaje **trójłopatowy HAWT** (211 PLN/MWh na R = 90 m), a
najciekawszą alternatywą jest **Darrieus troposkien** ("ubijaczka"): 239 PLN/MWh
i aż **13,0 kWh z każdego kg materiału rocznie** wobec 7,3 kWh/kg dla HAWT.
Kontr-wirnik (przeciwbieżny) daje realny +11 % Cp, ale koszt drugiego
układu napędowego zjada prawie cały zysk (212 vs 211 PLN/MWh).

## 2. Co dokładnie policzono

Katalog 17 koncepcji × 2–5 skal = **70 wariantów**. Każdy wariant:

* ma policzoną krzywą `Cp(λ)` (albo `Cp(V)` dla maszyn bez λ) i optimum sterowania,
* krzywą mocy `P(V)` po całce po rozkładzie Weibulla (k = 2,0; 6,5 m/s na 100 m;
  ścinanie α = 0,20) → **AEP**, **capacity factor**,
* masę i koszt: łopaty/wirnik, piasta, przekładnia+generator, wieża/maszt,
  dyfuzor, lina, fundament, elektronika, BOS,
* **LCOE** (WACC 7 %, OPEX 95 PLN/kW/rok × czynnik dostępności, czas życia
  8–25 lat wg koncepcji) oraz **kWh z kg materiału na rok**.

Modele aerodynamiczne:

| rodzina | model |
|---|---|
| HAWT (1/2/3/6/18 łopat, przeciwbieżny, Magnus) | **BEMT** wg Burtona: straty Prandtla (końcówka + piasta), poprawka Glauerta na stan *turbulent wake*, równanie równowagi rozwiązywane **bisekcją po kącie napływu** |
| Darrieus (H-rotor, helikalny, troposkien, hybryda z Savoniusem) | **DMST** (Paraschivoiu, ślad równowagowy) |
| DAWT (dyfuzor z kołnierzem), lej Venturiego | BEMT + model koncentracji strugi |
| Savonius, taśma flatterowa (windbelt) | krzywe empiryczne skalibrowane do zakresów z literatury |
| latawiec crosswind (AWE) | wzór Loyda + sprawność cyklu, liny, dyspozycyjność |
| bezwirnikowy rezonans wirowy (VIV) | model pasma synchronizacji Strouhala |

## 3. Ranking wg LCOE (najlepsza skala każdej koncepcji)

| # | koncepcja | skala | moc kW | Cp | CF | AEP MWh | zł/kW | **LCOE PLN/MWh** | kWh/kg·rok |
|---|---|---|---|---|---|---|---|---|---|
| 1 | Latawiec crosswind (AWE) | 100 m² | 2 334 | 0,593 | 0,295 | 6 039 | 1 630 | **134** | 89,8 |
| 2 | Trójłopatowy HAWT (odniesienie) | R 90 m | 11 239 | 0,449 | 0,281 | 27 687 | 4 948 | **211** | 7,3 |
| 3 | Przeciwbieżny dwuwirnikowy | R 90 m | 12 446 | 0,497 | 0,281 | 30 661 | 4 983 | **212** | 6,8 |
| 4 | Dwułopatowy HAWT | R 90 m | 10 516 | 0,420 | 0,281 | 25 907 | 5 008 | **213** | 7,2 |
| 5 | Sześciołopatowy | R 63 m | 5 641 | 0,468 | 0,232 | 11 456 | 4 173 | **223** | 9,3 |
| 6 | Darrieus troposkien | R 40 m | 2 164 | 0,349 | 0,123 | 2 335 | 2 123 | **239** | 13,0 |
| 7 | Darrieus helikalny | R 40 m | 2 174 | 0,374 | 0,119 | 2 269 | 2 155 | **250** | 12,7 |
| 8 | Jednołopatowy z przeciwwagą | R 40 m | 2 030 | 0,363 | 0,183 | 3 256 | 4 077 | **277** | 7,7 |
| 9 | Hybryda Darrieus+Savonius | R 20 m | 476 | 0,327 | 0,084 | 348 | 2 726 | **430** | 8,7 |
| 10 | Darrieus H-rotor | R 40 m | 2 161 | 0,372 | 0,119 | 2 255 | 5 026 | **509** | 4,5 |
| 11 | Wiatrak farmerski (18 łopat) | R 8 m | 44 | 0,316 | 0,141 | 55 | 9 092 | **710** | 2,6 |
| 12 | Savonius | R 6 m | 22 | 0,217 | 0,090 | 18 | 5 691 | **856** | 3,2 |
| 13 | Lej Venturiego | ∅ 30 m | 527 | 0,258 | 0,191 | 883 | 15 987 | **963** | 2,3 |
| 14 | Wirnik Magnusa | R 30 m | 265 | 0,101 | 0,170 | 393 | 14 170 | **1 130** | 2,7 |
| 15 | DAWT (dyfuzor z kołnierzem) | R 20 m | 849 | 0,693 | 0,146 | 1 085 | 14 882 | **1 184** | 2,3 |
| 16 | Bezwirnikowy (VIV) | 6 m | 2 | 0,090 | 0,037 | 1 | 26 659 | **9 060** | 0,5 |
| 17 | Taśma flatterowa (windbelt) | 2 m | 0,008 | 0,035 | 0,075 | 0,02 | 76 860 | **19 585** | 0,03 |

Pełna tabela (wszystkie 70 wariantów i skal) jest w `results/summary.csv`,
a wizualnie w **`results/report.html`** (karty każdej koncepcji, wykresy LCOE,
kWh/kg, krzywych mocy i `Cp(λ)`).

## 4. Wnioski

1. **Wiatr wygrywa z kształtem.** Największy skok LCOE bierze się z wysokości
   piasty i ze skali (R = 90 m: 211 PLN/MWh; R = 15 m: 347 PLN/MWh), nie z egzotyki
   wirnika. Każdą koncepcję trzeba więc porównywać w jej własnym optimum skali.
2. **Kontr-wirnik to realny, ale drogi zysk.** +11 % Cp (0,497 vs 0,449) i +11 %
   AEP, lecz drugi generator/przekładnia i trudniejszy maszt zjadają zysk: 212 vs
   211 PLN/MWh. Opłaca się tam, gdzie cena gruntu/ograniczenie średnicy jest
   ważniejsza niż koszt za MWh (offshore, dachy, mikroinstalacje).
3. **Najlepsza "twarda" alternatywa to troposkien.** Darrieus o łopatach
   troposkienowych ma Cp 0,349 (mniej niż HAWT), ale generator i przekładnia
   stoją na ziemi, a konstrukcja jest lekka: 2 123 zł/kW i **13 kWh/kg·rok**
   (prawie 2× lepiej niż HAWT). Przy drogim materiale, tanim gruncie i małych
   mocach to realna nisza.
4. **Dyfuzory i leje wygrywają w Cp, przegrywają w zł/kW.** DAWT ma Cp = 0,693
   (liczone na pole wirnika – wolno przekroczyć Betza, bo dyfuzor "dokłada"
   strugę), ale sama obudowa kosztuje więcej niż cały wirnik: 14 882 zł/kW.
   Opłaca się tylko tam, gdzie liczy się moc z małej średnicy (dachy, Bieszczady).
5. **Dragowce i " Gadżety" odpadają.** Savonius (0,217), Magnus (0,101),
   VIV (0,090), windbelt (0,035) mają o rząd wielkości gorszy uzysk z kg
   materiału – sens tylko jako bardzo tanie, bardzo małe źródła off-grid.
6. **AWE jest poza zasięgiem pozostałych w modelu, ale ma najszerszy przedział
   niepewności.** 89,8 kWh/kg·rok i 134 PLN/MWh to konsekwencja braku wieży
   i fundamentu (cała moc z 320 kg skrzydła + 220 kg liny). Model zakłada
   dyspozycyjność 72 % i cykl o sprawności 0,62 – jeśli rzeczywista
   dyspozycyjność spadnie do 40 %, LCOE rośnie do ~240 PLN/MWh, czyli do poziomu
   klasycznego HAWT. To jedyny wynik, którego nie zweryfikują pomiary
   terenowe małych prototypów.

## 5. Dlaczego Rust + rayon: zmierzony benchmark

Ten sam workload (identyczna liczba ewaluacji modelu aerodynamicznego,
czysty Python bez numpy) na 4-rdzeniowym runnerze GitHub Actions:

| implementacja | wątki | czas [s] | ewaluacji/s | przyspieszenie |
|---|---|---|---|---|
| Python (referencyjna) | 1 | 25,52 | 269 | 1,0× |
| **Rust + rayon** | 1 | 15,79 | 8 689 | **32,3×** |
| **Rust + rayon** | 2 | 7,90 | 17 369 | **64,6×** |
| **Rust + rayon** | 4 | 5,26 | 26 077 | **97,0×** |

(liczby z jednego uruchomienia na 4-rdzeniowym runnerze; między uruchomieniami
zmieniają się o kilka procent, rząd wielkości jest stały)

`results/bench_summary.txt`. Złota reguła: cały katalog 70 wariantów liczy się
**0,73 s** – 25 testów + sweep + benchmark + walidacja mieszczą się w jednym
uruchomieniu CI poniżej minuty.

## 6. Jak sprawdziliśmy, że wyniki są wiarygodne

* **Podwójna implementacja.** Cały model jest napisany dwa razy: w Rust
  (`src/`) i w Pythonie (`tools/reference_sweep.py`). CI porównuje 272 liczby
  kontrolne `Cp(id, skala, λ)` wiersz po wierszu:
  **największa różnica = 0,000e+00** (`results/compare_checksums.txt`).
  Udało się to dopiero po zamianie klasycznej iteracji BEMT (a, a') na
  rozwiązywanie równowagi bisekcją po kącie napływu – iteracja punktu stałego
  nie zbiega przy dużym obciążeniu elementu i dawała wyniki zależne od
  kolejności zaokrągleń.
* **25 testów jednostkowych** (`cargo test`, log: `ci/test.log`), m.in.
  `Cp < Betz` dla całego zakresu λ, `Cp_max` dobrze zaprojektowanego wirnika
  0,45–0,52 przy λ 6–9, optymalne λ maleje z liczbą łopat, Savonius w zakresie
  literaturowym, wzór Loyda zgodny z referencją, masa wieży NREL 5 MW.
* **Kalibracja do literatury** (źródła w komentarzach w `src/`): Cp_max klasy
  HAWT 0,45–0,50; Savonius 0,15–0,32; Darrieus/hybryda ≈0,34; Magnus ≤0,2;
  DAWT: Cp_wirnika ≤1,0 i augmentacja ≤3; masa łopaty NREL 5 MW ≈17,7 t;
  wieża ≈320 t; CAPEX HAWT ≈5 000–6 000 PLN/kW.

## 7. Uruchomienie

```bash
cargo run --release -- --mode sweep  --quality normal --out results   # pełny przesiew + raport HTML
cargo run --release -- --mode bench   --repeat 40                     # benchmark rayon
cargo run --release -- --mode checksum --quality normal --out results # liczby kontrolne
cargo test                                                            # 25 testów modelu

python3 tools/reference_sweep.py --mode checksum --quality normal --out results
python3 tools/compare_checksums.py results/checksums.csv results/checksums_python.csv
python3 tools/compare_bench.py results/bench_rust.txt results/bench_python.txt
```

Opcje: `--climate poland|coastal`, `--designs hawt3,dawt,kite`, `--threads N`,
`--quality fast|normal|fine` (siatka BEMT/DMST: 32/18/120 dla `normal`).

Workflow `sim.yml` robi to samo w CI i **commituje wyniki z powrotem na branch**
do `results/`; `ci.yml` dokłada logi kompilacji i testów w `ci/`.

## 8. Zawartość `results/`

| plik | co zawiera |
|---|---|
| `report.html` | główny raport: ranking, wykresy, karty 17 koncepcji, założenia |
| `summary.csv`, `summary.json` | pełna tabela 70 wariantów |
| `power_curves.csv`, `cp_curves.csv` | krzywe mocy `P(V)` i `Cp(λ)` |
| `checksums.csv`, `checksums_python.csv`, `compare_checksums.txt` | walidacja Rust vs Python |
| `bench_rust.txt`, `bench_python.txt`, `bench_summary.txt` | benchmark rayon vs Python |
| `console_*.txt` | logi poszczególnych kroków CI |

## 9. Ograniczenia

Model jest **przesiewowy** (screening): ma wyłapać rząd wielkości i kierunek
różnic, nie zastępuje pomiarów terenowych ani CFD. Nie ma tu aeroelastyki,
zmęczenia materiałowego, hałasu, oddziaływania parku (wake losses), kosztów
przyłącza ani ryzyka technologicznego (AWE/VIV/DAWT są oznaczone w raporcie
jako modele o niskiej pewności). Ceny są w PLN, poziom 2024–2025, kalibrowane
globalnie na CAPEX bazowego HAWT.
