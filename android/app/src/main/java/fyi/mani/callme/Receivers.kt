package fyi.mani.callme

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

class DeclineReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        Native.stopRinging(context, true)
        intent.getStringExtra(EXTRA_LINK)?.let {
            if (!CallActivity.finishCall(it)) {
                ListenService.callEnded(context, it)
                hangUp(it, goAsync())
            }
        }
    }

    companion object {
        const val EXTRA_LINK = "link"

        /** Tells Claude the call was declined / hung up, so it stops waiting. */
        fun hangUp(link: String, pending: PendingResult? = null) {
            Native.hangUp(link)
            pending?.finish()
        }
    }
}

class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        // Also after an app update, which otherwise leaves the listener stopped until the app is opened.
        if (intent.action == Intent.ACTION_BOOT_COMPLETED || intent.action == Intent.ACTION_MY_PACKAGE_REPLACED) ListenService.start(context)
    }
}

object Prefs {
    private fun prefs(context: Context) = context.getSharedPreferences("callme", Context.MODE_PRIVATE)
    // Default channel comes from res/values/topic.xml (kept out of git: it is effectively a password).
    fun topic(context: Context): String = prefs(context).getString("topic", null) ?: context.getString(R.string.ntfy_topic)
    fun setTopic(context: Context, topic: String) = prefs(context).edit().putString("topic", topic).apply()
    fun mode(context: Context): String = prefs(context).getString("mode", "both")!!
    fun modeChangedAt(context: Context): Long = prefs(context).getLong("modeChangedAt", 0L)
    fun setMode(context: Context, mode: String, at: Long = System.currentTimeMillis()) =
        prefs(context).edit().putString("mode", mode).putLong("modeChangedAt", at).apply()
}

/** Tells the PC agents how the user wants to be reached (read by CallMe and Discord Input). */
object ContactMode {
    val MODES = listOf("call", "text", "both", "none")

    /** A mode set elsewhere (Claude, Discord, the app) arrived from ntfy: store it unless a newer local change exists. */
    fun received(context: Context, mode: String, atMs: Long) {
        if (mode !in MODES || atMs < Prefs.modeChangedAt(context) - 2_000) return
        Prefs.setMode(context, mode, atMs)
        ModeWidget.updateAll(context)
    }

    /** Latest mode set from anywhere (phone, Claude, Discord) in the last 12h with its time (ms), or null. Network: call off the main thread. */
    fun latest(context: Context): Pair<String, Long>? =
        Native.latestMode(Prefs.topic(context))?.split(" ")?.let { (mode, at) -> mode to at.toLong() }

    fun publish(context: Context, mode: String) = Native.publishMode(Prefs.topic(context), mode)
}
