package fyi.mani.callme

import android.app.Activity
import android.app.NativeActivity
import android.app.Service
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.BroadcastReceiver
import android.content.ContentProvider
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.IBinder
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import android.text.Editable
import android.view.Surface
import android.view.View
import android.view.WindowInsets
import android.view.WindowInsetsAnimation
import android.widget.FrameLayout
import android.view.inputmethod.InputMethodManager
import android.view.SurfaceHolder
import android.view.WindowManager
import java.io.File
import java.io.FileNotFoundException
import android.window.BackEvent
import android.window.OnBackAnimationCallback
import android.window.OnBackInvokedDispatcher

/**
 * The whole app is the Rust library (phone/ in the repo, libcallme_phone.so). Android must create these classes
 * itself, so each one only forwards to it (phone/src/shell.rs).
 */
object Native {
    init { System.loadLibrary("callme_phone") }
    @JvmStatic external fun foreground(service: Service)
    @JvmStatic external fun stop()
    @JvmStatic external fun start(context: Context)
    @JvmStatic external fun stopRinging(context: Context)
    @JvmStatic external fun callOpened(activity: Activity, intent: Intent)
    @JvmStatic external fun callBack(activity: Activity)
    @JvmStatic external fun callClosed(activity: Activity)
    @JvmStatic external fun homeOpened(activity: Activity)
    @JvmStatic external fun homeResumed(activity: Activity)
    @JvmStatic external fun homeShown(shown: Boolean)
    @JvmStatic external fun permissionsAnswered(activity: Activity)
    @JvmStatic external fun picked(activity: Activity, data: Intent?)
    @JvmStatic external fun declined(context: Context, intent: Intent)
    @JvmStatic external fun modeTapped(context: Context, intent: Intent)
    @JvmStatic external fun chatAction(context: Context, intent: Intent)
    @JvmStatic external fun widgets(context: Context)
    @JvmStatic external fun back(phase: Int, progress: Float)
    @JvmStatic external fun keyboard(dp: Float)
}

/**
 * Keeps Android's starting window (the app's dark background, themes.xml) on screen until Rust has drawn a frame
 * into the new surface (live_gl.rs). Without it the window counted as drawn while still empty, so a tap on the icon
 * showed the launcher until the GPU's first frame popped the app in.
 */
open class Shown : NativeActivity() {
    // NativeActivity sets "keyboard: unspecified" here over the manifest's, which let Android raise the keyboard by
    // itself on opening whenever Slint's always-focusable input view held the focus as the window got it (more often
    // since the window now gets it only once drawn). Opening never shows it; a field tapped (or one that had it when
    // the app was left) still does.
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_STATE_HIDDEN or WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE)
        followKeyboard()
    }
    /**
     * How far the keyboard reaches up from the bottom edge, in dp, sent to Rust (Native.keyboard) frame by frame as it
     * slides, so the field rides on it. As an inset animation runs, Android first hands every view the insets it
     * will end with; those are held back until it ends (Slint's own copy passes them on: a frame with the field
     * already at the top). Rust draws on its own surface, about a frame after the keyboard's window moves (measured on
     * the emulator: the field trailed by one frame), so while the keyboard rises the field is put where it will be a
     * frame on, along its own curve: never under it. Going down it follows as it is (a frame early it went under the
     * keyboard; a frame late only leaves a gap that closes). Edge to edge, as Android 15 and later make every app:
     * without it older Android has no slide to follow, only where it ends up.
     */
    private fun followKeyboard() {
        if (Build.VERSION.SDK_INT < 30) return
        window.setDecorFitsSystemWindows(false)
        val ime = WindowInsets.Type.ime()
        val d = resources.displayMetrics.density
        val v = View(this)
        fun at(i: WindowInsets?) = (i?.getInsets(ime)?.bottom ?: 0) / d
        var sliding = false
        var from = 0f
        var to = 0f
        v.setOnApplyWindowInsetsListener { _, i -> if (!sliding) Native.keyboard(at(i)); i }
        v.setWindowInsetsAnimationCallback(object : WindowInsetsAnimation.Callback(DISPATCH_MODE_CONTINUE_ON_SUBTREE) {
            override fun onPrepare(a: WindowInsetsAnimation) {
                if (a.typeMask and ime != 0) { sliding = true; from = at(v.rootWindowInsets) }
            }
            override fun onStart(a: WindowInsetsAnimation, b: WindowInsetsAnimation.Bounds): WindowInsetsAnimation.Bounds {
                if (a.typeMask and ime != 0) to = at(v.rootWindowInsets)
                return b
            }
            override fun onProgress(i: WindowInsets, running: MutableList<WindowInsetsAnimation>): WindowInsets {
                if (!sliding) return i
                val a = running.firstOrNull { it.typeMask and ime != 0 }
                if (a == null || to <= from || a.durationMillis <= 0) {
                    Native.keyboard(at(i))
                } else {
                    val frame = 1000f / (display?.refreshRate ?: 60f)
                    val f = (a.fraction + frame / a.durationMillis).coerceAtMost(1f)
                    val k = a.interpolator?.getInterpolation(f) ?: f
                    Native.keyboard(from + (to - from) * k)
                }
                return i
            }
            override fun onEnd(a: WindowInsetsAnimation) {
                if (a.typeMask and ime != 0) { sliding = false; Native.keyboard(at(v.rootWindowInsets)) }
            }
        })
        addContentView(v, FrameLayout.LayoutParams(0, 0))
    }
    @JvmField @Volatile var waiting = false
    private var finish: Runnable? = null
    override fun surfaceRedrawNeededAsync(holder: SurfaceHolder, drawingFinished: Runnable) {
        finish?.run()
        finish = drawingFinished
        waiting = true
        surfaceRedrawNeeded(holder) // Rust draws now
        window.decorView.postDelayed({ drawn() }, 3000) // never held by a frame that doesn't come
    }
    fun drawn() = runOnUiThread { waiting = false; finish?.run(); finish = null }

    // How often the screen's picture changes, told to the display so it can slow down while it rests (home.rs);
    // 0: no preference. A touch lifts it at once.
    private var surface: Surface? = null
    private var fps = 0f
    override fun surfaceCreated(holder: SurfaceHolder) { super.surfaceCreated(holder); surface = holder.surface; fps = 0f }
    override fun surfaceDestroyed(holder: SurfaceHolder) { surface = null; super.surfaceDestroyed(holder) }
    fun rate(want: Float) = runOnUiThread {
        if (want != fps && Build.VERSION.SDK_INT >= 30) {
            fps = want
            surface?.takeIf { it.isValid }?.setFrameRate(want, Surface.FRAME_RATE_COMPATIBILITY_DEFAULT)
        }
    }
    override fun onUserInteraction() { super.onUserInteraction(); rate(0f) }

    /**
     * Empties the text field's copy of the text on the keyboard's side (Slint's input view) and restarts the keyboard
     * on it, as a text box does when the app sets its text: a message was just sent while the keyboard was still
     * putting a word together, and that word must not come back in the empty field.
     */
    fun clearField() = runOnUiThread {
        val v = currentFocus ?: return@runOnUiThread
        val f = runCatching { v.javaClass.getDeclaredField("mEditable").apply { isAccessible = true } }.getOrNull() ?: return@runOnUiThread
        (f.get(v) as? Editable)?.clear()
        (getSystemService(INPUT_METHOD_SERVICE) as InputMethodManager).restartInput(v)
    }
}

/** The home screen (phone/src/home.rs). */
class MainActivity : Shown() {
    /**
     * While something is open that Back closes (home.slint backable), the system back swipe is followed as it moves
     * (Android 14+): the open chat slides with the thumb and a cancelled swipe puts it back. With nothing open this
     * is unregistered, so Back reaches Slint as a key and the app goes to the background (home.slint).
     * Registered after Slint's own callback and before the keyboard's, so the keyboard still goes first.
     */
    private var back: Any? = null
    fun backable(on: Boolean) = runOnUiThread {
        if (Build.VERSION.SDK_INT < 34) return@runOnUiThread
        if (on && back == null) {
            val cb = object : OnBackAnimationCallback {
                override fun onBackStarted(e: BackEvent) = Native.back(0, e.progress)
                override fun onBackProgressed(e: BackEvent) = Native.back(1, e.progress)
                override fun onBackInvoked() = Native.back(2, 1f)
                override fun onBackCancelled() = Native.back(3, 0f)
            }
            onBackInvokedDispatcher.registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT, cb)
            back = cb
        } else if (!on && back != null) {
            onBackInvokedDispatcher.unregisterOnBackInvokedCallback(back as OnBackAnimationCallback)
            back = null
        }
    }
    override fun onCreate(savedInstanceState: Bundle?) { super.onCreate(savedInstanceState); Native.homeOpened(this) }
    override fun onNewIntent(intent: Intent) { super.onNewIntent(intent); setIntent(intent); Native.homeOpened(this) }
    override fun onResume() { super.onResume(); if (!isFinishing) Native.homeResumed(this) }
    override fun onStart() { super.onStart(); Native.homeShown(true) }
    override fun onStop() { Native.homeShown(false); super.onStop() }
    override fun onRequestPermissionsResult(code: Int, permissions: Array<out String>, results: IntArray) = Native.permissionsAnswered(this)
    @Deprecated("One picker: the result API needs androidx")
    override fun onActivityResult(code: Int, result: Int, data: Intent?) { super.onActivityResult(code, result, data); if (code == 7) Native.picked(this, data) }
}

/** The call screen (phone/src/screen.rs). */
class CallActivity : Shown() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Native.callOpened(this, intent)
        if (Build.VERSION.SDK_INT >= 33) {
            onBackInvokedDispatcher.registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT) { Native.callBack(this) }
        }
    }
    override fun onNewIntent(intent: Intent) { super.onNewIntent(intent); Native.callOpened(this, intent) }
    override fun onUserInteraction() { super.onUserInteraction(); Native.stopRinging(this) }
    @Deprecated("Used on Android 12 and earlier")
    override fun onBackPressed() = Native.callBack(this)
    override fun onDestroy() { Native.callClosed(this); super.onDestroy() }
}

/** Keeps the call listener running (phone/src/service.rs). */
class ListenService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int { Native.foreground(this); return START_STICKY }
    override fun onDestroy() { Native.stop(); super.onDestroy() }
}

class DeclineReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) = Native.declined(context, intent)
}

/** After a reboot or an app update, which otherwise leave the listener stopped until the app is opened. */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) = Native.start(context)
}

/** Widget tile taps. Not exported, so only this app's own taps can change the mode. */
class ModeTap : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) = Native.modeTapped(context, intent)
}

/** A chat notification's Reply or answer button (phone/src/chat_alert.rs). Not exported: only this app's own buttons. */
class ChatAction : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) = Native.chatAction(context, intent)
}

/** The contact-mode widget (phone/src/widget.rs). */
open class ModeWidget : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) = Native.widgets(context)
    override fun onAppWidgetOptionsChanged(context: Context, manager: AppWidgetManager, id: Int, options: Bundle) = Native.widgets(context)
}

/** Same widget, offered at 4x2 in the picker. */
class ModeWidgetWide : ModeWidget()

/**
 * Lets the video player read a video sent in a chat, by the PC or from this phone (filesDir/media, phone/src/inbox.rs). Not exported: only
 * the one-time read grant on the link the app opens reaches a file, and only a plain name inside that folder.
 */
class MediaProvider : ContentProvider() {
    private fun file(uri: Uri): File? {
        val dir = File(context!!.filesDir, "media")
        val f = File(dir, uri.lastPathSegment ?: return null)
        return f.takeIf { it.parentFile?.canonicalPath == dir.canonicalPath && it.isFile }
    }
    override fun onCreate() = true
    override fun getType(uri: Uri): String? = when (uri.lastPathSegment?.substringAfterLast('.')) {
        "mp4" -> "video/mp4"; "mov" -> "video/quicktime"; "webm" -> "video/webm"
        "png" -> "image/png"; "jpg" -> "image/jpeg"; "webp" -> "image/webp"; "gif" -> "image/gif"
        else -> null
    }
    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
        if (mode != "r") throw SecurityException("read only")
        return ParcelFileDescriptor.open(file(uri) ?: throw FileNotFoundException(), ParcelFileDescriptor.MODE_READ_ONLY)
    }
    override fun query(uri: Uri, projection: Array<out String>?, selection: String?, args: Array<out String>?, order: String?): Cursor? {
        val f = file(uri) ?: return null
        return MatrixCursor(arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE)).apply { addRow(arrayOf<Any>(f.name, f.length())) }
    }
    override fun insert(uri: Uri, values: ContentValues?): Uri? = null
    override fun update(uri: Uri, values: ContentValues?, selection: String?, args: Array<out String>?) = 0
    override fun delete(uri: Uri, selection: String?, args: Array<out String>?) = 0
}
