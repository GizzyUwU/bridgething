package com.bridgething.spotify

/**
 * connectivity seam. [AndroidConnectivityWatcher] is the real ConnectivityManager-backed impl
 * (touches android at construction, so the host builds it); tests inject a fake. it reports
 * default-network availability transitions; the glue acts on lost->available edges to resync
 * the dealer.
 */
interface ConnectivityWatcher {
    fun start(onAvailability: (available: Boolean) -> Unit)
    fun stop()
}

object NoOpConnectivityWatcher : ConnectivityWatcher {
    override fun start(onAvailability: (Boolean) -> Unit) {}
    override fun stop() {}
}

class AndroidConnectivityWatcher(context: android.content.Context) : ConnectivityWatcher {
    private val connectivityManager = context.applicationContext
        .getSystemService(android.content.Context.CONNECTIVITY_SERVICE) as android.net.ConnectivityManager
    private var callback: android.net.ConnectivityManager.NetworkCallback? = null

    override fun start(onAvailability: (Boolean) -> Unit) {
        val cb = object : android.net.ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: android.net.Network) = onAvailability(true)
            override fun onLost(network: android.net.Network) = onAvailability(false)
        }
        callback = cb
        connectivityManager.registerDefaultNetworkCallback(cb)
    }

    override fun stop() {
        callback?.let { runCatching { connectivityManager.unregisterNetworkCallback(it) } }
        callback = null
    }
}
