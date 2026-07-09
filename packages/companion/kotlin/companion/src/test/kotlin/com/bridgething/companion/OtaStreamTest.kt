package com.bridgething.companion

import com.bridgething.schema.BridgeThingMeta
import com.bridgething.schema.BridgeToGatewayMsgData
import com.bridgething.schema.BridgeToGatewaySystemMsg
import com.bridgething.schema.BridgeToGatewayTransferMsg
import com.bridgething.schema.GatewayToBridgeMsgData
import com.bridgething.schema.GatewayToBridgeSystemMsg
import com.bridgething.schema.GatewayToBridgeTransferMsg
import com.bridgething.schema.MsgMeta
import com.bridgething.schema.OtaAssetRange
import com.bridgething.schema.OtaBegin
import com.bridgething.schema.OtaBeginAck
import com.bridgething.schema.OtaKind
import com.bridgething.schema.RangeSpec
import com.bridgething.schema.ResponseMeta
import com.bridgething.schema.TransferAck
import com.bridgething.schema.TransferBody
import io.mockk.mockk
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import java.io.File
import java.util.UUID

/** regression coverage for the OTA resume path: a non-zero resume offset must still stream. */
class OtaStreamTest {
    private val fragmentBytes = 4 * 1024

    private suspend fun boot(scope: CoroutineScope): Pair<BridgethingCompanion, WireDriver> {
        val adapter = FakeAdapter()
        val companion = BridgethingCompanion(
            context = mockk(relaxed = true),
            adapter = adapter,
            lyricsResolver = FakeLyricsResolver(),
            host = HostInfo(appName = "test", appVersion = "0.0.1", osName = "test"),
            geo = NoOpGeoSource,
            volume = NoOpVolumeSource,
            audio = NoOpAudioBackend,
        )
        companion.setActive(FakeGlue())
        companion.start()
        val driver = WireDriver(adapter)
        driver.start(scope)
        driver.connect()
        return companion to driver
    }

    private fun writeTempArtifact(bytes: Int): File {
        val f = File.createTempFile("ota-", ".bin")
        f.writeBytes(ByteArray(bytes) { (it % 251).toByte() })
        return f
    }

    private fun writeFilledZck(bytes: Int, fill: Byte): File {
        val f = File.createTempFile("zck-", ".bin")
        f.writeBytes(ByteArray(bytes) { fill })
        return f
    }

    @Test
    fun `range request routes by asset`() = runBlocking {
        val (companion, driver) = boot(this)
        val systemZck = writeFilledZck(256, 0xAA.toByte())
        val bootZck = writeFilledZck(256, 0xBB.toByte())
        companion.ota.setLocalZcks(mapOf("system.img.zck" to systemZck, "boot.vfat.zck" to bootZck))

        suspend fun requestRange(asset: String): GatewayToBridgeSystemMsg {
            val reply = driver.request(
                BridgeToGatewayMsgData.System(
                    BridgeToGatewaySystemMsg.OtaAssetRange(
                        OtaAssetRange(updateId = "u1", asset = asset, ranges = listOf(RangeSpec(start = 0u, length = 256u))),
                    ),
                ),
            )
            return (reply.data as GatewayToBridgeMsgData.System).data
        }

        val bootReply = requestRange("boot.vfat.zck")
        assertTrue(bootReply is GatewayToBridgeSystemMsg.OtaAssetRangeReply, "boot asset must be served, got $bootReply")
        val bootBody = ((bootReply as GatewayToBridgeSystemMsg.OtaAssetRangeReply).data.body as TransferBody.Inline).data
        assertTrue(bootBody.contentEquals(ByteArray(256) { 0xBB.toByte() }), "boot asset must be served from the boot zck")

        val systemReply = requestRange("system.img.zck")
        assertTrue(systemReply is GatewayToBridgeSystemMsg.OtaAssetRangeReply, "system asset must be served, got $systemReply")
        val systemBody = ((systemReply as GatewayToBridgeSystemMsg.OtaAssetRangeReply).data.body as TransferBody.Inline).data
        assertTrue(systemBody.contentEquals(ByteArray(256) { 0xAA.toByte() }), "system asset must be served from the system zck")

        val unknownReply = requestRange("does-not-exist.zck")
        assertTrue(unknownReply is GatewayToBridgeSystemMsg.OtaAssetRangeRejected, "unknown asset must be rejected, got $unknownReply")
        assertTrue(
            (unknownReply as GatewayToBridgeSystemMsg.OtaAssetRangeRejected).data.reason.contains("does-not-exist.zck"),
            "rejection must name the missing asset",
        )

        systemZck.delete()
        bootZck.delete()
        companion.stop()
    }

    @Test
    fun `resume from non-zero offset streams the remainder`() = runBlocking {
        val payloadSize = 160 * 1024
        val resumeOffset = 64 * 1024
        val artifact = writeTempArtifact(payloadSize)
        val (companion, driver) = boot(this)

        val pushJob = async {
            companion.ota.pushDaemon(gateway = companion.gateway, deviceId = driver.deviceId, binaryPath = artifact).toList()
        }

        // the daemon already holds resumeOffset bytes and reports it as the resume point.
        val begin = driver.waitOutbound { msg ->
            ((msg.data as? GatewayToBridgeMsgData.System)?.data as? GatewayToBridgeSystemMsg.OtaBegin) != null
        }
        val transferId = ((begin.data as GatewayToBridgeMsgData.System).data as GatewayToBridgeSystemMsg.OtaBegin).data.transfer.id
        driver.send(
            BridgeToGatewayMsgData.System(BridgeToGatewaySystemMsg.OtaBeginAck(OtaBeginAck(resumeFromOffset = resumeOffset.toUInt()))),
            meta = MsgMeta.Response(ResponseMeta(requestId = begin.id)),
        )

        // regression: the first resume fragment must actually be sent. daemon acks are absolute file
        // offsets, so before the ack-window baseline was seeded to resumeOffset the sender gated the
        // absolute offset against acked(0)+window, deadlocked, and emitted zero fragments.
        var expected = resumeOffset
        var first = true
        while (expected < payloadSize) {
            val frame = driver.waitOutbound { msg ->
                val f = (msg.data as? GatewayToBridgeMsgData.Transfer)?.data as? GatewayToBridgeTransferMsg.Fragment
                f?.data?.transferId == transferId
            }
            val f = ((frame.data as GatewayToBridgeMsgData.Transfer).data as GatewayToBridgeTransferMsg.Fragment).data
            if (first) {
                assertEquals(resumeOffset, f.offset.toInt(), "first fragment must resume at the daemon's offset, not 0")
                first = false
            }
            assertEquals(expected, f.offset.toInt(), "resume fragments must arrive contiguous in offset order")
            assertTrue(f.bytes.size <= fragmentBytes)
            expected = f.offset.toInt() + f.bytes.size
            driver.send(
                BridgeToGatewayMsgData.Transfer(BridgeToGatewayTransferMsg.Ack(TransferAck(transferId = transferId, received = expected.toUInt()))),
                meta = MsgMeta.Event,
            )
        }
        assertEquals(payloadSize, expected, "the whole remainder past resumeOffset must stream")

        // the resume regression is fully asserted above; the daemon-side stage/activate/reboot terminal
        // is out of scope here, so tear the in-flight push down cleanly rather than driving it.
        pushJob.cancelAndJoin()
        artifact.delete()
        companion.stop()
    }

    private fun otaCacheDir(): File =
        File(System.getProperty("java.io.tmpdir") ?: "/tmp", "bridgething-ota").also { it.mkdirs() }

    private fun seedArtifact(dir: File, name: String, bytes: Int): File =
        File(dir, name).apply { writeBytes(ByteArray(bytes) { (it % 251).toByte() }) }

    private fun makeMeta(appVersion: String, imageVersion: String, channel: String, variant: String = "prod") =
        BridgeThingMeta(
            bridgethingVersion = appVersion, libbridgethingVersion = appVersion, appName = "bridgething",
            nickname = null, appVersion = appVersion, osName = "linux", osVersion = "1", osDescription = "",
            btMac = "", serialNumber = "", fccId = "", icId = "", modelName = "Car Thing", channel = channel,
            imageVariant = variant, imageVersion = imageVersion, imageBuildId = "", imageBuildDate = "",
            imageDistro = "", imageMachine = "", discord = "", credits = "",
        )

    private suspend fun injectMeta(companion: BridgethingCompanion, driver: WireDriver, meta: BridgeThingMeta) {
        driver.send(BridgeToGatewayMsgData.Version(meta), meta = MsgMeta.Event)
        repeat(100) {
            if (companion.ota.meta(driver.deviceId) != null) return
            delay(20)
        }
        throw AssertionError("device meta was never recorded by the ota service")
    }

    private suspend fun nextOtaBegin(driver: WireDriver): Pair<UUID, OtaBegin> {
        val frame = driver.waitOutbound { msg ->
            ((msg.data as? GatewayToBridgeMsgData.System)?.data as? GatewayToBridgeSystemMsg.OtaBegin) != null
        }
        val begin = ((frame.data as GatewayToBridgeMsgData.System).data as GatewayToBridgeSystemMsg.OtaBegin).data
        return frame.id to begin
    }

    /**
     * When the target composite bumps BOTH the image and the daemon, applyVersion must run the image OTA
     * only. The image slot carries its own matching daemon (adopted on boot), so the standalone daemon
     * bandaid must never be pushed: the first (and only) OtaBegin is Image, never Daemon.
     */
    @Test
    fun `apply-version image change runs image only`() = runBlocking {
        val (companion, driver) = boot(this)
        val channel = "stable"
        val dir = otaCacheDir()
        val swu = seedArtifact(dir, "image-$channel-2026.05.0.swu", 2048)
        val zck = seedArtifact(dir, "image-$channel-2026.05.0.zck", 256)
        val bootZck = seedArtifact(dir, "image-$channel-2026.05.0-boot.zck", 256)
        // seed the daemon artifact too so it is a live cache hit: proving the code path, not a missing
        // download, is what keeps the daemon bandaid from being pushed.
        val daemon = seedArtifact(dir, "daemon-$channel-0.8.4", 512)

        injectMeta(companion, driver, makeMeta(appVersion = "0.8.3", imageVersion = "2026.04.0", channel = channel))

        val applyJob = async {
            companion.ota.applyVersion(
                deviceId = driver.deviceId, channel = channel,
                version = "0.8.4+image.2026.05.0", rootUrl = "https://ota.invalid",
            )
        }

        val (beginId, begin) = nextOtaBegin(driver)
        assertEquals(OtaKind.Image, begin.kind, "an image change must run the image OTA, not the daemon bandaid")

        // ack so the stream side settles (resume==totalSize means no fragments), then confirm no further
        // OtaBegin (least of all a daemon bandaid) leaves the wire while the image is changing.
        driver.send(
            BridgeToGatewayMsgData.System(BridgeToGatewaySystemMsg.OtaBeginAck(OtaBeginAck(resumeFromOffset = begin.transfer.totalSize))),
            meta = MsgMeta.Response(ResponseMeta(requestId = beginId)),
        )
        val stray = runCatching { withTimeout(500) { nextOtaBegin(driver) } }
        assertTrue(stray.isFailure, "no standalone daemon bandaid push while the image is changing")

        applyJob.cancelAndJoin()
        listOf(swu, zck, bootZck, daemon).forEach { it.delete() }
        companion.stop()
    }

    /**
     * When only the daemon differs (image already matches the target), applyVersion still runs the daemon
     * bandaid: the first OtaBegin is Daemon and no image OTA is started.
     */
    @Test
    fun `apply-version daemon only runs bandaid`() = runBlocking {
        val (companion, driver) = boot(this)
        val channel = "stable"
        val dir = otaCacheDir()
        val daemon = seedArtifact(dir, "daemon-$channel-0.8.4", 512)

        injectMeta(companion, driver, makeMeta(appVersion = "0.8.3", imageVersion = "2026.05.0", channel = channel))

        val applyJob = async {
            companion.ota.applyVersion(
                deviceId = driver.deviceId, channel = channel,
                version = "0.8.4+image.2026.05.0", rootUrl = "https://ota.invalid",
            )
        }

        val (beginId, begin) = nextOtaBegin(driver)
        assertEquals(OtaKind.Daemon, begin.kind, "a daemon-only delta must run the daemon bandaid")

        // ack the staged piece, then confirm no image OtaBegin is ever started for a daemon-only delta.
        driver.send(
            BridgeToGatewayMsgData.System(BridgeToGatewaySystemMsg.OtaBeginAck(OtaBeginAck(resumeFromOffset = begin.transfer.totalSize))),
            meta = MsgMeta.Response(ResponseMeta(requestId = beginId)),
        )
        val stray = runCatching { withTimeout(500) { nextOtaBegin(driver) } }
        assertTrue(stray.isFailure, "a daemon-only delta must not start an image OTA")

        applyJob.cancelAndJoin()
        daemon.delete()
        companion.stop()
    }
}
