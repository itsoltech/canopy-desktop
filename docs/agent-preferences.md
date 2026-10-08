# Preferencje agentów

Widoki Claude, Codex, Gemini i OpenCode odwzorowują strukturę formularzy Electrona:
lista profili 180 px, odstęp 20 px, nagłówek z nazwą i zapisem, przewijane sekcje.
Używają wspólnych tokenów, PrefsRow (inline/stacked), Input, Select i Textarea.
Przełączenie profilu korzysta z CONTENT_REVEAL, a dodawanie zmiennej z Disclosure.

| Agent | Pola |
| --- | --- |
| Claude | Model, Permission mode, Effort level, API key, Base URL, Provider, Append to system prompt, Environment variables, Settings JSON override |
| Codex | Model, Approval mode, Sandbox, Full auto, Bypass approvals and sandbox, Profile, API key, Base URL, Environment variables, Settings JSON override |
| Gemini | Model, Approval mode, API key, Environment variables, Settings JSON override |
| OpenCode | Model, API key, Environment variables, Config JSON override |

Profile zachowują stabilne ID. Save zapisuje ich ustawienia; istniejące profile
bez pola `settings` dostają domyślne wartości w pamięci. Dane draftu oraz encje
kontrolek należą do ToolsPreferences / AgentForm / EnvironmentEditor.

## Runtime

- Claude: model, permission, effort, append-system-prompt i JSON jako osobne argv;
  API key, endpoint i provider jako env.
- Codex: approval/sandbox/profile jako argv. Full auto rozwija się do
  `--sandbox workspace-write --ask-for-approval on-request`, bo lokalny CLI
  0.153.4 nie obsługuje już `--full-auto`. Bypass ma pierwszeństwo nad tymi opcjami.
  Usuniętego z tego CLI `untrusted` nie oferujemy w nowym dropdownie.
- Codex JSON przyjmuje dokument z `hooks` i opcjonalnym `description`; `hooks`
  jest przekazywane jako per-process `--config hooks=<TOML>`. Nie zmieniamy
  CODEX_HOME, plików auth ani globalnych hooks.json. Hooks podlegają normalnej
  kontroli zaufania CLI; Canopy nie omija jej.
- Gemini: model/approval jako argv, API key/env dla procesu. JSON jest scalany
  z ustawieniami użytkownika w prywatnym tymczasowym `.gemini/settings.json`.
  Na Unix pozostałe istniejące pliki konfiguracyjne są linkowane symbolicznie.
  Na Windows proces dostaje prywatny `USERPROFILE`, a wyłącznie pliki auth z
  allowlisty są hardlinkowane, aby odświeżenia credentiali zachować w oryginale
  bez wymagania Developer Mode. Układ profilu i danych musi być na jednym
  woluminie. Katalog jest utrzymany do zakończenia procesu i sprzątany przez
  supervisor PTY.
- OpenCode: model jako argv, klucz Anthropic jako env, JSON przez
  OPENCODE_CONFIG_CONTENT. Innych providerów można skonfigurować zmiennymi.

Mapowanie opiera się na adapterach Electrona i pomocy zainstalowanych CLI.
Inline hooks są opisane w [dokumentacji Codex](https://learn.chatgpt.com/docs/hooks).
Układ Gemini home jest zgodny z [konfiguracją Gemini CLI](https://geminicli.com/docs/reference/configuration/).
Ustawienia obowiązują nowe starty i Restart; nie zmieniają działających procesów.
Nie uruchamiamy zapytań do modeli podczas testów formularzy.

## API keys

Pole jest maskowane. Puste pole zachowuje istniejący klucz; osobna kontrolka
usuwa odwołanie do zapisanego klucza. Nowy klucz trafia do macOS Keychain lub
Windows Credential Manager pod nowym losowym identyfikatorem. Oba backendy mają
oddzielny namespace od tokenów integracji i limit 2560 bajtów. SQLite przechowuje
wyłącznie identyfikator. Błąd zapisu SQLite próbuje usunąć nowy wpis, zachowując
poprzedni. Nieudany cleanup pozostaje jawny i zapisuje identyfikator do retry;
po udanym zapisie nieużywane poprzednie wpisy są sprzątane. Całość działa poza
wątkiem UI. Plaintext fallback nie istnieje.
Zaszyfrowanych kluczy ze starego safeStorage nie importujemy automatycznie.

Zmienne środowiskowe, podobnie jak w starym Canopy, są ustawieniami jawnymi w
SQLite (mimo maskowania w UI). Do klucza dostawcy służy pole API key.
Logowanie i pierwszeństwo źródeł uwierzytelnienia pozostają zasadami danego CLI.
Nie zmieniamy globalnego logowania użytkownika.

## Sprawdzenia

Testy: walidacja JSON/enum/env, literalne argv, priorytet Full auto/Bypass,
zgodność starszych rekordów, roundtrip SQLite, izolacja JSON i zachowanie
oryginalnych plików oraz trwałość kolejki cleanupu. Oddzielne testy natywne na
macOS i Windows sprawdzają zapis, aktualizację, odczyt i usunięcie jednorazowych
wpisów systemowego magazynu, bez prawdziwego API key. Windowsowy test pozostaje
do uruchomienia na natywnym runnerze.
