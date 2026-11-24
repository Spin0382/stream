# Architecture

Stream Goat (Kotlin) is a JavaFX window plus a CloudStream engine. This repo keeps
the engine and replaces the window. Everything here is about keeping that boundary
honest, so neither half grows into the other.

## The boundary

```
Rust (crates/core, crates/cli, crates/ui)          JVM (host/kotlin/ExtensionHost.kt)
──────────────────────────────────────────         ──────────────────────────────────
SQLite-less JSON settings/library                  Platform.init(), Network.init()
Stremio add-ons            (HTTP/JSON)             PluginManager.loadAll()  ← dex2jar,
Live TV: M3U / Xtream / XMLTV                      │                          URLClassLoader
Player: mpv subprocess     (VOD + Live)            │  CloudStream `library` (MainAPI,
Downloads                  (later)                 │  ExtractorLink, APIHolder), Android
UI: egui                                           │  stubs, OkHttp, Jsoup, Jackson
                                                   Engine.homePage/search/load/allLinks
        ── JSON-RPC on 127.0.0.1, Bearer token ──►
```

Rules:

1. **No bytecode, no Android, no CloudStream types in Rust.** The Rust side only
   sees the wire model in `docs/host-protocol.md`.
2. **No UI, no playlist parsing, no mpv in Kotlin.** The host is headless: it loads
   extensions and answers questions.
3. **Every new feature goes on the side that does not need the other.** Live EPG,
   Stremio and downloads never call the host.

## Why not decode `.cs3` in Rust?

A `.cs3` is a zip with `classes.dex`. Running it needs:

* a dex → JVM-bytecode translator (`dex2jar`), plus ASM rewriting for Kotlin's
  inline classes (`constructor-impl` names);
* CloudStream's `library` compiled for the JVM with the exact same signatures the
  extensions were built against (upstream commit pinned in the Kotlin repo);
* Android stubs (`android.content.Context`, `SharedPreferences`, `android.net.Uri`,
  `android.util.Base64`, …) — 25 classes in `android-shim`;
* extension dependencies resolved from the host classpath: Jackson, Jsoup, OkHttp,
  kotlinx-coroutines, NewPipe Extractor, xmlpull.

Reimplementing that in Rust means writing a Dalvik VM and a re-entrant coroutine
runtime for code we do not control. Embedding a JVM in the Rust process (JNI) would
work but produces a single binary that is two runtimes glued together, with a harder
crash story. A sidecar process is the boring, debuggable option: extensions can
crash, hang or leak and the UI survives (`Engine` already runs them on their own
thread pool and bounds every call with a timeout).

## Data flow

**Extension item**

1. UI: `HostClient::home(provider, page)` → `providers.home`.
2. Card shows `name`, `poster`, `year`, `type`.
3. Click → `resolve::details()` → `providers.load` → episodes for series.
4. Click a link/episode → `resolve::links()` → `providers.links` (shares the
   Kotlin 20-minute cache).
5. Rust picks a `StreamLink` and hands it to mpv with `--http-header-fields`.

**Stremio item**: `AddonClient` in Rust does the three calls itself; `MediaItem.url`
is `stremio:<base>#<type>#<id>` so `resolve` can rebuild the request.

**Live channel**: M3U/Xtream parsing in Rust; playing is a `StreamLink` with the
channel url (and any `#EXTVLCOPT` headers).

## Failure behaviour

| Situation | What the user sees | What keeps working |
|---|---|---|
| Host not running | "Conectar host" fails with a clear message | Stremio, Live TV, settings |
| An extension hangs | the call times out (`Engine` bounds it) | every other provider |
| The host crashes | UI stays alive; the app can restart it | all native features |
| A playlist is unreachable | the source's error, per source | other sources |
| mpv missing | "no se pudo abrir el reproductor" | listing, search |

## Roadmap

1. **Now**: CLI + core + first UI, host protocol, mpv.
2. Extension management in the UI: repositories, install/update/uninstall through
   the host (`extensions.*` methods).
3. Downloads: multi-connection, resumable, with the host resolving links only.
4. Player polish: libmpv in-process (subtitles, skip intro, next-episode countdown,
   audio/subtitle sync) — libmpv is C, so no JVM involved.
5. Look: the Kotlin app's theme, glass panels, pill navigation, start animation.
6. Packaging: Flatpak + AppImage + `.deb`, bundling the host jar and a jlink JRE.
