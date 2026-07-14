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

/**
 * Watches `ACTION_BOND_STATE_CHANGED` and makes it - not a failing RFCOMM
 * connect - the thing that drives connecting.
 *
 * The old flow had no bond observer at all, so it inferred bonding by connecting
 * an RFCOMM socket to an unbonded device and letting Android start pairing off
 * the failure. That makes the retry loop a pairing-dialog generator, and leaves
 * "did the bond land?" answerable only by trying again.
 *
 * Bond state alone cannot tell "the user unpaired us" from "our own pair attempt
 * just failed" - both are BOND_NONE. That ambiguity is what sank the two earlier
 * fixes here (they gated reconnects on BOND_NONE and killed live pairing). So the
 * discriminator is explicit user intent, not bond state: [beginPairing] opens a
 * window, and BOND_NONE means different things inside and outside it.
 *
 *  - inside the window  -> the pair attempt failed. Report it, so the UI can say
 *    "pairing failed, retry". We never silently re-issue createBond(): re-bonding
 *    on our own is precisely how a second system dialog appears.
 *  - outside the window -> a genuine unbond. Forget the device so no background
 *    reconnect can hammer it and re-trigger pairing.
 */
public object BondWatcher {
    private const val TAG = "BridgethingBT"

    /** How long after the user picks a device we still treat BOND_NONE as pairing noise. */
    private const val PAIR_WINDOW_MS = 120_000L

    /** How long [awaitBonded] waits for a bond to land before calling it a failure. */
    private const val BOND_TIMEOUT_MS = 60_000L

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    /** MAC (uppercase) -> deadline. Present means the user is actively pairing that device. */
    private val pairing = ConcurrentHashMap<String, Long>()

    /** MAC (uppercase) -> callers waiting to learn whether the bond landed. */
    private val waiters = ConcurrentHashMap<String, MutableList<CompletableDeferred<Boolean>>>()

    @Volatile private var registered = false

    public fun register(context: Context) {
        if (registered) return
        registered = true
        val appCtx = context.applicationContext
        // Runtime-registered on purpose: implicit broadcasts like this one are not
        // deliverable to manifest receivers since Oreo. The process is kept alive
        // by the connected-device foreground service, so this outlives the UI.
        // RECEIVER_NOT_EXPORTED on Tiramisu+; older releases use the no-flags
        // overload. Protected system broadcasts are still delivered when not exported.
        val filter = IntentFilter(BluetoothDevice.ACTION_BOND_STATE_CHANGED)
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.TIRAMISU) {
            appCtx.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
        } else {
            @Suppress("UnspecifiedRegisterReceiverFlag")
            appCtx.registerReceiver(receiver, filter)
        }
        Log.i(TAG, "bond watcher registered")
    }

    /** Mark [mac] as being deliberately paired right now, so bond flapping is tolerated. */
    public fun beginPairing(mac: String) {
        pairing[mac.uppercase()] = System.currentTimeMillis() + PAIR_WINDOW_MS
        Log.i(TAG, "pairing window opened for ${mac.uppercase()}")
    }

    public fun endPairing(mac: String) {
        pairing.remove(mac.uppercase())
    }

    /** Whether [mac] is inside a live pairing window. Expired windows are swept. */
    public fun isPairing(mac: String): Boolean {
        val key = mac.uppercase()
        val deadline = pairing[key] ?: return false
        if (System.currentTimeMillis() > deadline) {
            pairing.remove(key)
            return false
        }
        return true
    }

    /**
     * Suspend until [device] bonds, or the attempt fails / times out. Returns
     * immediately if it is already bonded.
     *
     * This is what lets the pair flow report a real outcome. Without it a failed
     * bond was invisible to the UI, which just sat on a 45s "still connecting"
     * timeout and told the user to tap a Pair prompt that was no longer there.
     */
    public suspend fun awaitBonded(device: BluetoothDevice): Boolean {
        val mac = device.address?.uppercase() ?: return false
        if (bondStateOf(device) == BluetoothDevice.BOND_BONDED) return true

        val waiter = CompletableDeferred<Boolean>()
        waiters.compute(mac) { _, existing -> (existing ?: mutableListOf()).also { it.add(waiter) } }
        // Re-check: the bond may have landed between the check above and registering.
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
                    // The bond is the green light to connect. Nothing else opens an
                    // RFCOMM socket to an unbonded device any more, so this is the
                    // one path that brings a freshly-paired peer up.
                    scope.launch {
                        CompanionHolder.connectBonded(context.applicationContext, device)
                    }
                }

                BluetoothDevice.BOND_NONE -> {
                    if (isPairing(mac)) {
                        // The pair attempt failed - the Car Thing's bond dropped, or
                        // the user dismissed the dialog. BONDING -> NONE looks the
                        // same either way, so we do not guess and we do NOT re-issue
                        // createBond(): silently re-bonding on a flap is exactly how
                        // a second dialog appears. Report the failure and let the
                        // user retry deliberately, which gets them one fresh dialog.
                        Log.w(TAG, "pairing failed for $mac (bond fell to NONE); needs a manual retry")
                        endPairing(mac)
                        resolve(mac, false)
                        return
                    }
                    // A real unbond outside any pair attempt: user unpaired us, or
                    // the stack dropped the key. Forget it, so the retry loop cannot
                    // hammer an unbonded device and silently re-pair in the background.
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
