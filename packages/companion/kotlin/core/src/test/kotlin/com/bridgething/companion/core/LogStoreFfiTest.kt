package com.bridgething.companion.core

import java.io.File
import java.nio.file.Files
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import uniffi.bridgething_companion.LogStore
import uniffi.bridgething_companion.LogStoreLevel

class LogStoreFfiTest {
  private fun installed(): Pair<File, LogStore> {
    val root = Files.createTempDirectory("logstore-ffi").toFile()
    return root to LogStore.install(root.path)
  }

  @Test
  fun aRecordPersistsThroughTheFfiAndComesBackInAnExport() {
    val (root, store) = installed()

    store.record(LogStoreLevel.WARN, "daemon", "[player] stalled")
    store.flush()

    val bundle = File(root, "bundle.txt")
    assertEquals(bundle.path, store.exportTo(bundle.path, null))
    val text = bundle.readText()
    assertTrue(text.startsWith("bridgething log export\n"), text.take(80))
    assertTrue(text.contains(" W daemon: [player] stalled"), text)
  }

  @Test
  fun aRawLogcatLineAtErrorSeverityPinsItsLaunch() {
    val (_, store) = installed()

    store.write("07-30 12:00:00.000  1  1 E BridgethingBT: rfcomm connect failed")
    store.flush()

    val live = store.archives().first { it.current }
    assertTrue(live.pinned, "an error line pins the launch holding it")
    assertTrue(live.bytes > 0uL)
  }

  @Test
  fun clearEmptiesTheLiveLaunchAndLeavesItRecordable() {
    val (_, store) = installed()

    store.record(LogStoreLevel.INFO, "daemon", "before")
    store.flush()
    store.clear()

    assertEquals(0uL, store.retainedBytes())
    assertFalse(store.archives().first { it.current }.pinned)

    store.record(LogStoreLevel.INFO, "daemon", "after")
    store.flush()
    assertTrue(store.retainedBytes() > 0uL)
  }
}
