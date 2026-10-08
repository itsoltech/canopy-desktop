# Profilowanie mocka Canopy — 2026-09-08

## Wynik i przyczyna

Samo przejście z debug do optymalizacji poprawiło wynik, ale nie wystarczyło.
Przewijanie nawigacji Preferences ponownie obliczało layout niezmienionego
General. W śladzie zoptymalizowanym `Window::draw` występował w 3331 ms
próbek głównego wątku, `TaffyLayoutEngine::compute_layout` w 2714 ms
(ok. 81% próbek zawierających draw). To pomiar próbek stosów, nie precyzyjny
stoper poszczególnych funkcji. Debug: odpowiednio 11475 i 10555 ms.

Rozdzielono nawigację i treść na osobne encje z `Entity::cached` i jawnym
rozmiarem viewportu. Powiadomienia kontrolek, zmiana granic i stylu tekstu
unieważniają właściwy cache. Nie dodano pętli notify ani wymuszania klatek.

Po zmianie p95 kosztu `Window::draw` wynosi 4.48–4.65 ms zamiast
17.21–17.80 ms w zoptymalizowanym buildzie przed cache i 54.20–54.40 ms w debug.
To ok. 74% poprawy względem poprzedniego builda zoptymalizowanego.

## Środowisko i metoda

- MacBook Pro / M1 Pro: 10 CPU (8P+2E), 14 GPU, 32 GB RAM.
- macOS 26.5.2 (25F84), Apple Metal; zasilanie AC, low power mode wyłączony.
- Wbudowany ekran, CoreGraphics: logical 1800 × 1169, backing 3600 × 2338,
  nominalne 120 Hz. Panel fizyczny 3024 × 1964; ustawiona skala systemowa.
- Rust 1.95.0, aarch64-apple-darwin, GPUI Kit 0.6.0 / gpui-pre 0.3.4.
- Wspólna baza Git 4c1ab57 plus niezacommitowany mock; różnica badana:
  profil Cargo oraz cache w Preferences. Snapshot przed cache zachowano w artefaktach.
- `dev`: opt-level 0 i debug assertions; `profiling`: dziedziczy release
  (opt-level 3), symbole debug=2, bez inspektora. Oba pomiary używają
  tego samego opcjonalnego `frame-profile`. Czysty release nie ma rejestratora.
- Nawigacja Preferences: 5 grup, 17 pozycji. General: 3 grupy, 6 wierszy;
  okno 920 × 720. Dane statyczne, fonty i układ rozgrzane przed próbami.
- Każda próba: 30 s zapisu, naprzemienne przewijanie nawigacji o stronę
  przez Computer Use. 244–279 poleceń na próbę; nie jest to ciągły strumień
  wejścia 120 Hz. Rejestrowana jest rzeczywista liczba draw, nie 120 × czas.
- Kompilator wyłączony podczas prób; aktywne zwykłe aplikacje desktopowe,
  Computer Use i WindowServer. Nie zmieniano konfiguracji zdalnego pulpitu.
- CPU Time Profiler działał dodatkowo podczas 3. próby optimized przed cache;
  debug Time Profiler pochodzi z osobnej przerwanej próby. Narzut narzędzi istnieje.

## Koszt pracy CPU/frameworka

Granica metryki: rozpoczęcie i zakończenie całego `Window::draw` z GPUI.
Nie oznacza czasu GPU ani fizycznej prezentacji. Każdy wiersz to osobna próba
jednego okna Preferences. Sporadyczne rekordy głównego okna wyłączono z tej
tabeli, ale zachowano w CSV/JSON. Wszystkie wartości w ms.

| Wariant / próba | n | p50 | p95 | p99 | max | > 8.333 ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Debug + profiler, 1 | 269 | 53.178 | 54.197 | 54.651 | 55.009 | 100.00% |
| Debug + profiler, 2 | 257 | 52.952 | 54.210 | 54.892 | 62.503 | 100.00% |
| Debug + profiler, 3 | 262 | 53.060 | 54.401 | 55.641 | 61.950 | 100.00% |
| Optimized przed cache, 1 | 302 | 14.383 | 17.804 | 19.285 | 22.719 | 84.77% |
| Optimized przed cache, 2 | 308 | 14.146 | 17.206 | 19.048 | 19.751 | 79.87% |
| Optimized przed cache, 3 | 302 | 14.272 | 17.565 | 18.593 | 21.373 | 86.42% |
| Optimized po cache, 1 | 285 | 2.992 | 4.544 | 5.819 | 10.044 | 0.35% |
| Optimized po cache, 2 | 287 | 2.838 | 4.652 | 5.210 | 10.089 | 0.35% |
| Optimized po cache, 3 | 309 | 2.987 | 4.479 | 5.834 | 13.455 | 0.32% |

Próg roboczy kosztu: p95 ≤ 6 ms, p99 ≤ 8.333 ms, przekroczenia ≤ 1%.
Trzy próby po cache mieszczą się w tych progach, jednak każda ma mniej niż
1200 próbek wymaganych do pełnej kwalifikacji według skilla. Są krótkim
porównaniem diagnostycznym. Nie łączono prób dla uzyskania lepszego percentyla.
Przerwane próby (zmiana aktywnego okna/interwencja UI) trzymane są osobno.

## GPU, submission i idle

Metal System Trace zapisano dla optimized przed cache. Narzędzie nie
zakończyło automatycznie zapisu po time-limit; po SIGINT zapisało ślad z
ostrzeżeniem o backdated signpost timestamps. Nie jest podstawą deklaracji FPS.
Filtrowano dane GPU po PID 16157: Vertex 109 zakresów, p95 0.055 ms;
Fragment 112 zakresów, p95 3.203 ms. Są to zakresy wykonania kanałów GPU,
nie kompletne klatki; nie sumujemy ich jako czasu jednej klatki. GPU po
cache nie mierzono osobnym śladem.

Rejestrator GPUI zapisuje również koszt submission i animation submission
interval. Sporadycznie submission przekracza 8.333 ms także po cache.
To czas przekazania platformie, a nie timestamp scanout. Faktyczne 120
prezentacji/s oraz end-to-end input latency pozostają niepotwierdzone.

Zebrano 10-sekundowe próbki ps/RSS i sample stosów bezczynności. %CPU ps
jest średnią kroczącą i zawiera wygasające obciążenie rozgrzewki; nie należy
porównywać jej jako stabilnego idle CPU. RSS przy końcu krótkich odczytów:
debug ok. 135 MiB, optimized przed cache ok. 121 MiB, po cache ok. 130 MiB.
To pojedyncze snapshoty, nie test retencji ani wycieku pamięci.

## Artefakty i odtwarzanie

Surowe dane: `target/performance/` (ignorowane w Git):
- debug/, optimized/, optimized-cache/: CSV klatek, po trzy ważne próby;
- interrupted/ oraz debug-resize/: próby przerwane/pomocnicze, poza tabelą;
- debug-scroll.trace, optimized-scroll.trace: stosy CPU;
- optimized-metal.trace, metal-gpu.xml, gpu-summary.json: dane GPU;
- summary.json, environment.txt, *-idle.csv i *-idle-sample.txt.
Ślady Instruments zawierają lokalne metadane procesu; nie publikowano ich.

```sh
./scripts/run-macos.sh release
./scripts/run-macos.sh profiling --features frame-profile
# Ctrl+Option+P: zapis 30 s; default $TMPDIR/canopy-profiles
python3 scripts/summarize-frames.py /path/to/frames-*.csv
```

Wydanie do lokalnego użycia: `target/release/Canopy.app`, arm64, opt-level 3,
podpis ad-hoc (bez Developer ID/notary). Skrypt domyślnie wybiera release
oraz atomowo podmienia binarium, aby uniknąć niespójności kodu uruchomionej
aplikacji i cache podpisu macOS. Profil diagnostyczny to osobny bundle.

## Sprawdzenia i ograniczenia

Build release i profiling, Clippy z frame-profile oraz bez, fmt, kontrola
podpisu ad-hoc: wykonane. W GUI sprawdzono przewijanie i zmianę stanu checkboxa,
menu selecta oraz wybór klawiaturą po cache. Zachowany wygląd General.
Użytkownik potwierdził prawidłowe działanie wydania zoptymalizowanego.

Resize głównego panelu ma tylko próbę pomocniczą/przerwaną; nie kwalifikujemy
jego p95. Nie zmierzono zimnego startu, realnego terminala, dużych zbiorów,
Windows/Linux, czytnika ekranu ani długotrwałej retencji pamięci. Do kolejnych
widoków zachowujemy tę samą metodę i osobno wykonamy test ciągłej prezentacji.
