package fyi.mani.callme

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import java.net.HttpURLConnection
import java.net.URL
import kotlin.concurrent.thread

class DeclineReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        Ringer.stop(context)
        intent.getStringExtra(EXTRA_LINK)?.let { hangUp(it, goAsync()) }
    }

    companion object {
        const val EXTRA_LINK = "link"

        /** Tells Claude the call was declined / hung up, so it stops waiting. */
        fun hangUp(link: String, pending: PendingResult? = null) {
            if (!link.startsWith("https://")) { pending?.finish(); return }
            thread {
                try {
                    (URL(link.replace("/?t=", "/decline?t=")).openConnection() as HttpURLConnection).run {
                        requestMethod = "POST"; connectTimeout = 10_000; readTimeout = 10_000
                        responseCode; disconnect()
                    }
                } catch (_: Exception) {
                } finally { pending?.finish() }
            }
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
    fun latest(context: Context): Pair<String, Long>? = try {
        val conn = URL("https://ntfy.sh/${Prefs.topic(context)}/json?poll=1&since=12h").openConnection() as HttpURLConnection
        conn.connectTimeout = 8_000; conn.readTimeout = 8_000
        conn.inputStream.bufferedReader().readLines().mapNotNull { runCatching { org.json.JSONObject(it) }.getOrNull() }
            .lastOrNull { it.optString("title") == "Contact mode" && it.optString("message").trim() in MODES }
            ?.let { it.optString("message").trim() to it.optLong("time") * 1000 }
    } catch (_: Exception) { null }

    fun publish(context: Context, mode: String) {
        val topic = Prefs.topic(context)
        thread {
            try {
                (URL("https://ntfy.sh/$topic").openConnection() as HttpURLConnection).run {
                    requestMethod = "POST"; doOutput = true; connectTimeout = 10_000; readTimeout = 10_000
                    setRequestProperty("Title", "Contact mode")
                    setRequestProperty("Priority", "1")
                    outputStream.use { it.write(mode.toByteArray()) }
                    responseCode; disconnect()
                }
            } catch (_: Exception) { }
        }
    }
}
