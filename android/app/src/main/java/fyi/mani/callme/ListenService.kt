package fyi.mani.callme

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import kotlin.concurrent.thread

/** Holds a streaming connection to the ntfy topic and rings when Claude calls. */
class ListenService : Service() {
    @Volatile private var running = false
    private var worker: Thread? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel(CHANNEL, "Waiting for calls", NotificationManager.IMPORTANCE_MIN))
        val open = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
        val n = Notification.Builder(this, CHANNEL)
            .setSmallIcon(android.R.drawable.sym_action_call)
            .setContentTitle("Ready for Claude's calls")
            .setContentIntent(open)
            .setOngoing(true)
            .build()
        if (Build.VERSION.SDK_INT >= 34) startForeground(1, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
        else startForeground(1, n)

        if (!running) {
            running = true
            worker = thread(name = "ntfy") { loop(Prefs.topic(this)) }
        }
        return START_STICKY
    }

    private fun loop(topic: String) {
        var lastId: String? = null
        var backoff = 2_000L
        while (running) {
            try {
                // After a drop, catch up on anything missed (the age check below skips stale calls).
                val since = "?since=" + (lastId ?: "75s")
                val conn = URL("https://ntfy.sh/$topic/json$since").openConnection() as HttpURLConnection
                conn.connectTimeout = 15_000
                conn.readTimeout = 60_000 // ntfy sends a keepalive every ~45s; silence longer than this = dead link
                conn.inputStream.bufferedReader().useLines { lines ->
                    backoff = 2_000L
                    for (line in lines) {
                        if (!running) break
                        val msg = JSONObject(line)
                        if (msg.optString("event") != "message") continue
                        lastId = msg.optString("id")
                        val ageMs = System.currentTimeMillis() - msg.optLong("time") * 1000
                        val link = msg.optString("click")
                        if (msg.optString("title") == "Contact mode") {
                            ContactMode.received(applicationContext, msg.optString("message").trim(), msg.optLong("time") * 1000)
                            continue
                        }
                        val isCall = msg.optString("title") == "Claude is calling"
                        // Claude keeps a call open ~75s; anything older has already given up.
                        if (isCall && ageMs < 75_000 && link.startsWith("https://")) {
                            Ringer.ring(applicationContext, link, msg.optString("message"))
                        }
                    }
                }
            } catch (e: Exception) {
                if (!running) break
                Thread.sleep(backoff)
                backoff = (backoff * 2).coerceAtMost(60_000L)
            }
        }
    }

    override fun onDestroy() {
        running = false
        worker?.interrupt()
        super.onDestroy()
    }

    companion object {
        private const val CHANNEL = "listening"
        fun start(context: Context) = context.startForegroundService(Intent(context, ListenService::class.java))
    }
}
