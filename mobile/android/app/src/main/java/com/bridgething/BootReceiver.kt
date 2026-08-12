package com.bridgething

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

public class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_BOOT_COMPLETED) return
        CompanionDevicePicker.startObservingPresence(context.applicationContext)
    }
}
