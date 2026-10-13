package fyi.mani.callme

import android.content.Context

/** The app's networking, in Rust (phone/src/net.rs, in libcallme_phone.so): the call listener, hanging up, contact mode. */
object Native {
    init { System.loadLibrary("callme_phone") }
    lateinit var app: Context

    /** Blocks until [stop]: rings for Claude's calls and passes contact-mode changes on. */
    @JvmStatic external fun listen(topic: String)
    @JvmStatic external fun stop()
    @JvmStatic external fun hangUp(link: String)
    @JvmStatic external fun publishMode(topic: String, mode: String)
    /** "mode atMs" or null. Network: call off the main thread. */
    @JvmStatic external fun latestMode(topic: String): String?

    // Called from Rust.
    @JvmStatic fun onRing(link: String, reason: String) = Ringer.ring(app, link, reason)
    @JvmStatic fun onMode(mode: String, atMs: Long) = ContactMode.received(app, mode, atMs)
}
