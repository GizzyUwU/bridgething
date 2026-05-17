package dev.bridgething.companion

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.location.Location
import android.location.LocationListener
import android.location.LocationManager
import android.os.Bundle
import android.os.Looper
import androidx.core.content.ContextCompat
import dev.bridgething.gateway.BridgethingGateway
import dev.bridgething.gateway.GeoGetOnceHandle
import dev.bridgething.gateway.geo
import dev.bridgething.schema.GeoAccuracy
import dev.bridgething.schema.GeoError
import dev.bridgething.schema.GeoErrorReply
import dev.bridgething.schema.GeoGetOnce
import dev.bridgething.schema.GeoGetOnceReply
import dev.bridgething.schema.GeoWatch
import dev.bridgething.schema.Position
import java.util.concurrent.Executor
import java.util.concurrent.Executors
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/**
 * Geo surface implementation. Tries Google Play Services'
 * FusedLocationProvider first (best accuracy + battery); falls back to
 * AOSP [LocationManager] when Play Services is missing (degoogled phones,
 * GrapheneOS, etc). Mirror of Swift `GeoController`.
 *
 * The fused provider is loaded reflectively so the companion module
 * doesn't have a hard compile-time dependency on play-services-location:
 * the host app adds the dep if it wants Fused, and we degrade gracefully
 * when the dep isn't on the classpath at runtime.
 */
public class GeoController(
    private val context: Context,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val mutex = Mutex()
    private val callbackExecutor: Executor = Executors.newSingleThreadExecutor { r ->
        Thread(r, "bridgething-geo").apply { isDaemon = true }
    }

    private var watchJob: Job? = null
    private var unwatchJob: Job? = null
    private var getOnceJob: Job? = null

    private var gatewayRef: BridgethingGateway? = null
    private var watching: Boolean = false
    private var fused: FusedBackend? = null
    private var legacy: LegacyBackend? = null

    public suspend fun start(gateway: BridgethingGateway) {
        mutex.withLock {
            gatewayRef = gateway
            watchJob?.cancel()
            unwatchJob?.cancel()
            getOnceJob?.cancel()
            watchJob = scope.launch {
                gateway.geo.watch.collect { (_, msg) -> handleWatch(msg) }
            }
            unwatchJob = scope.launch {
                gateway.geo.unwatch.collect { handleUnwatch() }
            }
            getOnceJob = scope.launch {
                gateway.geo.getOnceRequests.collect { (handle, req) ->
                    launch { handleGetOnce(handle, req) }
                }
            }
        }
    }

    public suspend fun stop() {
        mutex.withLock {
            watchJob?.cancel(); watchJob = null
            unwatchJob?.cancel(); unwatchJob = null
            getOnceJob?.cancel(); getOnceJob = null
            if (watching) {
                fused?.stopWatch()
                legacy?.stopWatch()
                watching = false
            }
            fused = null
            legacy = null
            gatewayRef = null
        }
    }

    // MARK: - watch / unwatch

    private suspend fun handleWatch(watch: GeoWatch) {
        if (!hasLocationPermission()) return
        val gateway = gatewayRef ?: return
        val onLocation: (Location) -> Unit = { loc ->
            scope.launch {
                runCatching { gateway.geo.position(makePosition(loc)) }
            }
        }
        val started = ensureFused()?.startWatch(watch.accuracy, callbackExecutor, onLocation)
            ?: ensureLegacy().startWatch(watch.accuracy, onLocation)
        if (started) watching = true
    }

    private fun handleUnwatch() {
        if (!watching) return
        fused?.stopWatch()
        legacy?.stopWatch()
        watching = false
    }

    // MARK: - get-once

    private suspend fun handleGetOnce(handle: GeoGetOnceHandle, req: GeoGetOnce) {
        if (!hasLocationPermission()) {
            runCatching { handle.respondErr(GeoErrorReply(error = GeoError.PermissionDenied)) }
            return
        }
        val location = try {
            ensureFused()?.getOnce(req.accuracy, callbackExecutor)
                ?: ensureLegacy().getOnce(req.accuracy)
        } catch (e: Throwable) {
            runCatching { handle.respondErr(GeoErrorReply(error = GeoError.Unavailable)) }
            return
        }
        if (location == null) {
            runCatching { handle.respondErr(GeoErrorReply(error = GeoError.Unavailable)) }
            return
        }
        runCatching {
            handle.respond(GeoGetOnceReply(position = makePosition(location)))
        }
    }

    // MARK: - backends

    private fun ensureFused(): FusedBackend? {
        fused?.let { return it }
        val backend = FusedBackend.tryCreate(context) ?: return null
        fused = backend
        return backend
    }

    private fun ensureLegacy(): LegacyBackend {
        legacy?.let { return it }
        val backend = LegacyBackend(context)
        legacy = backend
        return backend
    }

    private fun hasLocationPermission(): Boolean {
        val fine = ContextCompat.checkSelfPermission(context, Manifest.permission.ACCESS_FINE_LOCATION)
        val coarse = ContextCompat.checkSelfPermission(context, Manifest.permission.ACCESS_COARSE_LOCATION)
        return fine == PackageManager.PERMISSION_GRANTED || coarse == PackageManager.PERMISSION_GRANTED
    }

    private fun makePosition(loc: Location): Position {
        val ts = loc.time.coerceAtLeast(0L) / 1000L
        return Position(
            lat = loc.latitude,
            lon = loc.longitude,
            altM = if (loc.hasAltitude()) loc.altitude.toFloat() else null,
            accuracyM = if (loc.hasAccuracy()) loc.accuracy.coerceAtLeast(0f) else 0f,
            speedMps = if (loc.hasSpeed()) loc.speed else null,
            headingDeg = if (loc.hasBearing()) loc.bearing else null,
            tsUnixS = ts.coerceAtMost(UInt.MAX_VALUE.toLong()).toUInt(),
        )
    }
}

// ---- Fused (Google Play Services) backend, loaded reflectively. ----

/**
 * Reflective wrapper over `com.google.android.gms.location.FusedLocationProviderClient`.
 * Only present when `play-services-location` is on the classpath; we
 * detect that at `tryCreate` time and return null otherwise, so the
 * companion still works on degoogled devices.
 */
private class FusedBackend private constructor(
    private val client: Any,
    private val priorityHighAccuracy: Int,
    private val priorityBalanced: Int,
    private val locationRequestClass: Class<*>,
    private val requestBuilderClass: Class<*>,
    private val callbackClass: Class<*>,
    private val currentLocationMethod: java.lang.reflect.Method,
    private val requestUpdatesMethod: java.lang.reflect.Method,
    private val removeUpdatesMethod: java.lang.reflect.Method,
) {
    @Volatile
    private var activeCallback: Any? = null

    suspend fun startWatch(accuracy: GeoAccuracy, executor: Executor, onLocation: (Location) -> Unit): Boolean {
        val request = buildRequest(accuracy, intervalMs = 1000L)
        val callback = makeCallback(onLocation)
        requestUpdatesMethod.invoke(client, request, executor, callback)
        activeCallback = callback
        return true
    }

    fun stopWatch() {
        val cb = activeCallback ?: return
        runCatching { removeUpdatesMethod.invoke(client, cb) }
        activeCallback = null
    }

    suspend fun getOnce(accuracy: GeoAccuracy, executor: Executor): Location? {
        val priorityCls = Class.forName("com.google.android.gms.location.Priority")
        val priority = priorityCls.getField(
            if (accuracy == GeoAccuracy.Fine) "PRIORITY_HIGH_ACCURACY" else "PRIORITY_BALANCED_POWER_ACCURACY"
        ).getInt(null)
        val tokenCls = Class.forName("com.google.android.gms.tasks.CancellationTokenSource")
        val tokenSource = tokenCls.getDeclaredConstructor().newInstance()
        val tokenGetter = tokenCls.getMethod("getToken")
        val token = tokenGetter.invoke(tokenSource)
        val tokenParamCls = Class.forName("com.google.android.gms.tasks.CancellationToken")
        val task = currentLocationMethod.invoke(client, priority, tokenParamCls.cast(token))
        return awaitTask(task)
    }

    private fun buildRequest(accuracy: GeoAccuracy, intervalMs: Long): Any {
        val priority = if (accuracy == GeoAccuracy.Fine) priorityHighAccuracy else priorityBalanced
        val builder = requestBuilderClass.getDeclaredConstructor(Int::class.javaPrimitiveType, Long::class.javaPrimitiveType)
            .newInstance(priority, intervalMs)
        val build = requestBuilderClass.getMethod("build")
        return build.invoke(builder)!!
    }

    private fun makeCallback(onLocation: (Location) -> Unit): Any {
        return java.lang.reflect.Proxy.newProxyInstance(
            callbackClass.classLoader,
            arrayOf(callbackClass),
        ) { _, method, args ->
            when (method.name) {
                "onLocationResult" -> {
                    val result = args?.firstOrNull()
                    if (result != null) {
                        val getLast = result.javaClass.getMethod("getLastLocation")
                        val loc = getLast.invoke(result) as? Location
                        if (loc != null) onLocation(loc)
                    }
                    null
                }
                else -> defaultProxyReturn(method)
            }
        }
    }

    private suspend fun awaitTask(task: Any?): Location? {
        if (task == null) return null
        val taskCls = task.javaClass
        val isComplete = taskCls.getMethod("isComplete")
        val isSuccessful = taskCls.getMethod("isSuccessful")
        val getResult = taskCls.getMethod("getResult")
        // Spin lightly via a coroutine continuation by hooking a listener.
        val listenerCls = Class.forName("com.google.android.gms.tasks.OnCompleteListener")
        return kotlinx.coroutines.suspendCancellableCoroutine { cont ->
            val listener = java.lang.reflect.Proxy.newProxyInstance(
                listenerCls.classLoader,
                arrayOf(listenerCls),
            ) { _, method, args ->
                if (method.name == "onComplete") {
                    val t = args?.firstOrNull()
                    if (t != null) {
                        val complete = isComplete.invoke(t) as? Boolean ?: false
                        val ok = isSuccessful.invoke(t) as? Boolean ?: false
                        if (complete && ok) {
                            val loc = getResult.invoke(t) as? Location
                            cont.resumeWith(Result.success(loc))
                        } else {
                            cont.resumeWith(Result.success(null))
                        }
                    } else {
                        cont.resumeWith(Result.success(null))
                    }
                }
                null
            }
            val addListener = taskCls.getMethod("addOnCompleteListener", listenerCls)
            addListener.invoke(task, listener)
        }
    }

    companion object {
        fun tryCreate(context: Context): FusedBackend? = try {
            val locationServicesCls = Class.forName("com.google.android.gms.location.LocationServices")
            val clientFactory = locationServicesCls.getMethod("getFusedLocationProviderClient", Context::class.java)
            val client = clientFactory.invoke(null, context.applicationContext) ?: return null
            val priorityCls = Class.forName("com.google.android.gms.location.Priority")
            val priorityHigh = priorityCls.getField("PRIORITY_HIGH_ACCURACY").getInt(null)
            val priorityBalanced = priorityCls.getField("PRIORITY_BALANCED_POWER_ACCURACY").getInt(null)
            val locationRequestCls = Class.forName("com.google.android.gms.location.LocationRequest")
            val builderCls = Class.forName("com.google.android.gms.location.LocationRequest\$Builder")
            val callbackCls = Class.forName("com.google.android.gms.location.LocationCallback")
            val currentLocation = client.javaClass.getMethod(
                "getCurrentLocation",
                Int::class.javaPrimitiveType,
                Class.forName("com.google.android.gms.tasks.CancellationToken"),
            )
            val requestUpdates = client.javaClass.getMethod(
                "requestLocationUpdates",
                locationRequestCls,
                Executor::class.java,
                callbackCls,
            )
            val removeUpdates = client.javaClass.getMethod("removeLocationUpdates", callbackCls)
            FusedBackend(
                client = client,
                priorityHighAccuracy = priorityHigh,
                priorityBalanced = priorityBalanced,
                locationRequestClass = locationRequestCls,
                requestBuilderClass = builderCls,
                callbackClass = callbackCls,
                currentLocationMethod = currentLocation,
                requestUpdatesMethod = requestUpdates,
                removeUpdatesMethod = removeUpdates,
            )
        } catch (_: Throwable) {
            null
        }

        // Reflective interface proxies need primitive defaults for the
        // non-implemented methods (equals/hashCode/toString); java.lang.reflect.Proxy
        // will pass them through to us. Return sensible primitives.
        fun defaultProxyReturn(method: java.lang.reflect.Method): Any? = when (method.returnType) {
            Boolean::class.javaPrimitiveType -> false
            Int::class.javaPrimitiveType -> 0
            else -> null
        }
    }
}

// ---- Legacy (AOSP LocationManager) backend. ----

/**
 * AOSP fallback used when Play Services isn't on the device. Worse battery
 * + accuracy than Fused but works on any android. Subscribes to GPS or
 * Network providers depending on accuracy.
 */
private class LegacyBackend(
    private val context: Context,
) {
    private val manager = context.applicationContext.getSystemService(Context.LOCATION_SERVICE) as LocationManager
    private var listener: LocationListener? = null

    fun startWatch(accuracy: GeoAccuracy, onLocation: (Location) -> Unit): Boolean {
        if (!hasPermission()) return false
        val provider = pickProvider(accuracy) ?: return false
        stopWatch()
        val l = LocationListener { loc -> onLocation(loc) }
        listener = l
        try {
            manager.requestLocationUpdates(provider, 1000L, 0f, l, Looper.getMainLooper())
            return true
        } catch (_: SecurityException) {
            listener = null
            return false
        }
    }

    fun stopWatch() {
        listener?.let { manager.removeUpdates(it) }
        listener = null
    }

    suspend fun getOnce(accuracy: GeoAccuracy): Location? {
        if (!hasPermission()) return null
        val provider = pickProvider(accuracy) ?: return null
        // Try last-known first; if missing fall back to a single-shot
        // subscribe with a short timeout.
        try {
            val last = manager.getLastKnownLocation(provider)
            if (last != null) return last
        } catch (_: SecurityException) {
            return null
        }
        return kotlinx.coroutines.withTimeoutOrNull(SINGLE_SHOT_TIMEOUT_MS) {
            kotlinx.coroutines.suspendCancellableCoroutine { cont ->
                val l = object : LocationListener {
                    override fun onLocationChanged(location: Location) {
                        manager.removeUpdates(this)
                        if (cont.isActive) cont.resumeWith(Result.success(location))
                    }
                    override fun onProviderDisabled(provider: String) {}
                    override fun onProviderEnabled(provider: String) {}
                    @Suppress("DEPRECATION")
                    override fun onStatusChanged(provider: String?, status: Int, extras: Bundle?) {}
                }
                try {
                    manager.requestLocationUpdates(provider, 0L, 0f, l, Looper.getMainLooper())
                } catch (_: SecurityException) {
                    if (cont.isActive) cont.resumeWith(Result.success(null))
                }
                cont.invokeOnCancellation { manager.removeUpdates(l) }
            }
        }
    }

    private fun pickProvider(accuracy: GeoAccuracy): String? {
        val preferred = if (accuracy == GeoAccuracy.Fine) LocationManager.GPS_PROVIDER else LocationManager.NETWORK_PROVIDER
        val enabled = runCatching { manager.isProviderEnabled(preferred) }.getOrDefault(false)
        if (enabled) return preferred
        val fallback = if (preferred == LocationManager.GPS_PROVIDER) LocationManager.NETWORK_PROVIDER else LocationManager.GPS_PROVIDER
        val fbEnabled = runCatching { manager.isProviderEnabled(fallback) }.getOrDefault(false)
        return if (fbEnabled) fallback else null
    }

    private fun hasPermission(): Boolean {
        val fine = ContextCompat.checkSelfPermission(context, Manifest.permission.ACCESS_FINE_LOCATION)
        val coarse = ContextCompat.checkSelfPermission(context, Manifest.permission.ACCESS_COARSE_LOCATION)
        return fine == PackageManager.PERMISSION_GRANTED || coarse == PackageManager.PERMISSION_GRANTED
    }

    private companion object {
        const val SINGLE_SHOT_TIMEOUT_MS = 8_000L
    }
}
