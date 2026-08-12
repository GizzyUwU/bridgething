package com.bridgething.companion.core

import java.nio.file.Files
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import uniffi.bridgething_companion.CapabilityFlags
import uniffi.bridgething_companion.CompanionBackends
import uniffi.bridgething_companion.CompanionConfig
import uniffi.bridgething_companion.CompanionSession
import uniffi.bridgething_companion.HostClock
import uniffi.bridgething_companion.HostEnvironment
import uniffi.bridgething_companion.HostInfo
import uniffi.bridgething_companion.HttpDownloadSink
import uniffi.bridgething_companion.HttpRequest
import uniffi.bridgething_companion.HttpSink
import uniffi.bridgething_companion.HttpTransport
import uniffi.bridgething_companion.LinkDevice
import uniffi.bridgething_companion.LinkInbox
import uniffi.bridgething_companion.LinkTransport
import uniffi.bridgething_companion.LogLevel
import uniffi.bridgething_companion.LogSink
import uniffi.bridgething_companion.PeerLinkStatus
import uniffi.bridgething_companion.SecretStore
import uniffi.bridgething_companion.SessionEvent
import uniffi.bridgething_companion.SessionEventSink
import uniffi.bridgething_companion.WsConnect
import uniffi.bridgething_companion.WsFrame
import uniffi.bridgething_companion.WsInbox
import uniffi.bridgething_companion.WsTransport

private const val MAX_BATCH_BYTES = 32768u
private const val DEVICE_ID = "jvm-smoke-device"
private const val DEVICE_NAME = "smoke device"
private const val AWAIT_MS = 10_000L

private class RecordingSecretStore : SecretStore {
  val calls = CopyOnWriteArrayList<String>()

  override fun get(key: String): String? {
    calls.add("get:$key")
    return null
  }

  override fun set(
    key: String,
    value: String,
  ) {
    calls.add("set:$key")
  }

  override fun remove(key: String) {
    calls.add("remove:$key")
  }

  override fun getBlob(key: String): ByteArray? {
    calls.add("getBlob:$key")
    return null
  }
}

private class RecordingEventSink : SessionEventSink {
  val events = LinkedBlockingQueue<SessionEvent>()

  override fun onEvent(event: SessionEvent) {
    events.add(event)
  }

  inline fun <reified T : SessionEvent> await(): T {
    val deadline = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(AWAIT_MS)
    val seen = mutableListOf<SessionEvent>()
    while (System.nanoTime() < deadline) {
      val event = events.poll(200, TimeUnit.MILLISECONDS) ?: continue
      if (event is T) return event
      seen.add(event)
    }
    throw AssertionError("no ${T::class.simpleName} within ${AWAIT_MS}ms, saw $seen")
  }
}

private class LoopbackLinkTransport : LinkTransport {
  val batches = CopyOnWriteArrayList<ByteArray>()

  @Volatile
  private var inbox: LinkInbox? = null

  override fun maxBatchBytes(): UInt = MAX_BATCH_BYTES

  override fun start(inbox: LinkInbox) {
    this.inbox = inbox
    inbox.onConnected(LinkDevice(id = DEVICE_ID, name = DEVICE_NAME))
  }

  override fun stop() {
    inbox = null
  }

  override fun send(
    deviceId: String,
    batch: ByteArray,
  ) {
    batches.add(batch)
    inbox?.onWriteComplete(deviceId)
  }

  override fun disconnect(deviceId: String) {}

  override fun reconnect(deviceId: String) {}

  fun awaitBatch(): ByteArray {
    val deadline = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(AWAIT_MS)
    while (System.nanoTime() < deadline) {
      batches.firstOrNull()?.let { return it }
      Thread.sleep(20)
    }
    throw AssertionError("the connect sequence never reached the transport")
  }
}

private class FixedHost : HostEnvironment {
  override fun clock(): HostClock =
    HostClock(
      tzIana = "UTC",
      locale = "en-US",
      unixSeconds = 1_700_000_000uL,
      utcOffsetMinutes = 0,
      dstOffsetMinutes = 0,
    )
}

private class OfflineHttpTransport : HttpTransport {
  override fun execute(
    request: HttpRequest,
    sink: HttpSink,
  ) {
    sink.fail("the smoke test has no network")
  }

  override fun download(
    request: HttpRequest,
    sink: HttpDownloadSink,
  ) {
    sink.onFailed("the smoke test has no network")
  }
}

private class OfflineWsTransport : WsTransport {
  override fun connect(
    connect: WsConnect,
    inbox: WsInbox,
  ) {
    inbox.onClosed(connect.id, null, "the smoke test has no network")
  }

  override fun send(
    id: String,
    frame: WsFrame,
  ) {}

  override fun disconnect(
    id: String,
    code: UShort?,
    reason: String?,
  ) {}
}

private class CollectingLogSink : LogSink {
  val lines = CopyOnWriteArrayList<String>()

  override fun onLine(
    level: LogLevel,
    target: String,
    message: String,
  ) {
    lines.add("$level/$target: $message")
  }
}

private fun scratch(): String = Files.createTempDirectory("companion-smoke").toFile().absolutePath

private fun config(stateDir: String) =
  CompanionConfig(
    host =
      HostInfo(
        appName = "smoke",
        appVersion = "0.0.0",
        osName = "linux",
        osVersion = "test",
        hostIdentifier = "jvm-smoke",
      ),
    capabilities =
      CapabilityFlags(
        geo = false,
        notifications = false,
        netFetch = false,
        netWs = false,
        audioTts = false,
        voiceModel = false,
      ),
    stateDir = stateDir,
    cacheDir = stateDir,
  )

private fun backends(link: LinkTransport) =
  CompanionBackends(
    link = link,
    host = FixedHost(),
    http = OfflineHttpTransport(),
    ws = OfflineWsTransport(),
    secrets = RecordingSecretStore(),
    log = CollectingLogSink(),
  )

class CompanionFfiSmokeTest {
  @Test
  fun startBringsUpTheLinkAndTellsTheHostAboutThePeer() =
    runBlocking {
      val link = LoopbackLinkTransport()
      val sink = RecordingEventSink()
      val session = CompanionSession.create(config(scratch()), backends(link), sink)

      withTimeout(AWAIT_MS) { session.start() }

      val connected = sink.await<SessionEvent.PeerConnected>()
      assertEquals(DEVICE_ID, connected.peer.id)
      assertEquals(DEVICE_NAME, connected.peer.name)
      assertEquals(PeerLinkStatus.CONNECTED, connected.peer.status)
      assertTrue(link.awaitBatch().isNotEmpty()) { "the connect sequence put an empty batch on the wire" }

      withTimeout(AWAIT_MS) { session.stop() }
    }

  @Test
  fun theSnapshotReportsTheHostAndTheLivePeer() =
    runBlocking {
      val link = LoopbackLinkTransport()
      val sink = RecordingEventSink()
      val session = CompanionSession.create(config(scratch()), backends(link), sink)

      withTimeout(AWAIT_MS) { session.start() }
      sink.await<SessionEvent.PeerConnected>()

      val snapshot = withTimeout(AWAIT_MS) { session.snapshot() }
      assertEquals("smoke", snapshot.hostInfo.appName)
      assertEquals(listOf(DEVICE_ID), snapshot.peers.map { it.id })
      assertEquals(PeerLinkStatus.CONNECTED, snapshot.peers[0].status)

      withTimeout(AWAIT_MS) { session.stop() }
    }

  @Test
  fun logInboxFansOutToTheEventSink() =
    runBlocking {
      val sink = RecordingEventSink()
      val session = CompanionSession.create(config(scratch()), backends(LoopbackLinkTransport()), sink)

      session.logInbox().push(LogLevel.WARN, "platform", "hello from kotlin")

      val pushed = sink.await<SessionEvent.Log>()
      assertEquals(LogLevel.WARN, pushed.level)
      assertEquals("platform", pushed.target)
      assertEquals("hello from kotlin", pushed.message)
    }
}
