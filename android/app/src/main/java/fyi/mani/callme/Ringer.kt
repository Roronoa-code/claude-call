package fyi.mani.callme

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Person
import android.content.Context
import android.content.Intent
import android.media.AudioAttributes
import android.media.Ringtone
import android.media.RingtoneManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager

/** Incoming-call ringing: full-screen call screen, looping ringtone and vibration. */
object Ringer {
    private const val CHANNEL = "calls"
    private const val NOTIFICATION_ID = 2
    private const val RING_FOR_MS = 60_000L

    private val main = Handler(Looper.getMainLooper())
    private var ringtone: Ringtone? = null
    // Must cancel on the same Vibrator that started it: Android ties vibrations to the caller's instance.
    private var vibrator: Vibrator? = null
    private var currentLink: String? = null
    private val handled = mutableSetOf<String>() // answered or declined; Claude's re-rings for these are ignored

    fun ring(context: Context, link: String, reason: String) = main.post {
        if (link in handled || link == currentLink) return@post
        stopSound(context)
        currentLink = link

        val nm = context.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel(CHANNEL, "Calls from Claude", NotificationManager.IMPORTANCE_HIGH).apply {
            setSound(null, null) // we play the ringtone ourselves so it loops
        })
        val fullScreen = PendingIntent.getActivity(context, 1, CallActivity.intent(context, link, answer = false),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val answer = PendingIntent.getActivity(context, 2, CallActivity.intent(context, link, answer = true),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val decline = PendingIntent.getBroadcast(context, 3,
            Intent(context, DeclineReceiver::class.java).putExtra(DeclineReceiver.EXTRA_LINK, link),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)

        val builder = Notification.Builder(context, CHANNEL)
            .setSmallIcon(android.R.drawable.sym_action_call)
            .setContentTitle("Claude is calling")
            .setContentText(reason)
            .setCategory(Notification.CATEGORY_CALL)
            .setFullScreenIntent(fullScreen, true)
            .setContentIntent(answer) // tapping the alert answers
            .setOngoing(true)
            .setTimeoutAfter(RING_FOR_MS)
        if (Build.VERSION.SDK_INT >= 31) {
            builder.setStyle(Notification.CallStyle.forIncomingCall(
                Person.Builder().setName("Claude").setImportant(true).build(), decline, answer))
        } else {
            builder.addAction(Notification.Action.Builder(null, "Decline", decline).build())
            builder.addAction(Notification.Action.Builder(null, "Answer", answer).build())
        }
        nm.notify(NOTIFICATION_ID, builder.build())

        ringtone = RingtoneManager.getRingtone(context, RingtoneManager.getDefaultUri(RingtoneManager.TYPE_RINGTONE))?.apply {
            audioAttributes = AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_NOTIFICATION_RINGTONE)
                .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION).build()
            if (Build.VERSION.SDK_INT >= 28) isLooping = true
            play()
        }
        vibrator = context.applicationContext.getSystemService(VibratorManager::class.java)?.defaultVibrator?.also {
            it.vibrate(VibrationEffect.createWaveform(longArrayOf(0, 900, 700), 0))
        }

        // Missed: go quiet, but Claude's next re-ring for this call may ring again.
        main.postDelayed({ if (currentLink == link) stop(context, handledCall = false) }, RING_FOR_MS)
    }

    /** Stops ringing; an answered/declined call is remembered so its re-rings stay quiet. */
    fun stop(context: Context, handledCall: Boolean = true) {
        if (handledCall) currentLink?.let { handled += it }
        currentLink = null
        stopSound(context)
        context.getSystemService(NotificationManager::class.java).cancel(NOTIFICATION_ID)
    }

    private fun stopSound(context: Context) {
        ringtone?.stop(); ringtone = null
        vibrator?.cancel(); vibrator = null
    }
}
