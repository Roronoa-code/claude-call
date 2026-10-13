package fyi.mani.callme

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioDeviceInfo
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioTrack
import android.media.MediaPlayer
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import android.os.Build
import android.view.WindowInsets
import android.view.inputmethod.InputMethodManager
import android.webkit.JavascriptInterface
import android.webkit.WebView
import java.net.HttpURLConnection
import java.net.URL

/**
 * Plays Claude's voice like a phone call: through the earpiece by default (speaker on request),
 * with the screen switching off when the phone is held to the ear.
 * Exposed to the call page as `window.CallMeApp`.
 */
class CallAudio(context: Context, private val web: WebView, private val origin: String, private val onBegin: () -> Unit, private val onEnd: () -> Unit) {
    private val main = Handler(Looper.getMainLooper())
    private val executor = context.mainExecutor
    private val audio = context.getSystemService(AudioManager::class.java)
    private val proximity = context.getSystemService(PowerManager::class.java)
        .newWakeLock(PowerManager.PROXIMITY_SCREEN_OFF_WAKE_LOCK, "callme:ear")
    private val attrs = AudioAttributes.Builder()
        .setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
        .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build()
    private var player: MediaPlayer? = null
    @Volatile private var voice: Voice? = null
    @Volatile private var speaker = false
    private var started = false
    val isActive get() = started

    // The WebView's microphone switches the call to the loudspeaker when it opens; put our choice back.
    private val routeWatch = AudioManager.OnCommunicationDeviceChangedListener { d ->
        // Also when the system cleared the device (null): that is how the earpiece quietly became the speaker once.
        val builtIn = d == null || d.type == AudioDeviceInfo.TYPE_BUILTIN_SPEAKER || d.type == AudioDeviceInfo.TYPE_BUILTIN_EARPIECE
        if (started && builtIn && d?.type != wanted()) main.post { route() }
    }

    private fun wanted() = if (speaker) AudioDeviceInfo.TYPE_BUILTIN_SPEAKER else AudioDeviceInfo.TYPE_BUILTIN_EARPIECE

    private fun start() {
        if (started) return
        started = true
        onBegin()
        audio.mode = AudioManager.MODE_IN_COMMUNICATION
        audio.addOnCommunicationDeviceChangedListener(executor, routeWatch)
        route()
    }

    private fun route() {
        if (audio.mode != AudioManager.MODE_IN_COMMUNICATION) audio.mode = AudioManager.MODE_IN_COMMUNICATION // the system may reset it
        audio.availableCommunicationDevices.firstOrNull { it.type == wanted() }?.let { audio.setCommunicationDevice(it) }
        if (speaker) { if (proximity.isHeld) proximity.release() } else if (!proximity.isHeld) proximity.acquire()
    }

    /** The call was answered: switch to call audio (earpiece) now, before Claude's first word. */
    @JavascriptInterface fun begin() = main.post { start() }

    @JavascriptInterface fun hideKeyboard() = main.post {
        if (Build.VERSION.SDK_INT >= 30) web.windowInsetsController?.hide(WindowInsets.Type.ime())
        else web.context.getSystemService(InputMethodManager::class.java)?.hideSoftInputFromWindow(web.windowToken, 0)
    }

    @JavascriptInterface fun isSpeaker() = speaker

    /** Where call audio really goes right now (for the debug log). */
    @JavascriptInterface fun device(): String = when (audio.communicationDevice?.type) {
        AudioDeviceInfo.TYPE_BUILTIN_SPEAKER -> "speaker"
        AudioDeviceInfo.TYPE_BUILTIN_EARPIECE -> "earpiece"
        null -> "none"
        else -> "other"
    }

    /** The call is over: close the call screen so the mic and earpiece are released. */
    @JavascriptInterface fun ended() { main.post(onEnd) }

    @JavascriptInterface fun setSpeaker(on: Boolean) = main.post { speaker = on; if (started) route() }

    /** Plays one of Claude's lines (a whole file); calls window.__played(id, ok) when finished. */
    @JavascriptInterface fun play(url: String, id: Int) = main.post {
        start()
        player?.release()
        if (!url.startsWith(origin)) { done(id, false); return@post }
        player = MediaPlayer().apply {
            setAudioAttributes(attrs)
            setOnCompletionListener { done(id, true) }
            setOnErrorListener { _, _, _ -> done(id, false); true }
            setOnPreparedListener { it.start(); js("window.__audioStart && window.__audioStart($id)") }
            try { setDataSource(url); prepareAsync() } catch (e: Exception) { done(id, false) }
        }
    }

    /**
     * Plays one of Claude's lines while it is still being made: 16-bit mono PCM fetched in rolling pieces
     * (`&from=<bytes so far>`, the last one marked x-done: 1) and played as each arrives.
     * Tells the page when the sound starts (__audioStart), how long it is once fully received (__audioDur),
     * and when it has finished playing (__played).
     */
    @JavascriptInterface fun stream(url: String, id: Int) {
        voice?.cancel()
        if (!url.startsWith(origin)) { done(id, false); return }
        main.post { start() }
        voice = Voice(url, id).also { it.start() }
    }

    /** The user started talking over Claude: silence the current line at once (called on the page's thread, not queued). */
    @JavascriptInterface fun stop() {
        voice?.cancel(); voice = null
        main.post { player?.release(); player = null }
    }

    private inner class Voice(private val url: String, private val id: Int) : Thread("callme-voice") {
        @Volatile private var cancelled = false
        @Volatile private var track: AudioTrack? = null
        @Volatile private var conn: HttpURLConnection? = null
        @Volatile private var finished = false
        private var rate = 24_000
        // Where playback really is, every 200 ms (__audioPos), so captions follow the voice word by word.
        private val tick = object : Runnable {
            override fun run() {
                val t = track ?: return
                if (cancelled || finished) return
                try { web.evaluateJavascript("window.__audioPos && window.__audioPos($id, ${t.playbackHeadPosition.toLong() * 1000 / rate})", null) } catch (_: Exception) { return }
                main.postDelayed(this, 200)
            }
        }

        fun cancel() {
            cancelled = true
            try { track?.pause(); track?.flush() } catch (_: Exception) {}
            try { conn?.disconnect() } catch (_: Exception) {}
        }

        override fun run() {
            var ok = false
            try {
                val buf = ByteArray(8192)
                var have = 0 // a leftover odd byte from the previous read
                var bytes = 0L
                var from = 0L
                var t: AudioTrack? = null
                while (!cancelled) {
                    val c = (URL("$url&from=$from").openConnection() as HttpURLConnection).apply { connectTimeout = 10_000; readTimeout = 30_000 }
                    conn = c
                    if (c.responseCode != 200) { if (from == 0L) return else break } // later 404 = line replaced
                    val last = c.getHeaderField("x-done") == "1"
                    val tr = t ?: run {
                        rate = c.getHeaderField("x-sample-rate")?.toIntOrNull() ?: 24_000
                        val min = AudioTrack.getMinBufferSize(rate, AudioFormat.CHANNEL_OUT_MONO, AudioFormat.ENCODING_PCM_16BIT)
                        AudioTrack.Builder()
                            .setAudioAttributes(attrs)
                            .setAudioFormat(AudioFormat.Builder().setSampleRate(rate)
                                .setChannelMask(AudioFormat.CHANNEL_OUT_MONO).setEncoding(AudioFormat.ENCODING_PCM_16BIT).build())
                            .setTransferMode(AudioTrack.MODE_STREAM)
                            .setBufferSizeInBytes(maxOf(min, rate / 5 * 2)) // ~200 ms: small, so stopping is instant
                            .build().also { t = it; track = it }
                    }
                    c.inputStream.use { input ->
                        while (!cancelled) {
                            val n = input.read(buf, have, buf.size - have)
                            if (n < 0) break
                            from += n
                            val total = have + n
                            val even = total and 1.inv()
                            if (even > 0) {
                                if (bytes == 0L) { tr.play(); js("window.__audioStart && window.__audioStart($id)"); main.post(tick) }
                                tr.write(buf, 0, even)
                                bytes += even
                            }
                            have = total - even
                            if (have == 1) buf[0] = buf[total - 1]
                        }
                    }
                    if (last) break
                }
                val tr = t
                if (cancelled || tr == null) return
                val frames = bytes / 2
                js("window.__audioDur && window.__audioDur($id, ${frames * 1000 / rate})")
                // stop() resets the playback head, so wait for the tail before stopping.
                val until = System.currentTimeMillis() + 500 + frames * 1000 / rate
                while (!cancelled && tr.playbackHeadPosition < frames && System.currentTimeMillis() < until) sleep(20)
                tr.stop()
                ok = bytes > 0
            } catch (_: Exception) {
            } finally {
                finished = true
                try { track?.release() } catch (_: Exception) {}
                if (!cancelled) done(id, ok)
            }
        }
    }

    private fun js(code: String) = main.post { web.evaluateJavascript(code, null) }

    private fun done(id: Int, ok: Boolean) = js("window.__played && window.__played($id, $ok)")

    fun release() {
        voice?.cancel(); voice = null
        player?.release(); player = null
        if (proximity.isHeld) proximity.release()
        if (started) {
            audio.removeOnCommunicationDeviceChangedListener(routeWatch)
            audio.clearCommunicationDevice(); audio.mode = AudioManager.MODE_NORMAL
        }
        started = false
    }
}
