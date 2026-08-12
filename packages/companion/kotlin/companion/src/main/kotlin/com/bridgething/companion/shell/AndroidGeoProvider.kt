package com.bridgething.companion.shell

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.location.Location
import android.location.LocationListener
import android.location.LocationManager
import android.os.Bundle
import android.os.Looper
import androidx.core.content.ContextCompat
import java.util.concurrent.Executor
import java.util.concurrent.Executors
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withTimeoutOrNull
import uniffi.bridgething_companion.GeoAccuracy
import uniffi.bridgething_companion.GeoError
import uniffi.bridgething_companion.GeoInbox
import uniffi.bridgething_companion.GeoProvider
import uniffi.bridgething_companion.Position

private fun hasLocationPermission(context: Context): Boolean {
    val fine = ContextCompat.checkSelfPermission(context, Manifest.permission.ACCESS_FINE_LOCATION)
    val coarse = ContextCompat.checkSelfPermission(context, Manifest.permission.ACCESS_COARSE_LOCATION)
    return fine == PackageManager.PERMISSION_GRANTED || coarse == PackageManager.PERMISSION_GRANTED
}

public class AndroidGeoProvider(
    private val context: Context,
) : GeoProvider {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val callbackExecutor: Executor = Executors.newSingleThreadExecutor { r ->
        Thread(r, "bridgething-geo").apply { isDaemon = true }
    }

    private val lock = Any()
    private var inbox: GeoInbox? = null
    private var accuracy: GeoAccuracy = GeoAccuracy.COARSE
    private var watching = false
    private var onceJob: Job? = null
    private var fused: FusedBackend? = null
    private var legacy: LegacyBackend? = null

    override fun canProvideLocation(): Boolean = hasLocationPermission(context)

    override fun start(inbox: GeoInbox) {
        val previous = synchronized(lock) {
            val held = this.inbox
            this.inbox = inbox
            held
        }
        if (previous !== inbox) previous?.close()
    }

    override fun stop() {
        val previous = synchronized(lock) {
            if (watching) {
                fused?.stopWatch()
                legacy?.stopWatch()
                watching = false
            }
            onceJob?.cancel()
            onceJob = null
            fused = null
            legacy = null
            val held = inbox
            inbox = null
            held
        }
        previous?.close()
    }

    override fun configure(accuracy: GeoAccuracy) {
        synchronized(lock) { this.accuracy = accuracy }
    }

    override fun requestAuthorization() {}

    override fun startUpdating() {
        val wanted = synchronized(lock) { accuracy }
        if (!hasLocationPermission(context)) {
            report { it.onError(GeoError.PERMISSION_DENIED) }
            return
        }
        val onLocation: (Location) -> Unit = { loc -> report { it.onPosition(makePosition(loc)) } }
        val started = runCatching {
            ensureFused()?.startWatch(wanted, callbackExecutor, onLocation)
                ?: ensureLegacy().startWatch(wanted, onLocation)
        }.getOrDefault(false)
        synchronized(lock) { watching = started }
        if (!started) report { it.onError(GeoError.UNAVAILABLE) }
    }

    override fun stopUpdating() {
        synchronized(lock) {
            if (!watching) return
            watching = false
        }
        fused?.stopWatch()
        legacy?.stopWatch()
    }

    override fun requestOnce() {
        val wanted = synchronized(lock) { accuracy }
        if (!hasLocationPermission(context)) {
            report { it.onError(GeoError.PERMISSION_DENIED) }
            return
        }
        val job = scope.launch {
            val location = try {
                ensureFused()?.getOnce(wanted, callbackExecutor) ?: ensureLegacy().getOnce(wanted)
            } catch (_: Throwable) {
                null
            }
            when (location) {
                null -> report { it.onError(GeoError.UNAVAILABLE) }
                else -> report { it.onPosition(makePosition(location)) }
            }
        }
        synchronized(lock) {
            onceJob?.cancel()
            onceJob = job
        }
    }

    override fun cancelOnce() {
        synchronized(lock) {
            onceJob?.cancel()
            onceJob = null
        }
    }

    private fun report(deliver: (GeoInbox) -> Unit) {
        val held = synchronized(lock) { inbox } ?: return
        runCatching { deliver(held) }
    }

    private fun ensureFused(): FusedBackend? {
        synchronized(lock) { fused?.let { return it } }
        val backend = FusedBackend.tryCreate(context) ?: return null
        synchronized(lock) {
            fused?.let { return it }
            fused = backend
        }
        return backend
    }

    private fun ensureLegacy(): LegacyBackend {
        synchronized(lock) { legacy?.let { return it } }
        val backend = LegacyBackend(context)
        synchronized(lock) {
            legacy?.let { return it }
            legacy = backend
        }
        return backend
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

private class FusedBackend private constructor(
    private val client: Any,
    private val priorityHighAccuracy: Int,
    private val priorityBalanced: Int,
    private val requestBuilderClass: Class<*>,
    private val callbackClass: Class<*>,
    private val currentLocationMethod: java.lang.reflect.Method,
    private val requestUpdatesMethod: java.lang.reflect.Method,
    private val removeUpdatesMethod: java.lang.reflect.Method,
) {
    @Volatile
    private var activeCallback: Any? = null

    fun startWatch(accuracy: GeoAccuracy, executor: Executor, onLocation: (Location) -> Unit): Boolean {
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
            if (accuracy == GeoAccuracy.FINE) "PRIORITY_HIGH_ACCURACY" else "PRIORITY_BALANCED_POWER_ACCURACY",
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
        val priority = if (accuracy == GeoAccuracy.FINE) priorityHighAccuracy else priorityBalanced
        val builder = requestBuilderClass
            .getDeclaredConstructor(Int::class.javaPrimitiveType, Long::class.javaPrimitiveType)
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
        val listenerCls = Class.forName("com.google.android.gms.tasks.OnCompleteListener")
        return suspendCancellableCoroutine { cont ->
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
                requestBuilderClass = builderCls,
                callbackClass = callbackCls,
                currentLocationMethod = currentLocation,
                requestUpdatesMethod = requestUpdates,
                removeUpdatesMethod = removeUpdates,
            )
        } catch (_: Throwable) {
            null
        }

        fun defaultProxyReturn(method: java.lang.reflect.Method): Any? = when (method.returnType) {
            Boolean::class.javaPrimitiveType -> false
            Int::class.javaPrimitiveType -> 0
            else -> null
        }
    }
}

private class LegacyBackend(
    private val context: Context,
) {
    private val manager = context.applicationContext.getSystemService(Context.LOCATION_SERVICE) as LocationManager
    private var listener: LocationListener? = null

    fun startWatch(accuracy: GeoAccuracy, onLocation: (Location) -> Unit): Boolean {
        if (!hasLocationPermission(context)) return false
        val provider = pickProvider(accuracy) ?: return false
        stopWatch()
        val l = LocationListener { loc -> onLocation(loc) }
        listener = l
        return try {
            manager.requestLocationUpdates(provider, 1000L, 0f, l, Looper.getMainLooper())
            true
        } catch (_: SecurityException) {
            listener = null
            false
        }
    }

    fun stopWatch() {
        listener?.let { manager.removeUpdates(it) }
        listener = null
    }

    suspend fun getOnce(accuracy: GeoAccuracy): Location? {
        if (!hasLocationPermission(context)) return null
        val provider = pickProvider(accuracy) ?: return null
        try {
            val last = manager.getLastKnownLocation(provider)
            if (last != null) return last
        } catch (_: SecurityException) {
            return null
        }
        return withTimeoutOrNull(SINGLE_SHOT_TIMEOUT_MS) {
            suspendCancellableCoroutine { cont ->
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
        val preferred =
            if (accuracy == GeoAccuracy.FINE) LocationManager.GPS_PROVIDER else LocationManager.NETWORK_PROVIDER
        val enabled = runCatching { manager.isProviderEnabled(preferred) }.getOrDefault(false)
        if (enabled) return preferred
        val fallback =
            if (preferred == LocationManager.GPS_PROVIDER) LocationManager.NETWORK_PROVIDER else LocationManager.GPS_PROVIDER
        val fbEnabled = runCatching { manager.isProviderEnabled(fallback) }.getOrDefault(false)
        return if (fbEnabled) fallback else null
    }

    private companion object {
        const val SINGLE_SHOT_TIMEOUT_MS = 8_000L
    }
}
