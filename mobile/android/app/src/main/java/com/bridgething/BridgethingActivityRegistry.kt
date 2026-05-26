package com.bridgething

import android.app.Activity
import android.app.Application
import android.content.Intent
import android.os.Bundle
import java.util.concurrent.atomic.AtomicReference

/**
 * Process-wide pointer to the currently-resumed [Activity]. Nitro modules
 * read from it when they need to launch an OS picker; CompanionDeviceManager
 * requires `startIntentSenderForResult`, which is an Activity-only API.
 *
 * Also routes `onActivityResult` back to one-shot handlers keyed by request
 * code, since `ReactActivity` doesn't expose pluggable per-request-code dispatch.
 */
public object BridgethingActivityRegistry {
    private val currentRef = AtomicReference<Activity?>(null)

    public val currentActivity: Activity?
        get() = currentRef.get()

    private val handlers = mutableMapOf<Int, (Int, Intent?) -> Unit>()

    public fun installCallbacks(application: Application) {
        application.registerActivityLifecycleCallbacks(object : Application.ActivityLifecycleCallbacks {
            override fun onActivityCreated(activity: Activity, bundle: Bundle?) {}
            override fun onActivityStarted(activity: Activity) {}
            override fun onActivityResumed(activity: Activity) { currentRef.set(activity) }
            override fun onActivityPaused(activity: Activity) {
                currentRef.compareAndSet(activity, null)
            }
            override fun onActivityStopped(activity: Activity) {}
            override fun onActivitySaveInstanceState(activity: Activity, outState: Bundle) {}
            override fun onActivityDestroyed(activity: Activity) {
                currentRef.compareAndSet(activity, null)
            }
        })
    }

    /**
     * Register a one-shot result handler for [requestCode]. The handler
     * is removed after firing. Caller is expected to have already
     * launched the corresponding intent via [currentActivity].
     */
    public fun expectResult(requestCode: Int, handler: (resultCode: Int, data: Intent?) -> Unit) {
        synchronized(handlers) { handlers[requestCode] = handler }
    }

    public fun deliverResult(requestCode: Int, resultCode: Int, data: Intent?): Boolean {
        val handler = synchronized(handlers) { handlers.remove(requestCode) } ?: return false
        handler(resultCode, data)
        return true
    }
}
