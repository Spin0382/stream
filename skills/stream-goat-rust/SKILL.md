---
name: stream-goat-rust
description: Rules for the Stream Goat Rust port (repo Spin0382/stream): a Rust desktop app plus a JVM sidecar that runs CloudStream extensions. Load it before touching crates/core, crates/cli, crates/ui, the host protocol, or the CI workflow. Covers the Rust/JVM boundary, the protocol contract, code and naming conventions, the compile-only-in-CI loop, and the pitfalls already hit in this port.
---

# Stream Goat (Rust port)

A native rewrite of the Stream Goat desktop app. Rust owns the window and every
source that speaks HTTP. The CloudStream engine keeps running on a JVM, in a
sidecar process, because `.cs3` extensions are Dalvik bytecode linked against
CloudStream's Kotlin API.

```
Rust process (egui)                      JVM sidecar (host/kotlin)
  Stremio add-ons                          Platform.init()
  Live TV (M3U, Xtream, XMLTV)             PluginManager.loadAll()  dex2jar + URLClassLoader
  mpv player                               Engine.homePage/search/load/allLinks
  settings, library                        CloudStream library + Android stubs
        JSON-RPC on 127.0.0.1, Bearer token
```

## Hard rules

1. No bytecode, no Android, no CloudStream types in Rust. The Rust side only sees
   the wire model in `docs/host-protocol.md`.
2. No UI, no playlist parsing, no mpv calls in Kotlin. The host is headless.
3. A new feature goes on the Rust side unless it genuinely needs extensions.
   Live TV, EPG, Stremio and downloads must never call the host.
4. Adding an RPC method is one commit that changes three places: `docs/host-protocol.md`,
   `crates/core/src/host.rs` (typed wrapper) and `host/kotlin/ExtensionHost.kt`.
5. Never start or embed a JVM inside the Rust process. If something needs the JVM,
   it is a host method, not a dependency.

## Layout

| Path | Contents |
|---|---|
| `crates/core` | Stremio, Live TV, EPG, mpv, settings, host client, `resolve` |
| `crates/cli` | `streamgoat` binary: inspect sources, play, talk to the host |
| `crates/ui` | `streamgoat-ui`: egui window, `app.rs` state, `panels.rs` rendering |
| `host/kotlin` | the JVM host, added to the Kotlin app that owns the extensions |
| `docs/` | architecture and protocol. Update them with the code |

## Compile loop: CI only

The development machine must not run `cargo build`, `cargo clippy`, `cargo test`,
`cargo run`, or anything that creates `target/`. There is no disk space for it.
Push and read the run.

Cheap local checks that do not compile anything:

```sh
cargo metadata --no-deps --format-version 1 >/dev/null   # manifests are valid
rustfmt --edition 2021 $(find crates -name '*.rs')       # parses every file, rewrites nothing else
```

Reading the CI result (token in `/home/spin/.config/Stream-CI-token`):

```sh
TOK=$(cat /home/spin/.config/Stream-CI-token)
curl -s -H "Authorization: Bearer $TOK" \
  "https://api.github.com/repos/Spin0382/stream/actions/runs?per_page=2" |
  python3 -c 'import sys,json;[print(r["id"],r["status"],r["conclusion"],r["head_sha"][:7]) for r in json.load(sys.stdin)["workflow_runs"]]'
```

Logs only exist once a run has finished. Download the whole run and keep only the
failed steps:

```sh
curl -sL -H "Authorization: Bearer $TOK" \
  "https://api.github.com/repos/Spin0382/stream/actions/runs/<RUN_ID>/logs" -o /tmp/l.zip
cd /tmp && unzip -q l.zip -d run && sed -e 's/\x1b\[[0-9;]*m//g' run/*/7_Clippy.txt | grep -n "^error" -A 12
```

Pushing uses the token, not the stored `dusk0382` credentials:

```sh
GIT_ASKPASS=/home/spin/Documentos/Cloudstream-PC/.askpass.sh GIT_TERMINAL_PROMPT=0 \
  git -c credential.helper= push origin main
```

## Conventions in force

- Errors: one enum in `crates/core/src/error.rs`, `#[from]` where the source maps
  directly, `Error::Missing` for configuration and `Error::Host` for RPC errors.
- JSON helpers take `&Value`, and are named `json_string` / `json_i64`, so call
  sites read `entry.get("name").and_then(json_string)`.
- Closed sets are enums (`MediaKind`, `StreamKind`, `LiveSource`), never trait objects.
- Parsers are pure functions over `&str` (`parse_playlist`, `parse_xmltv`), so tests
  need no network and no runtime.
- Naming follows the `as_` (free view), `to_` (allocates), `into_` (consumes) promise.
- `[workspace.lints]` in `Cargo.toml` plus `clippy.toml` (from BeautifulRust) are
  authoritative. Fix the warning, or `#[expect(..., reason = "...")]`.
- User facing strings (UI, CLI output) are Spanish. Code, comments and docs are English.
- No em dashes anywhere, in prose, comments or commit messages.

## Pitfalls already hit

- `chrono` needs the `serde` feature for any type that derives `Serialize`.
- `clap` needs the `env` feature for `#[arg(env = "...")]`.
- `eframe`: `main.rs` declares `mod app; mod panels;`, and `app.rs` needs
  `use crate::panels;` to reach the panels module.
- Borrowing an `App` field and then calling a `&mut self` method fails. Clone the
  small thing (a link, an item) first, then act.
- `Guide::on_channel` returns a `Vec<&Programme>` on purpose: an iterator would keep
  the `&str` borrow alive and force the caller to own the channel id.
- `#EXTGRP` appears both before and after its `#EXTINF` in real playlists. Both
  orders must work, and the sample playlist in the test covers both.
- A new dependency must earn its place: the crates in use are serde, serde_json,
  thiserror, reqwest (rustls), chrono, tokio, clap, dirs, tracing, eframe/egui.

## Before saying a task is done

1. `rustfmt` clean and `cargo metadata` clean locally.
2. Pushed, and the CI run for that commit is green (`fmt + clippy + test` and
   `release build`).
3. If the change touches the protocol, both sides were updated in the same commit
   and `docs/host-protocol.md` matches the code.
4. State plainly what was verified in CI and what was not (the GUI has no runner,
   so it is reviewed, not tested, until packaging lands).
