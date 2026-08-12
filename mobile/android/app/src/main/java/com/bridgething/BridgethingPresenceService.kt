package com.bridgething

import android.companion.AssociationInfo
import android.companion.CompanionDeviceService
import androidx.annotation.RequiresApi

@RequiresApi(31)
public class BridgethingPresenceService : CompanionDeviceService() {
    override fun onDeviceAppeared(associationInfo: AssociationInfo) {
        BridgethingConnectionService.start(this)
    }

    override fun onDeviceDisappeared(associationInfo: AssociationInfo) {}

    @Deprecated("string overload is how 31-32 deliver presence; 33+ use the AssociationInfo variant")
    override fun onDeviceAppeared(address: String) {
        BridgethingConnectionService.start(this)
    }

    @Deprecated("string overload is how 31-32 deliver presence; 33+ use the AssociationInfo variant")
    override fun onDeviceDisappeared(address: String) {}
}
