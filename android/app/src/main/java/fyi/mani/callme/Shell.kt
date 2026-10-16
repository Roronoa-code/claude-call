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

private const val FIRST_FRAME_TIMEOUT_MS = 3000L

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

/** Android counts the window as drawn while it is still empty, so the launcher would show until the GPU's first frame. */
open class Shown : NativeActivity() {
    // NativeActivity sets "keyboard: unspecified" over the manifest's, which lets Android raise the keyboard by itself
    // on opening whenever Slint's always-focusable input view holds the focus.
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_STATE_HIDDEN or WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE)
        followKeyboard()
    }
    /**
     * During an inset animation Android first hands every view the insets it will end with, and Rust's own surface
     * draws about a frame after the keyboard's window moves. Without edge to edge, older Android has no slide to follow.
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
        surfaceRedrawNeeded(holder)
        window.decorView.postDelayed({ drawn() }, FIRST_FRAME_TIMEOUT_MS)
    }
    fun drawn() = runOnUiThread { waiting = false; finish?.run(); finish = null }

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

    fun clearField() = runOnUiThread {
        val v = currentFocus ?: return@runOnUiThread
        val f = runCatching { v.javaClass.getDeclaredField("mEditable").apply { isAccessible = true } }.getOrNull() ?: return@runOnUiThread
        (f.get(v) as? Editable)?.clear()
        (getSystemService(INPUT_METHOD_SERVICE) as InputMethodManager).restartInput(v)
    }
}

class MainActivity : Shown() {
    /** Back-swipe progress needs Android 14+. Registered after Slint's callback and before the keyboard's, so the keyboard goes first. */
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

class ListenService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int { Native.foreground(this); return START_STICKY }
    override fun onDestroy() { Native.stop(); super.onDestroy() }
}

class DeclineReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) = Native.declined(context, intent)
}

class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) = Native.start(context)
}

class ModeTap : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) = Native.modeTapped(context, intent)
}

class ChatAction : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) = Native.chatAction(context, intent)
}

open class ModeWidget : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) = Native.widgets(context)
    override fun onAppWidgetOptionsChanged(context: Context, manager: AppWidgetManager, id: Int, options: Bundle) = Native.widgets(context)
}

class ModeWidgetWide : ModeWidget()

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
