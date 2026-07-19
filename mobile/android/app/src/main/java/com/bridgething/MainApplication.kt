package com.bridgething

import android.app.Application
import com.bridgething.gateway.LogStore
import com.bridgething.gateway.LogcatCapture
import com.facebook.react.PackageList
import com.facebook.react.ReactApplication
import com.facebook.react.ReactHost
import com.facebook.react.ReactNativeApplicationEntryPoint.loadReactNative
import com.facebook.react.defaults.DefaultReactHost.getDefaultReactHost

class MainApplication : Application(), ReactApplication {

  override val reactHost: ReactHost by lazy {
    getDefaultReactHost(
      context = applicationContext,
      packageList =
        PackageList(this).packages.apply {
          // Packages that cannot be autolinked yet can be added manually here, for example:
          // add(MyReactNativePackage())
        },
    )
  }

  override fun onCreate() {
    super.onCreate()
    // persistent logging first: logcat replays this process's existing buffer on
    // attach, so anything logged between zygote fork and here is still captured.
    LogStore.install(this)
    LogcatCapture.start()
    // install the bridgething session backend before react native starts so
    // the JS proxy never sees a "backend not installed" throw on first bridge call.
    BridgethingActivityRegistry.installCallbacks(this)
    BridgethingApp.installBridgething(this)
    loadReactNative(this)
  }
}
