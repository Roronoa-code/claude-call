package fyi.mani.callme

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Color
import android.os.Bundle
import android.view.WindowInsets
import android.view.WindowManager
import android.webkit.PermissionRequest
import android.webkit.WebChromeClient
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient

/** The call screen: shows Claude's call page; the mic is granted by the app, so the page never asks. */
class CallActivity : Activity() {
    private lateinit var web: WebView
    private var callAudio: CallAudio? = null
    private var retries = 0

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        web = WebView(this).apply {
            setBackgroundColor(Color.parseColor("#111214"))
            settings.javaScriptEnabled = true
            settings.domStorageEnabled = true
            settings.mediaPlaybackRequiresUserGesture = false
            webChromeClient = object : WebChromeClient() {
                override fun onPermissionRequest(request: PermissionRequest) {
                    val micOk = checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
                    if (micOk && PermissionRequest.RESOURCE_AUDIO_CAPTURE in request.resources) {
                        request.grant(arrayOf(PermissionRequest.RESOURCE_AUDIO_CAPTURE))
                    } else request.deny()
                }
            }
            // A brand-new tunnel address can take a few seconds to resolve on the phone: keep retrying.
            webViewClient = object : WebViewClient() {
                override fun onReceivedError(view: WebView, request: WebResourceRequest, error: WebResourceError) {
                    if (request.isForMainFrame && retries++ < 30) {
                        view.loadData(RETRY_PAGE, "text/html", "utf-8")
                        view.postDelayed({ view.loadUrl(request.url.toString()) }, 1500)
                    }
                }
            }
            // Keep the page clear of the status and navigation bars.
            setOnApplyWindowInsetsListener { v, insets ->
                val bars = insets.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.ime())
                v.setPadding(bars.left, bars.top, bars.right, bars.bottom)
                insets
            }
        }
        setContentView(web)
        load(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        load(intent)
    }

    private fun load(intent: Intent) {
        val link = intent.getStringExtra(EXTRA_LINK) ?: return finish()
        val answer = intent.getBooleanExtra(EXTRA_ANSWER, false)
        // Opened by "Answer": stop now. Opened by the ringing itself (locked phone): keep ringing until touched.
        if (answer) Ringer.stop(this)
        if (link == TEST_LINK) {
            web.loadData(TEST_PAGE, "text/html", "utf-8")
            return
        }
        callAudio?.let { web.removeJavascriptInterface("CallMeApp"); it.release() }
        callAudio = CallAudio(this, web, link.substringBefore("/?")) { finish() }.also { web.addJavascriptInterface(it, "CallMeApp") }
        retries = 0
        web.loadUrl(if (answer) "$link&auto=1" else link)
    }

    // Any touch on the call screen, or leaving it, silences the ringing.
    override fun onUserInteraction() {
        super.onUserInteraction()
        Ringer.stop(this)
    }

    // Leaving the call screen hangs up.
    override fun onDestroy() {
        Ringer.stop(this)
        intent.getStringExtra(EXTRA_LINK)?.let { DeclineReceiver.hangUp(it) }
        callAudio?.release()
        web.destroy()
        super.onDestroy()
    }

    companion object {
        private const val EXTRA_LINK = "link"
        private const val EXTRA_ANSWER = "answer"
        const val TEST_LINK = "test:ring"
        private const val RETRY_PAGE = "<body style='margin:0;height:100vh;display:grid;place-items:center;" +
            "background:#111214;color:#9a9ca3;font:18px system-ui'>Connecting…</body>"
        private const val TEST_PAGE = "<body style='margin:0;height:100vh;display:grid;place-items:center;" +
            "background:#111214;color:#f2f2f3;font:20px system-ui;text-align:center'>" +
            "<div>✓ Test call worked<br><small style='color:#9a9ca3'>Press back to close</small></div></body>"
        fun intent(context: Context, link: String, answer: Boolean) =
            Intent(context, CallActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                .putExtra(EXTRA_LINK, link)
                .putExtra(EXTRA_ANSWER, answer)
    }
}
