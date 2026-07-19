package com.bridgething

import android.bluetooth.BluetoothDevice
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.util.Log
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

public object BondWatcher {
    private const val TAG = "BridgethingBT"
    private const val PAIR_WINDOW_MS = 120_000L
    private const val BOND_TIMEOUT_MS = 60_000L
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val pairing = ConcurrentHashMap<String, Long>()
    private val waiters = ConcurrentHashMap<String, MutableList<CompletableDeferred<Boolean>>>()
    @Volatile private var registered = false

    public fun register(context: Context) {
        if (registered) return
        registered = true
        val appCtx = context.applicationContext
        val filter = IntentFilter(BluetoothDevice.ACTION_BOND_STATE_CHANGED)
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.TIRAMISU) {
            appCtx.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
        } else {
            @Suppress("UnspecifiedRegisterReceiverFlag")
            appCtx.registerReceiver(receiver, filter)
        }
        Log.i(TAG, "bond watcher registered")
    }

    public fun beginPairing(mac: String) {
        pairing[mac.uppercase()] = System.currentTimeMillis() + PAIR_WINDOW_MS
        Log.i(TAG, "pairing window opened for ${mac.uppercase()}")
    }

    public fun endPairing(mac: String) {
        pairing.remove(mac.uppercase())
    }

    public fun isPairing(mac: String): Boolean {
        val key = mac.uppercase()
        val deadline = pairing[key] ?: return false
        if (System.currentTimeMillis() > deadline) {
            pairing.remove(key)
            return false
        }
        return true
    }

    public suspend fun awaitBonded(device: BluetoothDevice): Boolean {
        val mac = device.address?.uppercase() ?: return false
        if (bondStateOf(device) == BluetoothDevice.BOND_BONDED) return true

        val waiter = CompletableDeferred<Boolean>()
        waiters.compute(mac) { _, existing -> (existing ?: mutableListOf()).also { it.add(waiter) } }
        if (bondStateOf(device) == BluetoothDevice.BOND_BONDED) {
            resolve(mac, true)
            return true
        }

        val bonded = withTimeoutOrNull(BOND_TIMEOUT_MS) { waiter.await() } ?: false
        if (!bonded) {
            Log.w(TAG, "bond for $mac did not land (timeout or failure)")
            waiters[mac]?.remove(waiter)
            endPairing(mac)
        }
        return bonded
    }

    private val receiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            if (intent.action != BluetoothDevice.ACTION_BOND_STATE_CHANGED) return
            val device = deviceOf(intent) ?: return
            val mac = device.address?.uppercase() ?: return
            val state = intent.getIntExtra(BluetoothDevice.EXTRA_BOND_STATE, BluetoothDevice.ERROR)
            val prior = intent.getIntExtra(BluetoothDevice.EXTRA_PREVIOUS_BOND_STATE, BluetoothDevice.ERROR)
            Log.i(TAG, "bond state $mac: ${name(prior)} -> ${name(state)}")

            when (state) {
                BluetoothDevice.BOND_BONDED -> {
                    endPairing(mac)
                    resolve(mac, true)
                    scope.launch {
                        CompanionHolder.connectBonded(context.applicationContext, device)
                    }
                }

                BluetoothDevice.BOND_NONE -> {
                    if (isPairing(mac)) {
                        Log.w(TAG, "pairing failed for $mac (bond fell to NONE); needs a manual retry")
                        endPairing(mac)
                        resolve(mac, false)
                        return
                    }
                    Log.w(TAG, "bond for $mac lost; dropping device (no background re-pair)")
                    scope.launch { CompanionHolder.forgetDevice(mac) }
                }
            }
        }
    }

    private fun resolve(mac: String, bonded: Boolean) {
        waiters.remove(mac)?.forEach { it.complete(bonded) }
    }

    private fun bondStateOf(device: BluetoothDevice): Int =
        try { device.bondState } catch (_: SecurityException) { BluetoothDevice.BOND_NONE }

    private fun deviceOf(intent: Intent): BluetoothDevice? =
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.TIRAMISU) {
            intent.getParcelableExtra(BluetoothDevice.EXTRA_DEVICE, BluetoothDevice::class.java)
        } else {
            @Suppress("DEPRECATION") intent.getParcelableExtra(BluetoothDevice.EXTRA_DEVICE)
        }

    private fun name(state: Int): String = when (state) {
        BluetoothDevice.BOND_NONE -> "NONE"
        BluetoothDevice.BOND_BONDING -> "BONDING"
        BluetoothDevice.BOND_BONDED -> "BONDED"
        else -> "?($state)"
    }
}
