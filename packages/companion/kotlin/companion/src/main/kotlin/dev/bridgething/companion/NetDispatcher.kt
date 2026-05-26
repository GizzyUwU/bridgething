package dev.bridgething.companion

import dev.bridgething.gateway.BridgethingGateway
import dev.bridgething.gateway.device
import dev.bridgething.gateway.net
import dev.bridgething.schema.HttpHeader
import dev.bridgething.schema.HttpMethod
import dev.bridgething.schema.NetError
import dev.bridgething.schema.NetErrorRequestFailedInner
import dev.bridgething.schema.NetFetchErrorReply
import dev.bridgething.schema.NetFetchReply
import dev.bridgething.schema.NetFetchRequest
import dev.bridgething.schema.NetFetchResponse
import dev.bridgething.schema.NetWsClosed
import dev.bridgething.schema.NetWsErrorReply
import dev.bridgething.schema.NetWsMessage
import dev.bridgething.schema.NetWsOpenReply
import dev.bridgething.schema.Priority
import dev.bridgething.schema.StreamBegin
import dev.bridgething.schema.StreamChunk
import dev.bridgething.schema.StreamEnd
import dev.bridgething.schema.StreamError
import dev.bridgething.schema.WsError
import dev.bridgething.schema.WsErrorConnectFailedInner
import dev.bridgething.schema.WsFrame
import java.io.IOException
import java.net.SocketTimeoutException
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import okhttp3.Headers
import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString

/**
 * Net surface implementation: subscribes to bridge -> gateway Net traffic
 * (Fetch + WsOpen requests, WsClose / WsSend / StreamOpen / StreamCancel
 * commands) and answers with OkHttp.
 *
 * Per-connection state (websocket sockets, in-flight stream jobs) is
 * keyed by the wire connection / stream UUID and cleaned up on terminal
 * state.
 */
public class NetDispatcher(
    private val client: OkHttpClient = defaultClient(),
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val mutex = Mutex()

    private var fetchJob: Job? = null
    private var wsOpenJob: Job? = null
    private var wsCloseJob: Job? = null
    private var wsSendJob: Job? = null
    private var streamOpenJob: Job? = null
    private var streamCancelJob: Job? = null

    private val wsConnections = ConcurrentHashMap<UUID, WebSocket>()
    private val streamJobs = ConcurrentHashMap<UUID, Job>()

    public suspend fun start(gateway: BridgethingGateway) {
        mutex.withLock {
            fetchJob?.cancel()
            wsOpenJob?.cancel()
            wsCloseJob?.cancel()
            wsSendJob?.cancel()
            streamOpenJob?.cancel()
            streamCancelJob?.cancel()

            fetchJob = scope.launch {
                gateway.net.fetchRequests.collect { (handle, msg) ->
                    launch { handleFetch(handle, msg.request) }
                }
            }
            wsOpenJob = scope.launch {
                gateway.net.wsOpenRequests.collect { (handle, req) ->
                    launch { handleWsOpen(handle, req, gateway) }
                }
            }
            wsCloseJob = scope.launch {
                gateway.net.wsClose.collect { (_, msg) ->
                    handleWsClose(msg.connectionId, msg.code, msg.reason)
                }
            }
            wsSendJob = scope.launch {
                gateway.net.wsSend.collect { (_, msg) ->
                    handleWsSend(msg.connectionId, msg.frame)
                }
            }
            streamOpenJob = scope.launch {
                gateway.net.streamOpen.collect { (deviceId, msg) ->
                    val job = scope.launch {
                        runStream(deviceId, msg.streamId, msg.request, gateway)
                    }
                    streamJobs[msg.streamId] = job
                    job.invokeOnCompletion { streamJobs.remove(msg.streamId) }
                }
            }
            streamCancelJob = scope.launch {
                gateway.net.streamCancel.collect { (_, msg) ->
                    streamJobs.remove(msg.streamId)?.cancel()
                }
            }
        }
    }

    public suspend fun stop() {
        mutex.withLock {
            fetchJob?.cancel(); fetchJob = null
            wsOpenJob?.cancel(); wsOpenJob = null
            wsCloseJob?.cancel(); wsCloseJob = null
            wsSendJob?.cancel(); wsSendJob = null
            streamOpenJob?.cancel(); streamOpenJob = null
            streamCancelJob?.cancel(); streamCancelJob = null
        }
        for ((_, socket) in wsConnections) {
            socket.close(WS_NORMAL_CLOSURE, null)
        }
        wsConnections.clear()
        for ((_, job) in streamJobs) {
            job.cancel()
        }
        streamJobs.clear()
    }

    private suspend fun handleFetch(
        handle: dev.bridgething.gateway.NetFetchRequestMsgHandle,
        req: NetFetchRequest,
    ) {
        val request = buildOkRequest(req)
        if (request == null) {
            runCatching {
                handle.respondErr(
                    NetFetchErrorReply(
                        error = NetError.RequestFailed(NetErrorRequestFailedInner(reason = "invalid url"))
                    )
                )
            }
            return
        }
        val call = client.newBuilder()
            .applyTimeout(req.timeoutMs)
            .build()
            .newCall(request)

        try {
            val response = call.execute()
            response.use { resp ->
                val status = resp.code.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()
                val headers = resp.headers.toWireHeaders()
                val body = resp.body?.bytes() ?: ByteArray(0)
                runCatching {
                    handle.respond(NetFetchReply(NetFetchResponse(status = status, headers = headers, body = body)))
                }
            }
        } catch (_: SocketTimeoutException) {
            runCatching {
                handle.respondErr(NetFetchErrorReply(error = NetError.Timeout))
            }
        } catch (e: IOException) {
            runCatching {
                handle.respondErr(
                    NetFetchErrorReply(
                        error = NetError.RequestFailed(NetErrorRequestFailedInner(reason = e.message ?: e.toString()))
                    )
                )
            }
        }
    }

    private suspend fun handleWsOpen(
        handle: dev.bridgething.gateway.NetWsOpenHandle,
        req: dev.bridgething.schema.NetWsOpen,
        gateway: BridgethingGateway,
    ) {
        val builder = Request.Builder()
        try {
            builder.url(req.url)
        } catch (e: IllegalArgumentException) {
            runCatching {
                handle.respondErr(
                    NetWsErrorReply(
                        error = WsError.ConnectFailed(WsErrorConnectFailedInner(reason = e.message ?: "invalid url"))
                    )
                )
            }
            return
        }
        req.headers?.forEach { builder.addHeader(it.name, it.value) }
        req.protocols?.takeIf { it.isNotEmpty() }?.let {
            builder.addHeader("Sec-WebSocket-Protocol", it.joinToString(", "))
        }
        val opened = CompletableDeferred<Unit>()
        val connId = req.connectionId
        val listener = object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                wsConnections[connId] = webSocket
                opened.complete(Unit)
            }

            override fun onMessage(webSocket: WebSocket, text: String) {
                scope.launch {
                    runCatching {
                        gateway.net.wsMessage(NetWsMessage(connectionId = connId, frame = WsFrame.Text(text)))
                    }
                }
            }

            override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                scope.launch {
                    runCatching {
                        gateway.net.wsMessage(NetWsMessage(connectionId = connId, frame = WsFrame.Binary(bytes.toByteArray())))
                    }
                }
            }

            override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                webSocket.close(code, reason)
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                scope.launch {
                    wsConnections.remove(connId)
                    runCatching {
                        gateway.net.wsClosed(
                            NetWsClosed(
                                connectionId = connId,
                                code = code.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort(),
                                reason = reason,
                            )
                        )
                    }
                }
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                if (!opened.isCompleted) {
                    opened.completeExceptionally(t)
                    return
                }
                scope.launch {
                    wsConnections.remove(connId)
                    runCatching {
                        gateway.net.wsClosed(
                            NetWsClosed(
                                connectionId = connId,
                                code = WS_ABNORMAL_CLOSURE,
                                reason = t.message ?: t.toString(),
                            )
                        )
                    }
                }
            }
        }
        client.newWebSocket(builder.build(), listener)
        try {
            opened.await()
            runCatching { handle.respond(NetWsOpenReply(acceptedProtocol = null)) }
        } catch (e: Throwable) {
            runCatching {
                handle.respondErr(
                    NetWsErrorReply(
                        error = WsError.ConnectFailed(WsErrorConnectFailedInner(reason = e.message ?: e.toString()))
                    )
                )
            }
        }
    }

    private fun handleWsClose(connectionId: UUID, code: UShort?, reason: String?) {
        val socket = wsConnections.remove(connectionId) ?: return
        socket.close(code?.toInt() ?: WS_NORMAL_CLOSURE, reason)
    }

    private fun handleWsSend(connectionId: UUID, frame: WsFrame) {
        val socket = wsConnections[connectionId] ?: return
        when (frame) {
            is WsFrame.Text -> socket.send(frame.data)
            is WsFrame.Binary -> socket.send(ByteString.of(*frame.data))
        }
    }

    private suspend fun runStream(
        deviceId: String,
        streamId: UUID,
        req: NetFetchRequest,
        gateway: BridgethingGateway,
    ) {
        val request = buildOkRequest(req)
        if (request == null) {
            runCatching {
                gateway.net.streamError(
                    StreamError(
                        streamId = streamId,
                        error = NetError.RequestFailed(NetErrorRequestFailedInner(reason = "invalid url"))
                    )
                )
            }
            return
        }
        val call = client.newCall(request)
        try {
            val response = call.execute()
            response.use { resp ->
                val status = resp.code.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()
                val headers = resp.headers.toWireHeaders()
                val totalSize = resp.body?.contentLength()?.takeIf { it >= 0 }?.let {
                    it.coerceAtMost(UInt.MAX_VALUE.toLong()).toUInt()
                }
                gateway.device(deviceId).net.streamBegin(
                    StreamBegin(streamId = streamId, status = status, headers = headers, totalSize = totalSize),
                    priority = Priority.Bulk,
                )

                val source = resp.body?.source() ?: run {
                    gateway.device(deviceId).net.streamEnd(StreamEnd(streamId = streamId), priority = Priority.Bulk)
                    return
                }
                val buffer = okio.Buffer()
                var offset: UInt = 0u
                val chunkSize = 8 * 1024L
                while (!source.exhausted()) {
                    val read = source.read(buffer, chunkSize)
                    if (read <= 0) break
                    val bytes = buffer.readByteArray()
                    gateway.device(deviceId).net.streamChunk(
                        StreamChunk(streamId = streamId, offset = offset, bytes = bytes),
                        priority = Priority.Bulk,
                    )
                    offset = (offset.toLong() + bytes.size.toLong())
                        .coerceAtMost(UInt.MAX_VALUE.toLong())
                        .toUInt()
                }
                gateway.device(deviceId).net.streamEnd(StreamEnd(streamId = streamId), priority = Priority.Bulk)
            }
        } catch (_: SocketTimeoutException) {
            runCatching {
                gateway.net.streamError(StreamError(streamId = streamId, error = NetError.Timeout))
            }
        } catch (e: IOException) {
            runCatching {
                gateway.net.streamError(
                    StreamError(
                        streamId = streamId,
                        error = NetError.RequestFailed(NetErrorRequestFailedInner(reason = e.message ?: e.toString()))
                    )
                )
            }
        }
    }

    private fun buildOkRequest(req: NetFetchRequest): Request? {
        val builder = Request.Builder()
        try {
            builder.url(req.url)
        } catch (_: IllegalArgumentException) {
            return null
        }
        val body: RequestBody? = req.body?.let { bytes ->
            // honor an explicit Content-Type header if present; null lets OkHttp omit it.
            val contentType = req.headers
                .firstOrNull { it.name.equals("content-type", ignoreCase = true) }
                ?.value
                ?.toMediaTypeOrNull()
            bytes.toRequestBody(contentType)
        }
        builder.method(req.method.string, body)
        for (header in req.headers) {
            // Content-Type rides the body; OkHttp rejects duplicate keys on body-carrying methods.
            if (header.name.equals("content-type", ignoreCase = true) && body != null) continue
            builder.addHeader(header.name, header.value)
        }
        return builder.build()
    }

    private fun Headers.toWireHeaders(): List<HttpHeader> = buildList(size) {
        for (i in 0 until size) {
            add(HttpHeader(name = name(i), value = value(i)))
        }
    }

    private fun OkHttpClient.Builder.applyTimeout(timeoutMs: UInt?): OkHttpClient.Builder {
        val ms = timeoutMs?.toLong() ?: return this
        return callTimeout(ms, TimeUnit.MILLISECONDS)
    }

    private companion object {
        const val WS_NORMAL_CLOSURE = 1000
        val WS_ABNORMAL_CLOSURE: UShort = 1006u

        fun defaultClient(): OkHttpClient = OkHttpClient.Builder()
            .followRedirects(true)
            .build()
    }

    public fun close() {
        scope.cancel()
        client.dispatcher.executorService.shutdown()
        client.connectionPool.evictAll()
    }
}
