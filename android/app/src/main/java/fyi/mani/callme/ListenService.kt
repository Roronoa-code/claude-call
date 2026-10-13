package fyi.mani.callme

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Person
import android.app.Service
import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.content.pm.PackageManager
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
        val link = activeCallLink
        val pendingFlags = PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        val n = if (link == null) Notification.Builder(this, CHANNEL)
            .setSmallIcon(android.R.drawable.sym_action_call)
            .setContentTitle("Ready for Claude's calls")
            .setContentIntent(PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), pendingFlags))
            .setOngoing(true)
            .build()
        else {
            nm.createNotificationChannel(NotificationChannel(CALL_CHANNEL, "Ongoing Claude call", NotificationManager.IMPORTANCE_LOW))
            val open = PendingIntent.getActivity(this, 4, CallActivity.intent(this, link, answer = true), pendingFlags)
            val end = PendingIntent.getBroadcast(this, 5,
                Intent(this, DeclineReceiver::class.java).putExtra(DeclineReceiver.EXTRA_LINK, link), pendingFlags)
            val builder = Notification.Builder(this, CALL_CHANNEL)
                .setSmallIcon(android.R.drawable.sym_action_call)
                .setContentTitle("On call with Claude")
                .setContentText("Tap to return to the call")
                .setContentIntent(open)
                .setCategory(Notification.CATEGORY_CALL)
                .setOngoing(true)
                .setOnlyAlertOnce(true)
                .setWhen(activeCallStartedAt)
                .setShowWhen(true)
                .setUsesChronometer(true)
            if (Build.VERSION.SDK_INT >= 31) builder.setStyle(Notification.CallStyle.forOngoingCall(
                Person.Builder().setName("Claude").setImportant(true).build(), end))
            else builder.addAction(Notification.Action.Builder(null, "End call", end).build())
            val supportsPromotion = Build.VERSION.SDK_INT >= 36 && Build.VERSION.SDK_INT_FULL >= Build.VERSION_CODES_FULL.BAKLAVA_1
            if (supportsPromotion) {
                builder.setRequestPromotedOngoing(true)
            }
            val call = builder.build()
            if (supportsPromotion && !call.hasPromotableCharacteristics()) {
                // Some Samsung builds exclude CallStyle from their supported Live Update styles.
                builder.setStyle(Notification.BigTextStyle().bigText("Tap to return to the call"))
                    .setContentTitle("On call with Claude")
                    .setActions(Notification.Action.Builder(null, "End call", end).build())
                    .build()
            } else call
        }
        var types = if (Build.VERSION.SDK_INT >= 34) ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE else 0
        // Only an answered call requests microphone access; setup and boot remain listener-only.
        if (link != null && Build.VERSION.SDK_INT >= 30 &&
            checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) {
            types = types or ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
        }
        startForeground(1, n, types)

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
        private const val CALL_CHANNEL = "ongoing_calls"
        var activeCallLink: String? = null
            private set
        private var activeCallStartedAt = 0L
        fun start(context: Context) = context.startForegroundService(Intent(context, ListenService::class.java))
        fun callStarted(context: Context, link: String) {
            if (activeCallLink != link) activeCallStartedAt = System.currentTimeMillis()
            activeCallLink = link
            start(context)
        }
        fun callEnded(context: Context, link: String) {
            if (activeCallLink != link) return
            activeCallLink = null
            activeCallStartedAt = 0L
            start(context)
        }
    }
}
