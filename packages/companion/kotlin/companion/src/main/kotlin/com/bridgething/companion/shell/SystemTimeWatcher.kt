package com.bridgething.companion.shell

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Build
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

public class SystemTimeWatcher(
    context: Context,
    private val onChanged: suspend () -> Unit,
) {
    private val appContext = context.applicationContext

    @Volatile
    private var receiver: BroadcastReceiver? = null

    @Volatile
    private var scope: CoroutineScope? = null

    public fun start() {
        stop()
        val running = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        scope = running
        val recv = object : BroadcastReceiver() {
            override fun onReceive(c: Context?, intent: Intent?) {
                if (intent?.action !in ACTIONS) return
                running.launch { runCatching { onChanged() } }
            }
        }
        receiver = recv
        val filter = IntentFilter().apply { ACTIONS.forEach(::addAction) }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            appContext.registerReceiver(recv, filter, Context.RECEIVER_NOT_EXPORTED)
        } else {
            @Suppress("UnspecifiedRegisterReceiverFlag")
            appContext.registerReceiver(recv, filter)
        }
    }

    public fun stop() {
        receiver?.let { runCatching { appContext.unregisterReceiver(it) } }
        receiver = null
        scope?.cancel()
        scope = null
    }

    private companion object {
        val ACTIONS = setOf(Intent.ACTION_TIMEZONE_CHANGED, Intent.ACTION_TIME_CHANGED)
    }
}
