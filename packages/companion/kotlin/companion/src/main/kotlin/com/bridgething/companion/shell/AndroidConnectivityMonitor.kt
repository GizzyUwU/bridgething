package com.bridgething.companion.shell

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import uniffi.bridgething_companion.ConnectivityInbox
import uniffi.bridgething_companion.ConnectivityMonitor

public class AndroidConnectivityMonitor(
    context: Context,
) : ConnectivityMonitor {
    private val connectivityManager = context.applicationContext
        .getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager

    @Volatile
    private var callback: ConnectivityManager.NetworkCallback? = null

    @Volatile
    private var heldInbox: ConnectivityInbox? = null

    override fun start(inbox: ConnectivityInbox) {
        stop()
        heldInbox = inbox
        val cb = object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) {
                runCatching { inbox.onChanged(true) }
            }

            override fun onLost(network: Network) {
                runCatching { inbox.onChanged(false) }
            }
        }
        callback = cb
        runCatching { connectivityManager.registerDefaultNetworkCallback(cb) }
    }

    override fun stop() {
        callback?.let { runCatching { connectivityManager.unregisterNetworkCallback(it) } }
        callback = null
        val previous = heldInbox
        heldInbox = null
        previous?.close()
    }
}
