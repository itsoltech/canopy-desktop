# Wspólny system ruchu Canopy

Publiczne API: `canopy_desktop::motion` (biblioteka aplikacji w `src/lib.rs`).
Własne widoki używają tych samych tokenów tak jak `ui::theme` dla kolorów.
Notch jest pierwszym konsumentem; `ui::theme::init` przekazuje też skalę do
pól motion w GPUI Kit. Dane animacji należą do widoku, nie do globalnego timera.

## Tokeny

| Grupa | Tokeny / wartości |
| --- | --- |
| `duration` | INSTANT 0, STAGGER 40, MICRO 80, QUICK 150, FAST 250, MEDIUM 350, SLOW 400, VERY_SLOW 500 ms |
| `distance` | MICRO 4, SMALL 6, BASE 8, MEDIUM 12, LARGE 30 jednostek logicznych |
| `scale` | MODAL 0.96, DROPDOWN 0.97, TOOLTIP 0.98, SUBTLE 0.99 |
| `Easing` | SmoothOut, EaseInOut, EaseOut, Linear |

`SmoothOut` to cubic-bezier(0.22, 1, 0.36, 1), domyślna krzywa powierzchni.
Krzywe mają jedno źródło wartości zarówno dla naszego silnika, jak i GPUI Kit.
Nie wprowadzamy blur jako tokenu, dopóki nie mamy wspólnego, sprawdzonego
mechanizmu jego renderowania w GPUI.

## Gotowe profile

| `presets` | Wejście | Wyjście / zastosowanie |
| --- | --- | --- |
| PANEL | 400 ms | 350 ms |
| POPOVER | 250 ms | 150 ms |
| CONTENT_REVEAL | 40 ms delay + 250 ms | 150 ms, bez delay |
| RESIZE | 250 ms | zmiana liczbowego wymiaru |
| STATE_CHANGE | 150 ms | zmiana wartości/stanu |

Profile wybieramy według zastosowania, nie najbliższej liczby ms.
Nietypowy komponent może składać `PresenceSpec` i `TransitionSpec` z tokenów,
tak jak notch łączy wejście szerokości FAST z wyjściem MEDIUM. Nie zmieniamy
wspólnego presetu dla jednego wyjątku.

## Mechanizmy

- `Transition`: dowolna skończona wartość f32, np. szerokość, offset, skala.
- `Presence`: 0..1 i osobne przepisy wejścia/wyjścia.
- `policy(cx)`: wspólne Reduced Motion — flaga GPUI, ustawienie macOS oraz
  cache `SPI_GETCLIENTAREAANIMATION` na Windows. Windows odświeża cache przez
  scoped `WM_SETTINGCHANGE`, poza renderem.
- `request_frame(window, active)`: planuje klatkę tylko, gdy jest potrzebna.
- `apply_to_theme(theme)`: mapuje wspólne czasy/krzywe/odległości do motywu
  GPUI Kit. Jego wewnętrzne sprężyny i szczególne animacje pozostają własnością biblioteki.

Powtórzenie tego samego celu nie restartuje animacji. Zmiana celu w trakcie
ruchu zaczyna się od aktualnej wartości. Gwarantujemy ciągłość wartości,
nie ciągłość prędkości jak w symulacji fizycznej sprężyny.
`is_animating` uwzględnia delay i kończy się na ostatnim terminie kanału.
Reduced Motion pomija czas i delay; wartość natychmiast osiąga cel.
Zmiana systemowa Windows w trakcie animacji również zwraca od razu target,
kończy `is_animating` i zatrzymuje żądania kolejnych klatek. Szybkie ponowne
włączenie animacji nie wznawia starego przejścia; dopiero nowy retarget może
rozpocząć kolejny ruch.
Zerowa duration bez Reduced Motion może służyć do skoku po zadanym delay.

## Przykład widoku

```rust
use canopy_desktop::motion::{self, Presence, presets, distance};
use std::time::Instant;

// Pole widoku, inicjalizowane raz:
let reveal = Presence::new(false, presets::PANEL, Instant::now());

// Handler zmiany stanu:
self.open = open;
if self.reveal.set_open(open, Instant::now(), motion::policy(cx)) {
    cx.notify();
}

// W render:
let now = Instant::now();
let active = self.reveal.is_animating(now);
let progress = self.reveal.progress(now);
motion::request_frame(window, active);
let surface = div().children((self.open || active).then(|| {
    div()
        .relative()
        .top(px(distance::BASE * (1. - progress)))
        .opacity(progress)
        .child("Panel content")
}));
```

Przykład skalarnego resize:

```rust
use canopy_desktop::motion::{Transition, MotionPolicy, presets};
use std::time::Instant;

let now = Instant::now();
let mut width = Transition::new(220., now);
width.retarget(320., presets::RESIZE, now, MotionPolicy::Full);
let current_width = width.value(Instant::now());
```

## Zasady integracji

1. Trzymaj stan przejścia jako pole widoku; nie twórz go od nowa w render.
2. Zmieniaj cel w handlerze. Render odczytuje wszystkie kanały z jednego `now`.
3. Żądaj klatki, dopóki którykolwiek kanał pracuje; bez pętli notify/timera.
4. Opacity=0 nie wyłącza hit testingu. Po zamknięciu usuń zawartość albo
   jawnie wyłącz jej interakcje; w czasie exit nie uruchamiaj ukrytych akcji.
5. Nie steruj wejściem/wyjściem animowanej powierzchni hoverem przeliczanym
   z jej zmieniającego się layoutu. Notch ma osobną obsługę realnego kursora.
6. Nie sumuj p50 render z GPU i nie deklaruj 120 FPS na podstawie tokenów.

## Weryfikacja

Testy obejmują ciągłość przy odwróceniu, nieprzesuwanie terminu przy tym samym
celu, zero duration z delay, Reduced Motion, kończenie po exit i ograniczone
monotoniczne krzywe. Pozostają też regresje geometrii i przebiegu animacji notcha.

### Pojawianie się partii wpisów

`BatchReveal` posiada animacje najnowszego zakresu listy. `begin(start, count,
now, policy)` używa CONTENT_REVEAL i równomierny stagger rozłożony w oknie maksymalnie pięciu tokenów STAGGER;
`opacity(index, now)` zwraca 1 poza nową partią. `reset()` czyści stan przy
zmianie danych. Właściciel wywołuje `request_frame` w renderze wyłącznie, gdy
`is_animating(now)` zwraca true. Reduce Motion pomija fade oraz opóźnienia.
