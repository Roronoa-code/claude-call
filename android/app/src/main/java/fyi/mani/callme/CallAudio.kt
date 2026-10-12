package fyi.mani.callme

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.media.MediaPlayer
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import android.webkit.JavascriptInterface
import android.webkit.WebView

/**
 * Plays Claude's voice like a phone call: through the earpiece by default (speaker on request),
 * with the screen switching off when the phone is held to the ear.
 * Exposed to the call page as `window.CallMeApp`.
 */
class CallAudio(context: Context, private val web: WebView, private val origin: String, private val onEnd: () -> Unit) {
    private val main = Handler(Looper.getMainLooper())
    private val audio = context.getSystemService(AudioManager::class.java)
    private val proximity = context.getSystemService(PowerManager::class.java)
        .newWakeLock(PowerManager.PROXIMITY_SCREEN_OFF_WAKE_LOCK, "callme:ear")
    private var player: MediaPlayer? = null
    private var speaker = false
    private var started = false

    private fun start() {
        if (started) return
        started = true
        audio.mode = AudioManager.MODE_IN_COMMUNICATION
        route()
    }

    private fun route() {
        val want = if (speaker) AudioDeviceInfo.TYPE_BUILTIN_SPEAKER else AudioDeviceInfo.TYPE_BUILTIN_EARPIECE
        audio.availableCommunicationDevices.firstOrNull { it.type == want }?.let { audio.setCommunicationDevice(it) }
        if (speaker) { if (proximity.isHeld) proximity.release() } else if (!proximity.isHeld) proximity.acquire()
    }

    @JavascriptInterface fun isSpeaker() = speaker

    /** The call is over: close the call screen so the mic and earpiece are released. */
    @JavascriptInterface fun ended() { main.post(onEnd) }

    @JavascriptInterface fun setSpeaker(on: Boolean) = main.post { speaker = on; if (started) route() }

    /** Plays one of Claude's lines; calls window.__played(id, ok) when finished. */
    @JavascriptInterface fun play(url: String, id: Int) = main.post {
        start()
        player?.release()
        if (!url.startsWith(origin)) { done(id, false); return@post }
        player = MediaPlayer().apply {
            setAudioAttributes(AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
                .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
            setOnCompletionListener { done(id, true) }
            setOnErrorListener { _, _, _ -> done(id, false); true }
            setOnPreparedListener { it.start() }
            try { setDataSource(url); prepareAsync() } catch (e: Exception) { done(id, false) }
        }
    }

    /** The user started talking over Claude: stop the current line straight away. */
    @JavascriptInterface fun stop() = main.post { player?.release(); player = null }

    private fun done(id: Int, ok: Boolean) = main.post { web.evaluateJavascript("window.__played && window.__played($id, $ok)", null) }

    fun release() {
        player?.release(); player = null
        if (proximity.isHeld) proximity.release()
        if (started) { audio.clearCommunicationDevice(); audio.mode = AudioManager.MODE_NORMAL }
    }
}
