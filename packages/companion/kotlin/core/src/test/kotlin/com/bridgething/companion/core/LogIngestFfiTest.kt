package com.bridgething.companion.core

import java.nio.file.Files
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.runBlocking
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
import uniffi.bridgething_companion.LogLevel
import uniffi.bridgething_companion.LogOrigin
import uniffi.bridgething_companion.LogSink
import uniffi.bridgething_companion.SecretStore
import uniffi.bridgething_companion.SessionEvent
import uniffi.bridgething_companion.SessionEventSink
import uniffi.bridgething_companion.WsConnect
import uniffi.bridgething_companion.WsFrame
import uniffi.bridgething_companion.WsInbox
import uniffi.bridgething_companion.WsTransport

private const val AWAIT_MS = 10_000L

private class NoSecrets : SecretStore {
  override fun get(key: String): String? = null

  override fun set(key: String, value: String) {}

  override fun remove(key: String) {}

  override fun getBlob(key: String): ByteArray? = null
}

private class NoNetworkHttp : HttpTransport {
  override fun execute(request: HttpRequest, sink: HttpSink) {
    sink.use { it.fail("the log test has no network") }
  }

  override fun download(request: HttpRequest, sink: HttpDownloadSink) {
    sink.use { it.onFailed("the log test has no network") }
  }
}

private class NoNetworkWs : WsTransport {
  override fun connect(connect: WsConnect, inbox: WsInbox) {
    inbox.use { it.onClosed(connect.id, null, "the log test has no network") }
  }

  override fun send(id: String, frame: WsFrame) {}

  override fun disconnect(id: String, code: UShort?, reason: String?) {}
}

private class FixedClockHost : HostEnvironment {
  override fun clock(): HostClock =
    HostClock(tzIana = "UTC", locale = "en-US", unixSeconds = 1_700_000_000uL, utcOffsetMinutes = 0, dstOffsetMinutes = 0)
}

private class DiscardLog : LogSink {
  override fun onLine(level: LogLevel, target: String, message: String) {}
}

private class LogEvents : SessionEventSink {
  val events = LinkedBlockingQueue<SessionEvent>()

  override fun onEvent(event: SessionEvent) {
    events.add(event)
  }

  fun awaitLog(matches: (SessionEvent.Log) -> Boolean): SessionEvent.Log {
    val deadline = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(AWAIT_MS)
    while (System.nanoTime() < deadline) {
      val event = events.poll(200, TimeUnit.MILLISECONDS) ?: continue
      if (event is SessionEvent.Log && matches(event)) return event
    }
    throw AssertionError("no matching log event within ${AWAIT_MS}ms")
  }
}

private fun session(events: LogEvents): CompanionSession {
  val scratch = Files.createTempDirectory("companion-logs").toFile().absolutePath
  return CompanionSession.create(
    config = CompanionConfig(
      host = HostInfo(
        appName = "log-test",
        appVersion = "0.0.0",
        osName = "linux",
        osVersion = "test",
        hostIdentifier = "jvm-logs",
      ),
      capabilities = CapabilityFlags(
        geo = false,
        notifications = false,
        netFetch = false,
        netWs = false,
        audioTts = false,
        voiceModel = false,
      ),
      stateDir = scratch,
      cacheDir = scratch,
    ),
    backends = CompanionBackends(
      link = null,
      host = FixedClockHost(),
      http = NoNetworkHttp(),
      ws = NoNetworkWs(),
      secrets = NoSecrets(),
      log = DiscardLog(),
    ),
    events = events,
  )
}

class LogIngestFfiTest {
  @Test
  fun aPushedLogcatLineLandsInTheRingAndTheLiveStream() =
    runBlocking {
      val events = LogEvents()
      val session = session(events)

      session.logInbox().push(LogLevel.WARN, "BridgethingBT", "rfcomm connect failed")

      val live = events.awaitLog { it.message == "rfcomm connect failed" }
      assertEquals(LogOrigin.HOST, live.origin, "a platform line says it came from the host")
      assertEquals("BridgethingBT", live.target)

      val tail = session.deviceLogSnapshot(10u)
      assertEquals(1, tail.size, "the ring retained the pushed line for backfill")
      assertEquals("rfcomm connect failed", tail[0].message)
      assertEquals("BridgethingBT", tail[0].target)
      assertEquals(LogOrigin.HOST, tail[0].origin)
      assertEquals(LogLevel.WARN, tail[0].level)
      assertTrue(tail[0].seq > 0uL)
      assertTrue(tail[0].tsUnixMs > 0uL)
    }

  @Test
  fun theSnapshotIsABoundedNewestFirstBackfillServedOldestFirst() =
    runBlocking {
      val session = session(LogEvents())
      val inbox = session.logInbox()
      for (n in 1..5) {
        inbox.push(LogLevel.INFO, "logcat", "line $n")
      }

      val tail = session.deviceLogSnapshot(2u)
      assertEquals(listOf("line 4", "line 5"), tail.map { it.message }, "the limit keeps the newest, oldest first")
    }
}
