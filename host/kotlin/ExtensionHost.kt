/*
 * Stream Goat — JVM extension host.
 *
 * This file belongs in the Kotlin app (the CloudStream-for-Linux codebase this repo
 * replaces the UI of). Suggested location:
 *
 *   app/src/main/kotlin/com/lagradost/cloudstream3/linux/host/ExtensionHost.kt
 *
 * It is *not* compiled by the Rust workspace: the Rust app starts it as a sidecar
 * process and talks to it over the JSON-RPC loopback protocol described in
 * docs/host-protocol.md.
 *
 * Everything valuable from the Kotlin side is reused as-is: `Platform.init()`,
 * `PluginManager.loadAll()` (dex2jar + URLClassLoader + CloudStream API),
 * `Engine`/`Catalogs` for home/search/load/links. Nothing about extensions is
 * reimplemented in Rust.
 */

package com.lagradost.cloudstream3.linux.host

import com.lagradost.cloudstream3.AnimeLoadResponse
import com.lagradost.cloudstream3.Episode
import com.lagradost.cloudstream3.LiveStreamLoadResponse
import com.lagradost.cloudstream3.LoadResponse
import com.lagradost.cloudstream3.MainAPI
import com.lagradost.cloudstream3.MovieLoadResponse
import com.lagradost.cloudstream3.SearchResponse
import com.lagradost.cloudstream3.SubtitleFile
import com.lagradost.cloudstream3.TorrentLoadResponse
import com.lagradost.cloudstream3.TvSeriesLoadResponse
import com.lagradost.cloudstream3.TvType
import com.lagradost.cloudstream3.linux.Catalogs
import com.lagradost.cloudstream3.linux.Engine
import com.lagradost.cloudstream3.linux.Network
import com.lagradost.cloudstream3.linux.Platform
import com.lagradost.cloudstream3.linux.PluginManager
import com.lagradost.cloudstream3.mapper
import com.lagradost.cloudstream3.utils.ExtractorLink
import com.lagradost.cloudstream3.utils.ExtractorLinkType
import com.fasterxml.jackson.databind.JsonNode
import com.sun.net.httpserver.HttpExchange
import com.sun.net.httpserver.HttpServer
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeoutOrNull
import java.net.InetSocketAddress
import java.security.SecureRandom
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors

private const val DEFAULT_TIMEOUT = 45_000L

fun main(args: Array<String>) {
    val port = args.valueAfter("--port")?.toIntOrNull() ?: 0
    val token = args.valueAfter("--token") ?: randomToken()

    Platform.init()
    Network.init()
    runBlocking { PluginManager.loadAll() }.forEach { (name, error) ->
        System.err.println("warn: $name failed to load: $error")
    }

    val server = HttpServer.create(InetSocketAddress("127.0.0.1", port), 0)
    server.createContext("/rpc") { exchange -> serve(exchange, token) }
    server.executor = Executors.newFixedThreadPool(8)
    server.start()

    // The single line the Rust app reads to know where to connect.
    println(mapper.writeValueAsString(mapOf("addr" to "127.0.0.1:${server.address.port}", "token" to token)))
    System.out.flush()

    CountDownLatch(1).await() // run until killed
}

private fun Array<String>.valueAfter(flag: String): String? =
    indexOf(flag).takeIf { it >= 0 }?.let { getOrNull(it + 1) }

private fun randomToken(): String {
    val bytes = ByteArray(24)
    SecureRandom().nextBytes(bytes)
    return bytes.joinToString("") { "%02x".format(it) }
}

private fun serve(exchange: HttpExchange, token: String) {
    var id = 0L
    try {
        if (exchange.requestHeaders.getFirst("Authorization") != "Bearer $token") {
            reply(exchange, 401, mapOf("error" to mapOf("code" to 401, "message" to "bad token")))
            return
        }
        val body = exchange.requestBody.use { it.readBytes().toString(Charsets.UTF_8) }
        val request = mapper.readTree(body)
        id = request.get("id")?.asLong() ?: 0L
        val method = request.get("method")?.asText() ?: ""
        val params = request.get("params") ?: mapper.createObjectNode()
        val result = runBlocking { dispatch(method, params) }
        reply(exchange, 200, mapOf("id" to id, "result" to result))
    } catch (t: Throwable) {
        t.printStackTrace()
        reply(exchange, 200, mapOf("id" to id, "error" to mapOf("code" to -1, "message" to (t.message ?: t.toString()))))
    }
}

private fun reply(exchange: HttpExchange, status: Int, payload: Map<String, Any?>) {
    val bytes = mapper.writeValueAsBytes(payload)
    exchange.responseHeaders.add("Content-Type", "application/json; charset=utf-8")
    exchange.sendResponseHeaders(status, bytes.size.toLong())
    exchange.responseBody.use { it.write(bytes) }
}

private suspend fun dispatch(method: String, params: JsonNode): Any? = when (method) {
    "health" -> mapOf(
        "version" to System.getProperty("streamgoat.version", "0.1.0"),
        "providers" to Engine.allProviders().size,
        "plugins" to PluginManager.installed().size,
    )

    "providers.list" -> mapOf(
        "providers" to Engine.allProviders().map { api ->
            mapOf(
                "id" to api.name,
                "name" to api.name,
                "plugin" to api.sourcePlugin,
                "language" to api.lang,
                "tvTypes" to api.supportedTypes.map { it.name },
                "iconUrl" to api.iconUrl,
            )
        },
    )

    "extensions.list" -> mapOf(
        "plugins" to PluginManager.installed().map { plugin ->
            mapOf(
                "name" to (plugin.name ?: plugin.internalName),
                "internalName" to plugin.internalName,
                "version" to plugin.version,
                "url" to plugin.repositoryUrl,
                "providerCount" to Engine.allProviders().count { it.sourcePlugin == plugin.internalName },
            )
        },
    )

    "providers.home" -> {
        val api = provider(params)
        val page = params.get("page")?.asInt() ?: 1
        val items = mutableListOf<Map<String, Any?>>()
        var hasNext = false
        for (entry in Catalogs.enabled(api)) {
            val response = Engine.homePage(api, entry.section, page) ?: continue
            response.items.forEach { row -> row.list.forEach { items += itemJson(it, api.name) } }
            if (response.hasNext) hasNext = true
        }
        mapOf("items" to items.distinctBy { it["url"] }, "hasNext" to hasNext)
    }

    "providers.search" -> {
        val api = provider(params)
        val query = params.get("query")?.asText().orEmpty()
        val results = withTimeoutOrNull(api.searchTimeoutMs ?: DEFAULT_TIMEOUT) {
            api.search(query, 1)?.items.orEmpty()
        }.orEmpty()
        mapOf("items" to results.map { itemJson(it, api.name) })
    }

    "providers.quickSearch" -> {
        val query = params.get("query")?.asText().orEmpty()
        val items = mutableListOf<Map<String, Any?>>()
        Engine.search(query).toList().forEach { batch ->
            batch.results.forEach { items += itemJson(it, batch.api.name) }
        }
        mapOf("items" to items)
    }

    "providers.load" -> {
        val api = provider(params)
        val url = params.get("url")?.asText().orEmpty()
        val response = Engine.load(api, url) ?: error("${api.name} returned nothing for $url")
        mapOf("item" to loadJson(response, api.name))
    }

    "providers.links" -> {
        val api = provider(params)
        val url = params.get("url")?.asText().orEmpty()
        val found = Engine.allLinks(api, url)
        val subtitles = found.subtitles.map { subtitleJson(it) }
        mapOf("links" to found.links.map { linkJson(it, subtitles) })
    }

    else -> error("unknown method '$method'")
}

private fun provider(params: JsonNode): MainAPI {
    val name = params.get("provider")?.asText().orEmpty()
    return Engine.provider(name) ?: error("provider not found: '$name'")
}

// --- wire shapes --------------------------------------------------------------

private fun kind(type: TvType?): String = when (type) {
    TvType.Movie, TvType.AnimeMovie, TvType.Cartoon, TvType.Documentary, TvType.Torrent, TvType.NSFW -> "movie"
    TvType.TvSeries, TvType.AsianDrama -> "series"
    TvType.Anime -> "anime"
    TvType.Live -> "live"
    else -> "other"
}

private fun itemJson(item: SearchResponse, providerName: String): Map<String, Any?> = mapOf(
    "id" to "host:$providerName:${item.url}",
    "name" to item.name,
    "url" to item.url,
    "poster" to item.posterUrl,
    "background" to null,
    "description" to item.plot,
    "year" to item.year,
    "type" to kind(item.type),
    "provider" to providerName,
    "episodes" to emptyList<Map<String, Any?>>(),
)

private fun episodeJson(playable: com.lagradost.cloudstream3.linux.Playable): Map<String, Any?> = mapOf(
    "name" to (playable.name ?: playable.label),
    "url" to playable.data,
    "season" to playable.season,
    "episode" to playable.episode,
    "thumbnail" to playable.posterUrl,
    "description" to playable.description,
)

private fun loadJson(response: LoadResponse, providerName: String): Map<String, Any?> {
    val playables = Engine.playables(response)
    val episodes: List<Map<String, Any?>> = when (response) {
        is TvSeriesLoadResponse -> response.episodes.map { episodeJson(it.toPlayable()) }
        is AnimeLoadResponse -> response.episodes.values.flatten().map { episodeJson(it.toPlayable()) }
        is MovieLoadResponse, is LiveStreamLoadResponse, is TorrentLoadResponse -> emptyList()
        else -> playables.map { episodeJson(it) }
    }
    return mapOf(
        "id" to "host:$providerName:${response.url}",
        "name" to response.name,
        "url" to response.dataUrl,
        "poster" to response.posterUrl,
        "background" to (response as? LiveStreamLoadResponse)?.posterUrl,
        "description" to response.plot,
        "year" to response.year,
        "type" to kind(response.type),
        "provider" to providerName,
        "episodes" to episodes,
    )
}

private fun Episode.toPlayable(): com.lagradost.cloudstream3.linux.Playable {
    val code = listOfNotNull(season?.let { "S$it" }, episode?.let { "E$it" }).joinToString("")
    val label = listOf(code, name ?: "").filter { it.isNotBlank() }.joinToString(" · ").ifBlank { "Episode" }
    return com.lagradost.cloudstream3.linux.Playable(label, data, season, episode, name, posterUrl, description, null)
}

private fun linkJson(link: ExtractorLink, subtitles: List<Map<String, Any?>>): Map<String, Any?> = mapOf(
    "url" to link.url,
    "name" to link.name,
    "quality" to link.quality.toString(),
    "source" to link.source,
    "referer" to link.referer,
    "headers" to link.headers,
    "type" to when (link.type) {
        ExtractorLinkType.M3U8 -> "hls"
        ExtractorLinkType.DASH -> "dash"
        ExtractorLinkType.VIDEO -> "mp4"
        ExtractorLinkType.TORRENT, ExtractorLinkType.MAGNET -> "torrent"
        else -> "other"
    },
    "subtitles" to subtitles,
)

private fun subtitleJson(subtitle: SubtitleFile): Map<String, Any?> = mapOf(
    "lang" to subtitle.lang,
    "url" to subtitle.url,
    "name" to null,
)
