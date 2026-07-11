package com.bridgething

import android.annotation.SuppressLint
import android.app.Activity
import android.bluetooth.BluetoothDevice
import android.companion.AssociationRequest
import android.companion.BluetoothDeviceFilter
import android.companion.CompanionDeviceManager
import android.content.Context
import android.content.IntentSender
import com.margelo.nitro.bridgething.session.BridgethingBtBondState
import com.margelo.nitro.bridgething.session.BridgethingBtDevice
import java.util.regex.Pattern
import kotlinx.coroutines.CompletableDeferred

/**
 * CompanionDeviceManager-backed pair flow. CDM handles scan + pair +
 * permission prompts in one OS-managed surface, avoiding `BLUETOOTH_SCAN`
 * runtime grants and custom picker UI. The trade-off is requiring a
 * foreground Activity for CDM's `IntentSender` launch.
 */
public object CompanionDevicePicker {
    private const val REQUEST_CDM_PICK = 0xBA01

    private val carThingNameRegex: Pattern =
        Pattern.compile("(Car Thing|bridgething)", Pattern.CASE_INSENSITIVE)

    @SuppressLint("MissingPermission")
    public suspend fun pick(context: Context): BridgethingBtDevice? {
        val activity = BridgethingActivityRegistry.currentActivity
            ?: error("CompanionDevicePicker needs a foreground activity")
        val manager = context.applicationContext
            .getSystemService(Context.COMPANION_DEVICE_SERVICE) as? CompanionDeviceManager
            ?: error("CompanionDeviceManager unavailable on this device")

        val deferred = CompletableDeferred<BridgethingBtDevice?>()
        BridgethingActivityRegistry.expectResult(REQUEST_CDM_PICK) { resultCode, data ->
            if (resultCode != Activity.RESULT_OK || data == null) {
                deferred.complete(null)
                return@expectResult
            }
            val device: BluetoothDevice? =
                if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.TIRAMISU) {
                    data.getParcelableExtra(CompanionDeviceManager.EXTRA_DEVICE, BluetoothDevice::class.java)
                } else {
                    @Suppress("DEPRECATION") data.getParcelableExtra(CompanionDeviceManager.EXTRA_DEVICE)
                }
            // Kick off bonding here, straight off the user's selection while we're
            // foreground, so Android shows the pairing DIALOG. If we instead let
            // the background RFCOMM connect trigger the bond, the system only
            // posts a tap-to-open notification.
            device?.let {
                if (it.bondState != BluetoothDevice.BOND_BONDED) {
                    runCatching { it.createBond() }
                }
            }
            deferred.complete(device?.let(::toWireDevice))
        }

        val builder = AssociationRequest.Builder()
            .addDeviceFilter(
                BluetoothDeviceFilter.Builder()
                    .setNamePattern(carThingNameRegex)
                    .build()
            )
            .setSingleDevice(false)
        // The WATCH companion profile grants the phone / call-log / contacts / notifications /
        // MANAGE_ONGOING_CALLS bundle via the companion role in a single consent. On Android 12+
        // a role-less association CANNOT receive PHONE_STATE or the caller number, so this is what
        // makes incoming-call display (and companion DTMF) work without claiming the default-dialer
        // role. DEVICE_PROFILE_WATCH is fully public; the method itself is API 31+.
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.S) {
            builder.setDeviceProfile(AssociationRequest.DEVICE_PROFILE_WATCH)
        }
        val request = builder.build()

        manager.associate(request, object : CompanionDeviceManager.Callback() {
            override fun onDeviceFound(chooserLauncher: IntentSender) {
                try {
                    activity.startIntentSenderForResult(
                        chooserLauncher, REQUEST_CDM_PICK,
                        null, 0, 0, 0,
                    )
                } catch (e: IntentSender.SendIntentException) {
                    BridgethingActivityRegistry.deliverResult(REQUEST_CDM_PICK, Activity.RESULT_CANCELED, null)
                    deferred.complete(null)
                    android.util.Log.w(TAG, "CDM intent sender failed: ${e.message}")
                }
            }

            override fun onFailure(error: CharSequence?) {
                BridgethingActivityRegistry.deliverResult(REQUEST_CDM_PICK, Activity.RESULT_CANCELED, null)
                deferred.complete(null)
                if (!error.isNullOrEmpty()) android.util.Log.i(TAG, "CDM picker failure: $error")
            }
        }, null)

        return deferred.await()
    }

    /**
     * MAC addresses the user has authorized via CDM for this app. Used
     * at session start to reopen RFCOMM sockets without prompting again.
     */
    public fun associations(context: Context): Set<String> {
        val manager = context.applicationContext
            .getSystemService(Context.COMPANION_DEVICE_SERVICE) as? CompanionDeviceManager
            ?: return emptySet()
        return try {
            if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.TIRAMISU) {
                manager.myAssociations.mapNotNullTo(mutableSetOf()) { it.deviceMacAddress?.toString() }
            } else {
                @Suppress("DEPRECATION") manager.associations.toSet()
            }
        } catch (_: Throwable) { emptySet() }
    }

    /**
     * Ask the system to bind our [BridgethingPresenceService] when an associated
     * Car Thing comes into BT range, so the app wakes from cold and reconnects
     * without the user opening it. API 31+ only; older versions rely on the app
     * being opened to start the connection service.
     */
    public fun startObservingPresence(context: Context) {
        if (android.os.Build.VERSION.SDK_INT < android.os.Build.VERSION_CODES.S) return
        val manager = context.applicationContext
            .getSystemService(Context.COMPANION_DEVICE_SERVICE) as? CompanionDeviceManager ?: return
        for (mac in associations(context)) {
            runCatching {
                @Suppress("DEPRECATION")
                manager.startObservingDevicePresence(mac)
            }
        }
    }

    /**
     * Forget an associated Car Thing: stop observing its presence and disassociate it so the OS no longer
     * wakes us for it and `associations()` no longer returns it.
     */
    public fun forget(context: Context, mac: String) {
        val manager = context.applicationContext
            .getSystemService(Context.COMPANION_DEVICE_SERVICE) as? CompanionDeviceManager ?: return
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.TIRAMISU) {
            val matches = runCatching { manager.myAssociations }.getOrDefault(emptyList())
                .filter { it.deviceMacAddress?.toString().equals(mac, ignoreCase = true) }
            for (assoc in matches) {
                assoc.deviceMacAddress?.toString()?.let { stopObserving(manager, it) }
                runCatching { manager.disassociate(assoc.id) }
            }
        } else {
            stopObserving(manager, mac)
            runCatching { @Suppress("DEPRECATION") manager.disassociate(mac) }
        }
    }

    private fun stopObserving(manager: CompanionDeviceManager, mac: String) {
        if (android.os.Build.VERSION.SDK_INT < android.os.Build.VERSION_CODES.S) return
        runCatching { @Suppress("DEPRECATION") manager.stopObservingDevicePresence(mac) }
    }

    private fun toWireDevice(device: BluetoothDevice): BridgethingBtDevice {
        val name = try { device.name } catch (_: SecurityException) { null }
        return BridgethingBtDevice(
            address = device.address ?: "",
            name = name,
            bondState = BridgethingBtBondState.BONDED,
            isCarThing = true,
        )
    }

    private const val TAG = "bridgething.cdm"
}
