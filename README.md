# Stream Goat (Rust)

A native rewrite of the Stream Goat desktop app: movies, series, anime and Live TV
on Linux (and Windows/macOS), written in Rust, **while still running the original
CloudStream extensions**.

```
┌────────────────────────────── Rust process ──────────────────────────────┐
│  egui window                                                             │
│   ├── Stremio add-ons      (HTTP + JSON, native)                          │
│   ├── Live TV              (M3U, Xtream Codes, XMLTV, native)             │
│   ├── Downloads/library    (native)                                      │
│   └── Player               (mpv; libVLC later)                            │
└───────────────┬──────────────────────────────────────────────────────────┘
                │  JSON-RPC over 127.0.0.1 (Bearer token)
┌───────────────▼──────────── JVM sidecar ─────────────────────────────────┐
│  CloudStream engine, unchanged: dex2jar + URLClassLoader + MainAPI        │
│  (`host/kotlin/ExtensionHost.kt` in this repo)                            │
└──────────────────────────────────────────────────────────────────────────┘
```

Why the split: a `.cs3` extension is Dalvik bytecode linked against CloudStream's
Kotlin API and Android stubs (Jackson, Jsoup, OkHttp, coroutines, NewPipe). Anything
that runs them is a JVM. So the JVM stays, as a sidecar, and everything else is
Rust. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and
[docs/host-protocol.md](docs/host-protocol.md).

## Workspace

| Crate | What it is |
|---|---|
| `crates/core` | `streamgoat-core`: Stremio, Live TV, EPG, mpv, settings, host client, link resolution |
| `crates/cli` | `streamgoat` command line (inspect sources, play, talk to the host) |
| `crates/ui` | `streamgoat-ui`: the desktop app (egui/eframe) |
| `host/` | the JVM extension host (Kotlin) and how to build it into the existing app |
| `skills/` | the project skill for coding agents (`skills/stream-goat-rust/SKILL.md`) |

## Status

Working: playlist and Xtream parsing, XMLTV guides, Stremio catalogs/meta/streams,
the host protocol and client, settings, mpv playback, the CLI, a first egui UI.
Next: extension/repository management in the UI, downloads, subtitles UI, the Kotlin
app's look (glass, pills, themes), Flatpak/AppImage packaging.

## Build

Requires Rust 1.82+ and `mpv`.

```sh
cargo build --release            # everything
cargo test --workspace           # unit tests (no network)
cargo run -p streamgoat-ui       # the app
```

The app looks for settings in `~/.config/streamgoat/settings.json` (or
`$XDG_CONFIG_HOME/streamgoat`).

## Command line

```sh
streamgoat stremio manifest https://v3-cinemeta.strem.io/manifest.json
streamgoat stremio catalog  https://v3-cinemeta.strem.io movie top
streamgoat stremio streams  https://v3-cinemeta.strem.io movie tt1160419 --play

streamgoat live m3u https://iptv-org.github.io/iptv/countries/es.m3u --search "la 1"
streamgoat live xtream http://panel:8080 user pass --categories

streamgoat epg https://example.com/guide.xml --channel tve.es --limit 10
streamgoat play "https://cdn/x.m3u8" --header "Referer: https://site/"

streamgoat host --addr 127.0.0.1:41235 --token "$TOKEN" providers
streamgoat host --addr 127.0.0.1:41235 --token "$TOKEN" links Cine https://provider/x --play 1
```

## Configuration

`settings.json`:

```json
{
  "host_addr": "127.0.0.1:41235",
  "host_token": "…",
  "host_command": "java -jar /opt/streamgoat/streamgoat-host.jar",
  "addons": ["https://v3-cinemeta.strem.io/manifest.json"],
  "live_sources": [
    { "kind": "m3u", "name": "España", "spec": "https://iptv-org.github.io/iptv/countries/es.m3u" },
    { "kind": "xtream", "name": "Mi panel", "base": "http://panel:8080", "username": "u", "password": "p" }
  ],
  "epg_url": null,
  "player_command": "mpv"
}
```

## License

GPL-3.0-or-later, like Stream Goat itself. Stream Goat is created by Syed Ali Hamza;
this rewrite keeps the same licence and the same "player only, you bring the sources"
disclaimer.
