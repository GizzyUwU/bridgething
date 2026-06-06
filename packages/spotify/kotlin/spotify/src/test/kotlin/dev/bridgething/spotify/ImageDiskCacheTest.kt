package dev.bridgething.spotify

import org.junit.jupiter.api.Assertions.assertArrayEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File

class ImageDiskCacheTest {
    @Test
    fun roundTripsAndMissesNull(@TempDir dir: File) {
        val cache = ImageDiskCache(File(dir, "art"), 1024)
        cache.put("spotify/img/248/a", byteArrayOf(1, 2, 3))
        assertArrayEquals(byteArrayOf(1, 2, 3), cache.get("spotify/img/248/a"))
        assertNull(cache.get("spotify/img/248/missing"))
    }

    @Test
    fun evictsLeastRecentlyAccessedOverCap(@TempDir dir: File) {
        val cache = ImageDiskCache(File(dir, "art"), 10)
        cache.put("a", ByteArray(6))
        cache.put("b", ByteArray(3))
        // touch a so b becomes the least-recently-accessed entry
        cache.get("a")
        // total would be 12 > 10, so the lru (b) is evicted
        cache.put("c", ByteArray(3))
        assertArrayEquals(ByteArray(6), cache.get("a"))
        assertNull(cache.get("b"))
        assertArrayEquals(ByteArray(3), cache.get("c"))
    }

    @Test
    fun seedsFromDiskOnReopen(@TempDir dir: File) {
        val artDir = File(dir, "art")
        ImageDiskCache(artDir, 1024).put("k", ByteArray(5))
        assertArrayEquals(ByteArray(5), ImageDiskCache(artDir, 1024).get("k"))
    }
}
