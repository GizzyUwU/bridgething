package com.bridgething

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * Re-arms companion-device presence observation after a reboot. The OS clears
 * observation requests on boot, so without this the Car Thing can't wake the
 * app from cold until the user opens it once. Associations survive the reboot;
 * only the observation request needs re-registering.
 */
public class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_BOOT_COMPLETED) return
        CompanionDevicePicker.startObservingPresence(context.applicationContext)
    }
}
