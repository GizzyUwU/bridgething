package com.bridgething

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
 * CompanionDeviceManager-backed pair flow. Mirror of the iOS
 * AccessorySetupKit `AccessoryPickerCoordinator` - same idea, same UX,
 * same Promise<BtDevice?> contract.
 *
 * Why CDM over the older `BluetoothAdapter.startDiscovery` +
 * `device.createBond` dance: the system picker handles scan + pair +
 * permission prompts in one OS-managed surface. No `BLUETOOTH_SCAN`
 * runtime grant, no `neverForLocation` workaround, no in-app picker UI
 * to maintain. The trade-off is needing the foreground Activity
 * (CDM's `IntentSender` is activity-launched) - hence
 * [BridgethingActivityRegistry].
 */
public object CompanionDevicePicker {
    private const val REQUEST_CDM_PICK = 0xBA01

    private val carThingNameRegex: Pattern =
        Pattern.compile("(Car Thing|bridgething)", Pattern.CASE_INSENSITIVE)

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
            deferred.complete(device?.let(::toWireDevice))
        }

        val request = AssociationRequest.Builder()
            .addDeviceFilter(
                BluetoothDeviceFilter.Builder()
                    .setNamePattern(carThingNameRegex)
                    .build()
            )
            .setSingleDevice(false)
            .build()

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
