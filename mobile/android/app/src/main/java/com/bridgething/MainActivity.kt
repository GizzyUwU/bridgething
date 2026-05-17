package com.bridgething

import android.content.Intent
import com.facebook.react.ReactActivity
import com.facebook.react.ReactActivityDelegate
import com.facebook.react.defaults.DefaultNewArchitectureEntryPoint.fabricEnabled
import com.facebook.react.defaults.DefaultReactActivityDelegate

class MainActivity : ReactActivity() {

  override fun getMainComponentName(): String = "bridgething"

  override fun createReactActivityDelegate(): ReactActivityDelegate =
      DefaultReactActivityDelegate(this, mainComponentName, fabricEnabled)

  // Route activity results to one-shot handlers registered via
  // BridgethingActivityRegistry (CompanionDeviceManager picker, etc).
  override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
    if (!BridgethingActivityRegistry.deliverResult(requestCode, resultCode, data)) {
      super.onActivityResult(requestCode, resultCode, data)
    }
  }
}
