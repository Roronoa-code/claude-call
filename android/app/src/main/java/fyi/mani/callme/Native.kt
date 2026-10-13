package fyi.mani.callme

import android.content.Context

/** The app's logic, in Rust (phone/src, libcallme_phone.so): the call listener, ringing, hanging up, contact mode. */
object Native {
    init { System.loadLibrary("callme_phone") }

    /** Blocks until [stop]: rings for Claude's calls and passes contact-mode changes on. */
    @JvmStatic external fun listen(context: Context, topic: String)
    @JvmStatic external fun stop()
    @JvmStatic external fun ring(context: Context, link: String, reason: String)
    /** [handled]: answered or declined, so Claude's re-rings for this call stay quiet. */
    @JvmStatic external fun stopRinging(context: Context, handled: Boolean)
    @JvmStatic external fun hangUp(link: String)
    @JvmStatic external fun publishMode(topic: String, mode: String)
    /** "mode atMs" or null. Network: call off the main thread. */
    @JvmStatic external fun latestMode(topic: String): String?

    // Called from Rust.
    @JvmStatic fun onMode(context: Context, mode: String, atMs: Long) = ContactMode.received(context, mode, atMs)
}
