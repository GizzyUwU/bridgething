package com.bridgething.companion

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import androidx.core.content.ContextCompat
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.bridgething.companion.shell.AndroidNotificationBackend
import java.util.concurrent.CountDownLatch
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.bridgething_companion.ActionSink
import uniffi.bridgething_companion.NoHandle
import uniffi.bridgething_companion.NotificationActionError

private class RecordingActionSink : ActionSink(NoHandle) {
    val outcomes = LinkedBlockingQueue<Any>()

    override fun complete() {
        outcomes.add("ok")
    }

    override fun fail(error: NotificationActionError) {
        outcomes.add(error)
    }
}

@RunWith(AndroidJUnit4::class)
class AndroidNotificationBackendTest {
    private val context: Context
        get() = InstrumentationRegistry.getInstrumentation().targetContext

    @Test
    fun invokePositiveFiresResolvedPendingIntent() {
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
            val sink = RecordingActionSink()
            backend.invokePositive("n-1", sink)
            assertTrue("the resolved action PendingIntent should fire", latch.await(5, TimeUnit.SECONDS))
            assertTrue("the sink completes", sink.outcomes.poll(5, TimeUnit.SECONDS) == "ok")
        } finally {
            context.unregisterReceiver(receiver)
        }
    }

    @Test
    fun invokeWithNoResolvedActionFailsTheSink() {
        val backend = AndroidNotificationBackend(resolveAction = { _, _ -> null })
        val positive = RecordingActionSink()
        backend.invokePositive("missing", positive)
        assertNotNull("a missing action answers the sink", positive.outcomes.poll(5, TimeUnit.SECONDS))
        val negative = RecordingActionSink()
        backend.invokeNegative("missing", negative)
        assertNotNull("a missing action answers the sink", negative.outcomes.poll(5, TimeUnit.SECONDS))
    }

    private companion object {
        const val ACTION = "com.bridgething.test.NOTIF_ACTION"
    }
}
