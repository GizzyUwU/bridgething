package com.bridgething.companion.shell

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import java.io.ByteArrayOutputStream
import kotlin.math.roundToInt
import uniffi.bridgething_companion.ImageScaler

internal fun scaleToJpeg(bitmap: Bitmap, maxEdge: Int, quality: Float): ByteArray? = runCatching {
    val edge = maxOf(bitmap.width, bitmap.height)
    val scaled = if (edge <= maxEdge || maxEdge <= 0) {
        bitmap
    } else {
        val scale = maxEdge.toFloat() / edge
        Bitmap.createScaledBitmap(
            bitmap,
            (bitmap.width * scale).toInt().coerceAtLeast(1),
            (bitmap.height * scale).toInt().coerceAtLeast(1),
            true,
        )
    }
    val out = ByteArrayOutputStream()
    val jpegQuality = (quality.coerceIn(0f, 1f) * 100f).roundToInt()
    if (!scaled.compress(Bitmap.CompressFormat.JPEG, jpegQuality, out)) return null
    out.toByteArray()
}.getOrNull()

public class AndroidImageScaler : ImageScaler {
    override fun downsampleJpeg(bytes: ByteArray, maxEdge: UInt, quality: Float): ByteArray? = runCatching {
        val bmp = BitmapFactory.decodeByteArray(bytes, 0, bytes.size) ?: return null
        scaleToJpeg(bmp, maxEdge.toInt(), quality)
    }.getOrNull()
}
