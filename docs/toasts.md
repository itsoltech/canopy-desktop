# Powiadomienia toast

`AppState.toasts` posiada pojedynczy `ToastHost`. Wywołanie
`toasts.update(cx, |host, cx| host.show(message, cx))` pokazuje lekki komunikat
w głównym oknie. Obecnie korzystają z niego sukcesy Pull/Push i commit.
Błędy pozostają inline, decyzje w modalach. Nie pokazujemy toastów dla hoveru,
przełączania tabów, autosave ani zwykłego odświeżania statusu.

Host wykorzystuje natywny `gpui_kit::base::Toast` oraz tokeny Canopy. Nie używa
systemowego centrum powiadomień i nie prosi o uprawnienia macOS. Jest nakładką
nad prawym końcem statusbara, poniżej warstw modali, bez zmiany layoutu i focusu.
Tylko powierzchnia komunikatu przechwytuje kliknięcia.

Jeden aktywny toast i dwie widoczne krawędzie oczekujących kart. Kolejka FIFO
mieści do 10 komunikatów; przepełnienie zastępuje najnowszy oczekujący wpis,
a identyczne wiadomości z osobnych operacji zachowują osobne miejsca w kolejce. Aktywny toast nie jest zastępowany.
Po jego wyjściu następny płynnie rozszerza się z tylnej karty i dostaje własne 4 s.
Po 4 sekundach toast znika przez Presence/POPOVER. Hover zatrzymuje timeout,
a opuszczenie wznawia pozostały czas bez resetowania. X pojawia się na hover.
Przeciągnięcie w bok o co najmniej 64 px zamyka toast; krótszy gest animuje powrót.
Dolna krawędź wypełnia się od lewej do prawej zgodnie z upływem 4 sekund. Generacja timera chroni
nowy komunikat przed spóźnionym zamknięciem poprzedniego.

Reduce Motion pomija animację. Klatki są żądane podczas wejścia/wyjścia, gestu i aktywnego odliczania paska;
w spoczynku jest najwyżej jeden timer. Nie ma zapisu w SQLite ani
stałej pętli odrysowań. Gotowe Notification z GPUI Component ma własne sztywne
animacje; używamy jego natywnej bazy Toast ze wspólnym motion Canopy.
