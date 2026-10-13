package fyi.mani.callme

import android.app.NativeActivity
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.widget.Toast
import android.window.OnBackInvokedDispatcher
import java.lang.ref.WeakReference

/**
 * The call screen. Drawn and run by the Rust library (libcallme_phone.so, built from phone/ in the repo);
 * this class only keeps the Android-only parts: ringing, the ongoing-call notification, and hanging up when
 * the screen closes.
 */
class CallActivity : NativeActivity() {
    private var active = false
    private val link get() = intent.getStringExtra(EXTRA_LINK)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        current = WeakReference(this)
        if (link == TEST_LINK) {
            Toast.makeText(this, "✓ Test call worked", Toast.LENGTH_LONG).show()
            finish()
            return
        }
        // Opened by "Answer": stop now. Opened by the ringing itself (locked phone): keep ringing until touched.
        if (intent.getBooleanExtra(EXTRA_ANSWER, false)) Ringer.stop(this)
        if (Build.VERSION.SDK_INT >= 33) {
            onBackInvokedDispatcher.registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT) { leaveCall() }
        }
    }

    // "Return to call" while it is open: just come back to it.
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        if (intent.getBooleanExtra(EXTRA_ANSWER, false)) Ringer.stop(this)
    }

    /** Called from Rust once the call is answered. */
    @Suppress("unused")
    fun callBegan() = runOnUiThread {
        Ringer.stop(this)
        active = true
        link?.let { ListenService.callStarted(this, it) }
    }

    // Any touch on the call screen silences the ringing.
    override fun onUserInteraction() {
        super.onUserInteraction()
        Ringer.stop(this)
    }

    // Back and Home keep a live call going (the notification brings it back); before answering, Back closes it.
    private fun leaveCall() {
        if (active) moveTaskToBack(true) else finish()
    }

    @Deprecated("Used on Android 12 and earlier")
    override fun onBackPressed() = leaveCall()

    // Finishing the call screen hangs up.
    override fun onDestroy() {
        Ringer.stop(this)
        link?.takeIf { it != TEST_LINK }?.let {
            DeclineReceiver.hangUp(it)
            ListenService.callEnded(this, it)
        }
        if (current?.get() === this) current = null
        super.onDestroy()
    }

    companion object {
        private var current: WeakReference<CallActivity>? = null
        fun finishCall(link: String): Boolean {
            val activity = current?.get()?.takeIf { it.link == link && !it.isFinishing } ?: return false
            activity.finish()
            return true
        }
        private const val EXTRA_LINK = "link"
        private const val EXTRA_ANSWER = "answer"
        const val TEST_LINK = "test:ring"
        fun intent(context: Context, link: String, answer: Boolean) =
            Intent(context, CallActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                .putExtra(EXTRA_LINK, link)
                .putExtra(EXTRA_ANSWER, answer)
    }
}
