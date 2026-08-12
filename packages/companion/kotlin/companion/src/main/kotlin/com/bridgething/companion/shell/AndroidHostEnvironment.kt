package com.bridgething.companion.shell

import java.util.Date
import java.util.Locale
import java.util.TimeZone
import uniffi.bridgething_companion.HostClock
import uniffi.bridgething_companion.HostEnvironment

public class AndroidHostEnvironment : HostEnvironment {
    override fun clock(): HostClock {
        val nowMs = System.currentTimeMillis()
        val tz = TimeZone.getDefault()
        val dstMs = if (tz.inDaylightTime(Date(nowMs))) tz.dstSavings else 0
        return HostClock(
            tzIana = tz.id,
            locale = Locale.getDefault().toLanguageTag(),
            unixSeconds = (nowMs / 1000L).coerceAtLeast(0L).toULong(),
            utcOffsetMinutes = (tz.rawOffset / 60000).toShort(),
            dstOffsetMinutes = (dstMs / 60000).toByte(),
        )
    }
}
