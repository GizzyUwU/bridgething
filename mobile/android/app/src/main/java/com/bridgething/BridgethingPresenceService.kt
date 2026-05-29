package com.bridgething

import android.companion.AssociationInfo
import android.companion.CompanionDeviceService
import androidx.annotation.RequiresApi

/**
 * Bound by the system when an associated Car Thing appears in BT range (API
 * 31+). Wakes the connection foreground service from cold so the link comes
 * back without the user opening the app, then lets it go when the device leaves.
 */
@RequiresApi(31)
public class BridgethingPresenceService : CompanionDeviceService() {
    override fun onDeviceAppeared(associationInfo: AssociationInfo) {
        BridgethingConnectionService.start(this)
    }

    override fun onDeviceDisappeared(associationInfo: AssociationInfo) {
        BridgethingConnectionService.stop(this)
    }

    @Deprecated("string overload is how 31-32 deliver presence; 33+ use the AssociationInfo variant")
    override fun onDeviceAppeared(address: String) {
        BridgethingConnectionService.start(this)
    }

    @Deprecated("string overload is how 31-32 deliver presence; 33+ use the AssociationInfo variant")
    override fun onDeviceDisappeared(address: String) {
        BridgethingConnectionService.stop(this)
    }
}
