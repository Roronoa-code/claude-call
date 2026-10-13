package fyi.mani.callme

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.view.WindowInsets
import android.view.WindowInsetsAnimation
import android.view.WindowManager
import android.window.OnBackInvokedDispatcher
import android.webkit.PermissionRequest
import android.webkit.WebChromeClient
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import java.lang.ref.WeakReference

/** The call screen: shows Claude's call page; the mic is granted by the app, so the page never asks. */
class CallActivity : Activity() {
    private lateinit var web: WebView
    private var callAudio: CallAudio? = null
    private var retries = 0
    private var loadedLink: String? = null
    private var keyboardHeightCss = 0f
    private var publishedKeyboardHeightCss = -1f

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        window.setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE)
        if (Build.VERSION.SDK_INT >= 30) window.setDecorFitsSystemWindows(false)
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
                override fun onPageFinished(view: WebView, url: String) {
                    publishedKeyboardHeightCss = -1f
                    publishKeyboardHeight()
                }
                override fun onReceivedError(view: WebView, request: WebResourceRequest, error: WebResourceError) {
                    if (request.isForMainFrame && retries++ < 30) {
                        view.loadData(RETRY_PAGE, "text/html", "utf-8")
                        view.postDelayed({ view.loadUrl(request.url.toString()) }, 1500)
                    }
                }
            }
        }
        val frame = FrameLayout(this).apply {
            setBackgroundColor(Color.parseColor("#111214"))
            addView(web, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
            val imeAnimations = mutableSetOf<WindowInsetsAnimation>()
            var finalInsets: WindowInsets? = null
            fun applyInsets(insets: WindowInsets) {
                val bars = insets.getInsets(WindowInsets.Type.systemBars())
                setPadding(bars.left, bars.top, bars.right, bars.bottom)
                val overlap = maxOf(0, insets.getInsets(WindowInsets.Type.ime()).bottom - bars.bottom)
                val scale = web.scale.takeIf { it > 0 } ?: resources.displayMetrics.density
                keyboardHeightCss = overlap / scale
                publishKeyboardHeight()
            }
            // Keep the renderer viewport stable; only the page's foreground follows the keyboard.
            setOnApplyWindowInsetsListener { _, insets ->
                if (Build.VERSION.SDK_INT >= 30) {
                    finalInsets = insets
                    if (imeAnimations.isEmpty()) applyInsets(insets)
                    WindowInsets.CONSUMED
                } else insets // Android 10's decor handles system bars and keyboard resizing.
            }
            if (Build.VERSION.SDK_INT >= 30) {
                setWindowInsetsAnimationCallback(object : WindowInsetsAnimation.Callback(DISPATCH_MODE_STOP) {
                    override fun onPrepare(animation: WindowInsetsAnimation) {
                        if (animation.typeMask and WindowInsets.Type.ime() != 0) imeAnimations += animation
                    }
                    override fun onProgress(insets: WindowInsets, runningAnimations: MutableList<WindowInsetsAnimation>): WindowInsets {
                        applyInsets(insets)
                        return WindowInsets.CONSUMED
                    }
                    override fun onEnd(animation: WindowInsetsAnimation) {
                        imeAnimations -= animation
                        if (imeAnimations.isEmpty()) finalInsets?.let { applyInsets(it) }
                    }
                })
            }
        }
        setContentView(frame)
        current = WeakReference(this)
        if (Build.VERSION.SDK_INT >= 33) {
            onBackInvokedDispatcher.registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT) { leaveCall() }
        }
        load(intent)
    }

    private fun publishKeyboardHeight() {
        if (Build.VERSION.SDK_INT < 30 || keyboardHeightCss == publishedKeyboardHeightCss) return
        publishedKeyboardHeightCss = keyboardHeightCss
        web.evaluateJavascript("document.documentElement.style.setProperty('--native-keyboard-height', '${keyboardHeightCss}px')", null)
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
        if (link == loadedLink && callAudio?.isActive == true) return
        if (link == TEST_LINK) {
            web.loadData(TEST_PAGE, "text/html", "utf-8")
            return
        }
        callAudio?.let { web.removeJavascriptInterface("CallMeApp"); it.release() }
        loadedLink?.let { ListenService.callEnded(this, it) }
        loadedLink = link
        callAudio = CallAudio(this, web, link.substringBefore("/?"),
            onBegin = { ListenService.callStarted(this, link) }, onEnd = { finish() })
            .also { web.addJavascriptInterface(it, "CallMeApp") }
        retries = 0
        web.loadUrl(if (answer) "$link&auto=1" else link)
    }

    // Any touch on the call screen, or leaving it, silences the ringing.
    override fun onUserInteraction() {
        super.onUserInteraction()
        Ringer.stop(this)
    }

    private fun leaveCall() {
        if (callAudio?.isActive == true) moveTaskToBack(true) else finish()
    }

    @Deprecated("Used on Android 12 and earlier")
    override fun onBackPressed() = leaveCall()

    // Finishing the call screen releases the call; Home and Back keep the activity alive.
    override fun onDestroy() {
        Ringer.stop(this)
        intent.getStringExtra(EXTRA_LINK)?.let { DeclineReceiver.hangUp(it) }
        callAudio?.release()
        loadedLink?.let { ListenService.callEnded(this, it) }
        if (current?.get() === this) current = null
        web.destroy()
        super.onDestroy()
    }

    companion object {
        private var current: WeakReference<CallActivity>? = null
        fun finishCall(link: String): Boolean {
            val activity = current?.get()?.takeIf { it.loadedLink == link && !it.isFinishing } ?: return false
            activity.finish()
            return true
        }
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
