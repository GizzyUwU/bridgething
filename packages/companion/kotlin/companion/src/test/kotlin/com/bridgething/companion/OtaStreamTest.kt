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
import com.bridgething.schema.OtaPhase
import com.bridgething.schema.OtaProgress
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

class OtaStreamTest {
    private val fragmentBytes = TransferPacer.FRAGMENT_BYTES

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
        companion.attach(FakeGlue())
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
    fun `range stream windows against acks`() = runBlocking {
        val (companion, driver) = boot(this)
        val size = 256 * 1024
        val window = TransferPacer.MAX_WINDOW_BYTES.toInt()
        val zck = writeTempArtifact(size)
        companion.ota.setLocalZcks(mapOf("system.img.zck" to zck))

        val reply = driver.request(
            BridgeToGatewayMsgData.System(
                BridgeToGatewaySystemMsg.OtaAssetRange(
                    OtaAssetRange(updateId = "u1", asset = "system.img.zck", ranges = listOf(RangeSpec(start = 0u, length = size.toUInt()))),
                ),
            ),
        )
        val replyData = (reply.data as GatewayToBridgeMsgData.System).data
        assertTrue(replyData is GatewayToBridgeSystemMsg.OtaAssetRangeReply, "expected a range reply, got $replyData")
        val body = (replyData as GatewayToBridgeSystemMsg.OtaAssetRangeReply).data.body
        assertTrue(body is TransferBody.Stream, "a range larger than the inline cap must stream")
        val transferId = (body as TransferBody.Stream).data.id

        suspend fun nextFragment() =
            ((driver.waitOutbound { msg ->
                val f = (msg.data as? GatewayToBridgeMsgData.Transfer)?.data as? GatewayToBridgeTransferMsg.Fragment
                f?.data?.transferId == transferId
            }.data as GatewayToBridgeMsgData.Transfer).data as GatewayToBridgeTransferMsg.Fragment).data

        suspend fun ack(received: Int) = driver.send(
            BridgeToGatewayMsgData.Transfer(BridgeToGatewayTransferMsg.Ack(TransferAck(transferId = transferId, received = received.toUInt()))),
            meta = MsgMeta.Event,
        )

        var assembled = 0
        val first = nextFragment()
        assertEquals(0, first.offset.toInt())
        assembled += first.bytes.size

        val expectedInFlight = (TransferPacer.MIN_WINDOW_BYTES / fragmentBytes).toInt()
        assertTrue(expectedInFlight >= 4, "initial window must span several fragments")
        repeat(expectedInFlight - 1) {
            val f = nextFragment()
            assertEquals(assembled, f.offset.toInt(), "range fragments must arrive contiguous in offset order")
            assembled += f.bytes.size
        }
        val past = runCatching { withTimeout(600) { nextFragment() } }
        assertTrue(past.isFailure, "range sender ran past its window without an ack")

        var acked = assembled
        ack(acked)
        while (assembled < size) {
            val f = nextFragment()
            assertEquals(assembled, f.offset.toInt(), "range fragments must arrive contiguous in offset order")
            assertTrue(f.offset.toInt() < acked + window, "range sender must stay within the max window of acked")
            assembled += f.bytes.size
            acked = f.offset.toInt() + f.bytes.size
            ack(acked)
        }
        assertEquals(size, assembled)
        zck.delete()
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

        val begin = driver.waitOutbound { msg ->
            ((msg.data as? GatewayToBridgeMsgData.System)?.data as? GatewayToBridgeSystemMsg.OtaBegin) != null
        }
        val transferId = ((begin.data as GatewayToBridgeMsgData.System).data as GatewayToBridgeSystemMsg.OtaBegin).data.transfer.id
        driver.send(
            BridgeToGatewayMsgData.System(BridgeToGatewaySystemMsg.OtaBeginAck(OtaBeginAck(resumeFromOffset = resumeOffset.toUInt()))),
            meta = MsgMeta.Response(ResponseMeta(requestId = begin.id)),
        )

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

    @Test
    fun `apply-version image change runs image only`() = runBlocking {
        val (companion, driver) = boot(this)
        val channel = "stable"
        val dir = otaCacheDir()
        val swu = seedArtifact(dir, "image-$channel-2026.05.0.swu", 2048)
        val zck = seedArtifact(dir, "image-$channel-2026.05.0.zck", 256)
        val bootZck = seedArtifact(dir, "image-$channel-2026.05.0-boot.zck", 256)
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
