# Host protocol (Rust UI ⟷ JVM extension host)

The Rust app never runs Dalvik bytecode. CloudStream `.cs3` extensions keep running on
a JVM, in a **separate process** that the Rust app starts and talks to over loopback
HTTP. This document is the contract between the two halves; both sides are in this
repo (`crates/core/src/host.rs` and `host/kotlin/ExtensionHost.kt`).

## Transport

* `POST http://127.0.0.1:<port>/rpc`
* Header `Authorization: Bearer <token>`
* Content type `application/json`

Request:

```json
{ "id": 1, "method": "providers.search", "params": { "provider": "Cine", "query": "dune" } }
```

Reply, exactly one of:

```json
{ "id": 1, "result": { "items": [] } }
{ "id": 1, "error": { "code": -1, "message": "provider not found" } }
```

`code` is for humans; the client shows `message`. Errors inside an extension are
reported as `error`, never as a broken reply.

## Startup handshake

The host is started by the app (`settings.host_command`, default
`java -jar streamgoat-host.jar`). On stdout, **one line of JSON**, then nothing but
logs:

```json
{"addr":"127.0.0.1:41235","token":"c1f4…"}
```

* `--port 0` (default) lets the kernel pick a free port.
* `--token` is optional; when missing the host generates a random one, so an
  unauthenticated process cannot reach it.
* Logs go to stderr; the app never parses them.

## Methods

All media items share one shape (camelCase, the Kotlin side emits it directly):

```json
{
  "id": "host:Cine:abc123",
  "name": "Dune",
  "url": "https://provider.example/dune",
  "poster": "https://img/dune.jpg",
  "background": null,
  "description": "…",
  "year": 2021,
  "type": "movie",
  "provider": "Cine",
  "episodes": []
}
```

`type` is `movie`, `series`, `anime`, `live` or `other`. `url` is the provider's
opaque data url: the same string the extension gave us, never interpreted by Rust.

Episodes:

```json
{ "name": "Pilot", "url": "https://provider.example/dune/s1e1",
  "season": 1, "episode": 1, "thumbnail": null, "description": null }
```

Links:

```json
{ "url": "https://cdn/x.m3u8", "name": "Server 1", "quality": "1080p",
  "source": "Cine", "referer": "https://provider.example/",
  "headers": { "User-Agent": "…" }, "type": "hls",
  "subtitles": [ { "lang": "es", "url": "https://cdn/es.vtt", "name": null } ] }
```

`type` is `hls`, `dash`, `mp4`, `torrent` or `external`.

### `health`

```json
→ {} 
← { "version": "0.1.0", "providers": 141, "plugins": 12 }
```

### `providers.list`

```json
→ {}
← { "providers": [ { "id": "Cine", "name": "Cine", "plugin": "CineProvider.cs3",
                     "language": "es", "tvTypes": ["Movie"], "iconUrl": null } ] }
```

`id` is what every other method takes: the app uses the provider name, which is
unique because `Engine.provider(name)` resolves it case-insensitively.

### `providers.home`

```json
→ { "provider": "Cine", "page": 1 }
← { "items": [ … ], "hasNext": true }
```

Flattens every enabled `Catalogs` section of that provider for the page.

### `providers.search`

```json
→ { "provider": "Cine", "query": "dune" }
← { "items": [ … ] }
```

### `providers.quickSearch`

```json
→ { "query": "dune" }
← { "items": [ … ] }        // every provider at once
```

### `providers.load`

```json
→ { "provider": "Cine", "url": "https://provider.example/dune" }
← { "item": { … with "episodes": [ … ] } }
```

Series and anime return their episode list; movies and live streams return `[]`.

### `providers.links`

```json
→ { "provider": "Cine", "url": "https://provider.example/dune" }
← { "links": [ … ] }
```

Uses `Engine.allLinks`, so the 20-minute link cache of the Kotlin side is shared
with anything else that asks for the same episode.

### `extensions.list`

```json
→ {}
← { "plugins": [ { "name": "Cine", "internalName": "CineProvider", "version": 7,
                   "url": "https://repo.example/plugins.json", "providerCount": 3 } ] }
```

## Why not just reimplement this in Rust?

Because it is not HTTP: a `.cs3` is a zip with `classes.dex`, translated to JVM
bytecode (`dex2jar` + ASM) and linked against CloudStream's Kotlin `library`
(`MainAPI`, `ExtractorLink`, `APIHolder`) plus Android stubs. Extensions also use
Jackson, Jsoup, OkHttp, NewPipe Extractor and kotlinx-coroutines. Anything that runs
them is a JVM; the only real question is which language writes the window. Rust does.
