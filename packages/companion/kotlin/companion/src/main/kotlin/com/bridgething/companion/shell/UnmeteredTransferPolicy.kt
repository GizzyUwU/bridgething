package com.bridgething.companion.shell

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import uniffi.bridgething_companion.TransferPolicy

public class UnmeteredTransferPolicy(
    context: Context,
) : TransferPolicy {
    private val appContext = context.applicationContext

    override fun allowsLargeTransfer(): Boolean {
        val manager = appContext.getSystemService(ConnectivityManager::class.java) ?: return false
        val caps = manager.activeNetwork?.let { manager.getNetworkCapabilities(it) }
        return caps?.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED) == true
    }
}
