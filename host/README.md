# Extension host (JVM side)

`kotlin/ExtensionHost.kt` is the only Kotlin this project needs: it exposes the
existing CloudStream engine over loopback HTTP so the Rust app can use extensions
without a JVM in its own process.

## Adding it to the Kotlin app

1. Copy the file to
   `app/src/main/kotlin/com/lagradost/cloudstream3/linux/host/ExtensionHost.kt`.
2. Add a launcher to `app/build.gradle.kts`:

   ```kotlin
   tasks.register<JavaExec>("extensionHost") {
       group = "application"
       mainClass.set("com.lagradost.cloudstream3.linux.host.ExtensionHostKt")
       classpath = sourceSets["main"].runtimeClasspath
       // Same JVM flags as the GUI: extension bytecode comes from dex2jar.
       jvmArgs(
           "-XX:+UnlockDiagnosticVMOptions",
           "-XX:-BytecodeVerificationRemote",
           "--enable-native-access=ALL-UNNAMED",
       )
   }
   ```

   and a second launcher for release builds:

   ```kotlin
   addAll(listOf("--add-launcher", "streamgoat-host=" + rootProject.file("packaging/host-launcher.properties").path))
   ```

   with `packaging/host-launcher.properties`:

   ```
   main-class=com.lagradost.cloudstream3.linux.host.ExtensionHostKt
   ```

3. Run it:

   ```sh
   ./gradlew :app:extensionHost                 # prints {"addr":"127.0.0.1:PORT","token":"…"}
   java -jar "Stream Goat/lib/app/streamgoat-host.jar"   # from the packaged app
   ```

## How the Rust app uses it

`settings.host_command` holds the command line. The Rust side either:

* connects to `settings.host_addr` + `settings.host_token` (host already running), or
* runs `HostProcess::spawn`, reads the handshake line from stdout and connects.

The protocol is in [`../docs/host-protocol.md`](../docs/host-protocol.md); the Rust
client is [`../crates/core/src/host.rs`](../crates/core/src/host.rs) and the CLI can
poke it directly:

```sh
streamgoat host --addr 127.0.0.1:41235 --token "$TOKEN" providers
streamgoat host --addr 127.0.0.1:41235 --token "$TOKEN" search Cine dune
```

## Notes

* `Engine.allLinks` keeps the 20-minute link cache, so the Rust player and the
  Kotlin UI (if both run) share results.
* Only `HttpServer` from the JDK is used: no extra dependency, nothing to build.
* The token is random per start unless `--token` is given; the host binds to
  `127.0.0.1` only.
