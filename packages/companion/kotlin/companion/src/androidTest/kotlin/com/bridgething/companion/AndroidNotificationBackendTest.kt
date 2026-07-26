package com.bridgething.companion

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import androidx.core.content.ContextCompat
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(AndroidJUnit4::class)
class AndroidNotificationBackendTest {
    private val context: Context
        get() = InstrumentationRegistry.getInstrumentation().targetContext

    @Test
    fun invokePositiveFiresResolvedPendingIntent() = runBlocking {
        val latch = CountDownLatch(1)
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(c: Context?, intent: Intent?) = latch.countDown()
        }
        ContextCompat.registerReceiver(context, receiver, IntentFilter(ACTION), ContextCompat.RECEIVER_NOT_EXPORTED)
        try {
            val pending = PendingIntent.getBroadcast(
                context,
                0,
                Intent(ACTION).setPackage(context.packageName),
                PendingIntent.FLAG_IMMUTABLE,
            )
            val backend = AndroidNotificationBackend(
                resolveAction = { id, positive -> if (id == "n-1" && positive) pending else null },
            )
            backend.invokePositive("n-1")
            assertTrue("the resolved action PendingIntent should fire", latch.await(5, TimeUnit.SECONDS))
        } finally {
            context.unregisterReceiver(receiver)
        }
    }

    @Test
    fun invokeWithNoResolvedActionIsNoOp() = runBlocking {
        val backend = AndroidNotificationBackend(resolveAction = { _, _ -> null })
        backend.invokePositive("missing")
        backend.invokeNegative("missing")
    }

    private companion object {
        const val ACTION = "com.bridgething.test.NOTIF_ACTION"
    }
}
