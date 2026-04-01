# SPEC: Browser Credential Store

## Wymagania

- Prosty credential store per-domena (login + hasło)
- **Zapis**: auto-detect submit formularza logowania → prompt "Save password?" + możliwość ręcznego zarządzania w Settings
- **Autofill**: ikona klucza w polu password → klik wypełnia formularz
- Hasła szyfrowane via `safeStorage` (OS keychain)
- Przechowywane w SQLite (nowa tabela `credentials`)
- Sekcja w Settings "Web Browser" → zarządzanie zapisanymi credentials

## Security Model

- Hasła szyfrowane at rest przez `safeStorage` (macOS Keychain / Windows Credential Store)
- Odszyfrowanie tylko w momencie autofill (na żądanie użytkownika)
- Brak preload script w webview — detekcja i wstrzykiwanie przez `executeJavaScript()`
- Hasło przechodzi: SQLite (encrypted) → main process (decrypted) → IPC → renderer → executeJavaScript (inject do formularza)
- **Znane ryzyko**: odszyfrowane hasło żyje chwilowo w pamięci Node.js/renderer. Akceptowalne dla uproszczonego credential store.

## Analiza złożoności

- **Scope:** duży
- **Pliki:** ~8 (DB migration, CredentialStore, IPC, preload, BrowserManager, BrowserPane, BrowserToolbar, Settings)
- **Główne obszary:** main (storage + encryption), renderer (detection + UI), IPC bridge

---

## Plan implementacji

### Faza 1: CredentialStore (main process)

**Cel:** Tabela SQLite + szyfrowany CRUD z safeStorage.

**Lokalizacje:**

- `src/main/db/Database.ts` — nowa migracja
- `src/main/db/CredentialStore.ts` — nowy plik

**Zadania:**

- [ ] Migracja #N: tabela `credentials (id TEXT PK, domain TEXT NOT NULL, username TEXT NOT NULL, password_enc TEXT NOT NULL, created_at, updated_at)` + unique index na `(domain, username)`
- [ ] `CredentialStore` klasa:
  - `getForDomain(domain): Credential[]` — decrypt passwords
  - `save(domain, username, password)` — encrypt + upsert
  - `delete(id)`
  - `getAll(): Credential[]` — lista do Settings (hasła masked)
  - `getAllDecrypted(): Credential[]` — z odszyfrowanymi hasłami (do exportu/edycji)

**Wzorzec:** jak `PreferencesStore` z `safeStorage.encryptString/decryptString`

**Tech notes:**

- **Security:** `safeStorage.encryptString(password).toString('base64')` do zapisu, `safeStorage.decryptString(Buffer.from(enc, 'base64'))` do odczytu
- **Gotcha:** `safeStorage.isEncryptionAvailable()` może być false na Linuxie bez keyring — fallback do plain text z warning
- **Walidacja:** unit test encrypt→decrypt round-trip

---

### Faza 2: IPC bridge

**Cel:** Expose credential operations do renderera.

**Lokalizacje:**

- `src/main/ipc/handlers.ts` — nowe handlery
- `src/preload/index.ts` + `index.d.ts` — nowe metody

**Zadania:**

- [ ] IPC handlery: `credentials:getForDomain`, `credentials:save`, `credentials:delete`, `credentials:getAll`
- [ ] Preload bridge: `getCredentials(domain)`, `saveCredential(domain, username, password)`, `deleteCredential(id)`, `listCredentials()`
- [ ] Instancja CredentialStore w `index.ts` (obok preferencesStore)

**Tech notes:**

- **Security:** `credentials:getForDomain` zwraca odszyfrowane hasła — wywołanie TYLKO z renderera (contextIsolation chroni)
- **Walidacja:** IPC round-trip działa

---

### Faza 3: Detekcja formularzy logowania

**Cel:** Po załadowaniu strony wykryj pola password i oferuj autofill.

**Lokalizacje:**

- `src/renderer/src/components/browser/BrowserPane.svelte` — inject detection + icon
- `src/main/browser/BrowserManager.ts` — opcjonalnie (detekcja w main)

**Zadania:**

- [ ] Po `did-stop-loading` w BrowserPane: inject JS do wykrycia `input[type="password"]`
- [ ] Jeśli password field znaleziony + credentials istnieją dla domeny → inject ikona klucza (absolutnie pozycjonowana obok pola)
- [ ] Klik na ikonę: pobierz credential z main → inject wartości do formularza (`input.value = x` + dispatch InputEvent)
- [ ] Jeśli >1 credential dla domeny: pokaż mini-dropdown z wyborem usera

**Wzorzec JS injection:**

```javascript
// Detekcja (inject po did-stop-loading)
const pwFields = document.querySelectorAll('input[type="password"]')
return pwFields.length > 0 ? { domain: location.hostname, count: pwFields.length } : null

// Autofill icon injection
const icon = document.createElement('div')
icon.style.cssText = '...'
icon.onclick = () => window.__canopyFillCredential?.()
pwField.parentElement.style.position = 'relative'
pwField.parentElement.appendChild(icon)

// Fill (osobny executeJavaScript call)
const userField = document.querySelector(
  'input[type="email"],input[type="text"],input[name*="user"]',
)
if (userField) {
  userField.value = USERNAME
  userField.dispatchEvent(new Event('input', { bubbles: true }))
}
pwField.value = PASSWORD
pwField.dispatchEvent(new Event('input', { bubbles: true }))
```

**Tech notes:**

- **Gotcha:** `input.value = x` nie wystarczy dla React/Vue — potrzebny `dispatchEvent(new Event('input', {bubbles:true}))` żeby framework zobaczył zmianę
- **Gotcha:** SPA-style loginy mogą ładować formularz po `did-stop-loading` — rozważ MutationObserver
- **Security:** hasło jest wstrzykiwane do DOM strony — to jedyny moment exposure
- **Walidacja:** autofill działa na google.com, github.com

---

### Faza 4: Save password prompt

**Cel:** Po submit formularza z password fieldem → pokaż banner "Save password?"

**Lokalizacje:**

- `src/renderer/src/components/browser/BrowserPane.svelte` — banner UI + grab logic

**Zadania:**

- [ ] Na `will-navigate` webview event: jeśli strona miała password field, `executeJavaScript` żeby pobrać wartości (username, password)
- [ ] Jeśli udało się pobrać: pokaż banner pod toolbarem "Save password for {domain}? [Save] [Never]"
- [ ] Save: wywołaj `window.api.saveCredential(domain, username, password)`
- [ ] State: `savePrompt: { domain, username, password } | null`

**Tech notes:**

- **Gotcha:** `will-navigate` może być za późno — strona mogła już wyczyścić formularz. Alternatywa: inject listener na `submit` event wcześniej
- **Wzorzec UI:** thin banner (32px) pod toolbarem, auto-dismiss po 10s
- **Edge case:** strona z wieloma password fields (np. "change password") — pomiń
- **Walidacja:** submit login form → banner się pojawia

---

### Faza 5: Zarządzanie credentials w Settings

**Cel:** Sekcja w Web Browser settings z listą zapisanych credentials.

**Lokalizacje:**

- `src/renderer/src/components/preferences/ViewportsPrefs.svelte` — nowa subsection

**Zadania:**

- [ ] Dodaj subsection "Saved Passwords" w Web Browser settings
- [ ] Lista: domain | username | ••••• | [Delete]
- [ ] Opcja "Show password" (klik odsłania na 3s, potem maskuje)
- [ ] Pobranie listy: `window.api.listCredentials()`

**Tech notes:**

- **Wzorzec:** jak ToolPrefs (lista z remove button)
- **Security:** hasła domyślnie masked, odsłonięcie po kliknięciu z auto-hide
- **Walidacja:** credentials widoczne w Settings, usuwanie działa

---

### Faza 6: Toolbar indicator

**Cel:** Ikona klucza w toolbar gdy credentials istnieją dla aktualnej domeny.

**Lokalizacje:**

- `src/renderer/src/components/browser/BrowserToolbar.svelte` — ikona
- `src/renderer/src/components/browser/BrowserPane.svelte` — prop

**Zadania:**

- [ ] Prop `hasCredentials: boolean` w Toolbar
- [ ] Ikona Key obok Star — aktywna (niebieska) gdy credentials istnieją
- [ ] Klik → trigger manual autofill (gdy auto-detect nie zadziałał)

**Tech notes:**

- **Walidacja:** ikona widoczna na stronach z zapisanymi credentials

---

## Weryfikacja końcowa

- [ ] `npm run typecheck` + `npm run lint` — clean
- [ ] Manual: zaloguj się na stronie → banner "Save?" → zapisz
- [ ] Manual: odwiedź tę samą stronę → ikona klucza w password field → klik wypełnia
- [ ] Manual: Settings > Web Browser > Saved Passwords — widać wpis
- [ ] Manual: usuń credential z Settings → autofill przestaje działać
- [ ] Manual: restart app → credentials zachowane
- [ ] Security: credentials w SQLite są zaszyfrowane (sprawdź plik DB ręcznie)
