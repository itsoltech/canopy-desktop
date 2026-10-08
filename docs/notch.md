# Notch sesji agentów

Osobny, nieaktywujący overlay przy górnej krawędzi głównego ekranu działa na
macOS i ma adapter Windows. Referencje: `00-notch.png`, `00-notch-hover.png`
i komponenty NotchOverlay w repozytorium Electron.

## Wygląd i ruch

Czarna wyspa pokazuje rzeczywiste sesje Claude/Codex z globalnego AgentsState.
Wysokość belki pochodzi z wysokości menu/work area (fallback 37); szerokość
zwinięta to round(height × 5.5) + 80.
Rozwinięcie ma 480 jednostek szerokości, wiersz 48 i padding dolny 6.
Na badanym ekranie stan zwinięty to 295 × 39, rozwinięty 480 × 93.

`components/notch.rs` zawiera stateless surface/header/session row.
`notch_motion.rs` składa wspólne Presence z `canopy_desktop::motion`:
szerokość 250 ms, wysokość 400 ms, zamknięcie 350 ms bez zwłoki,
fade treści 40+250 ms / 150 ms i przesunięcie 4 px. Retarget zachowuje
bieżącą wartość. Reduce Motion jest odczytywane przy zmianie stanu.
Nie ma pollingu ani pętli klatek w bezczynności.

## Stałe okno i przepuszczanie kliknięć

**Natywna ramka pozostaje stała również podczas animacji.** Próby zmieniania
jej rozmiaru powodowały niestabilne renderowanie GPUI/Metal i zostały usunięte.
Sama przezroczystość NSPanel nie zapewnia przepuszczania kliknięć.

`PointerMonitor` w `notch_macos.rs` rejestruje lokalny i globalny monitor
zdarzeń ruchu/przeciągania myszy. Nie monitoruje klawiatury i nie wykonuje
pollingu. Sprawdza rzeczywistą pozycję kursora względem widocznej wyspy,
włącznie z zaokrąglonymi dolnymi rogami:

- poza wyspą: NSWindow.ignoresMouseEvents = true;
- wewnątrz: false, a controller dostaje zdarzenie wejścia.

Dzięki globalnemu monitorowi wejście jest wykrywane także wtedy, gdy panel
ignoruje zdarzenia. Lokalne zdarzenia są zwracane bez zmian. Zmiany stanu
są przekazywane przez executor GPUI, nie przez reentrant update w callbacku
AppKit. Region jest aktualizowany zgodnie z animowaną geometrią; renderer
nie zmienia natywnej ramki. Powtarzające się zdarzenia nie restartują ruchu.
Monitor jest własnością encji notcha i usuwa swoje tokeny w Drop.

Na Windows `SetWindowRgn` ogranicza zarówno rysowanie, jak i hit testing do
bieżącej wyspy. Scoped `WH_MOUSE_LL` wykrywa wejście także poza regionem okna,
bez monitorowania klawiatury i bez pollingu. Region jest skalowany z jednostek
GPUI do fizycznych pikseli bieżącego DPI. `WS_EX_NOACTIVATE` oraz scoped
`WM_MOUSEACTIVATE → MA_NOACTIVATE` chronią foreground window.

To zastępuje także on_hover przeliczany z layoutu, który wcześniej potrafił
odwracać zamykanie na podstawie ostatniej pozycji myszy w oknie.

## Pozycja i cykl życia

GPUI 0.3.4 tworzy panel z NSTitledWindowMask również dla titlebar=None.
Adapter usuwa tę flagę tylko w notchu i ustawia Borderless |
NonactivatingPanel oraz pozycję według pełnego NSScreen.frame.
Poziom PopUp to 101; wcześniej zweryfikowano top_gap=0.00pt.
Zwykłe okna aplikacji pozostają bez zmian. Region przezroczysty nie służy
już do przechwytywania myszy ani utrzymywania aktywacji.

Kliknięcie sesji przywraca workspace. Zamknięcie ostatniego zwykłego okna
kończy proces; sam notch go nie utrzymuje. CANOPY_NOTCH_PREVIEW=1 ukrywa
początkowo workspace na potrzeby izolowanego podglądu panelu na obu systemach.

Windows kotwiczy overlay do górnej krawędzi primary work area, reaguje na DPI,
zmianę monitorów i ustawień taskbara. Obcy borderless fullscreen ukrywa notch
tylko na tym samym monitorze. Foreground oraz filtrowane zmiany stanu/lokalizacji
aktywnego HWND obsługują także F11 → F11 bez przełączania aplikacji.
Seria zdarzeń jest scalana do jednej prywatnej wiadomości HWND w iteracji pętli;
callback WinEvent nie przelicza geometrii. Zmaksymalizowane i własne okna Canopy
go nie ukrywają.

Błąd runtime regionu/DPI ukrywa HWND i trafia jako typowane zdarzenie do
kontrolera. Główne okno pokazuje trwały status, wykonuje jedną opóźnioną próbę,
a kolejne zdarzenie monitora/foreground pozwala ponowić recovery bez pollingu.
Render przy stanie unavailable jedynie zachowuje najnowszy region i nie ponawia
Win32 I/O co klatkę. Po sukcesie status błędu jest czyszczony. Komunikat jest
widoczny również w pustym stanie głównego okna bez wybranego projektu.

## Weryfikacja i ograniczenia

Testy regionu wykluczają
przezroczysty obszar pod zwiniętą belką, boczne marginesy i narożniki.
Zachowane są regresje animacji i geometrii; testy Windows obejmują skalowanie,
ujemne originy, top taskbar, clamping, F11 na tym samym HWND i dwa monitory.
Adapter przechodzi
MSVC Clippy harness, ale strategia monitorowania wymaga sprawdzenia rzeczywistą
myszką. Nie deklarujemy pomiaru FPS ani pełnej kwalifikacji kliknięć w
przeglądarce na podstawie testu geometrii.

Przełącznik Preferences pozostaje poza tym etapem. Wiele monitorów,
Spaces/virtual desktops, fullscreen, taskbar auto-hide i resume nie były jeszcze
natywnie kwalifikowane. Mouse monitors nie zbierają ani nie zapisują historii
ruchu — używany jest wyłącznie aktualny stan inside/outside.
