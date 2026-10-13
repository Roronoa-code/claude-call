package fyi.mani.callme

import android.Manifest
import android.annotation.SuppressLint
import android.app.NativeActivity
import android.app.NotificationManager
import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.PowerManager
import android.provider.Settings
import kotlin.concurrent.thread

/**
 * The home screen. Drawn and run by the Rust library (phone/src/home.rs); this class only keeps the Android
 * calls it makes: permissions, settings, widgets and the test ring. Rust calls the methods below by name.
 */
class MainActivity : NativeActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        returnToCall()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        returnToCall()
    }

    // During a call the app icon brings the call back.
    private fun returnToCall() {
        val link = ListenService.activeCallLink ?: return
        startActivity(CallActivity.intent(this, link, answer = true))
        finish()
    }

    override fun onResume() {
        super.onResume()
        if (isFinishing) return
        if (missing().isEmpty()) ListenService.start(this)
        // Pick up a mode changed elsewhere (Claude or Discord).
        thread { ContactMode.latest(this)?.let { (m, at) -> ContactMode.received(this, m, at) } }
    }

    /** What still needs the user's OK, comma-separated ("" = all set). */
    fun missing(): String {
        val nm = getSystemService(NotificationManager::class.java)
        val pm = getSystemService(PowerManager::class.java)
        return buildList {
            if (checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) add("Notifications")
            if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) add("Microphone")
            if (Build.VERSION.SDK_INT >= 34 && !nm.canUseFullScreenIntent()) add("Full-screen calls")
            if (!pm.isIgnoringBatteryOptimizations(packageName)) add("Battery: unrestricted")
        }.joinToString(", ")
    }

    /** Walks through whatever is still missing, one system screen at a time. */
    @SuppressLint("BatteryLife")
    fun setUp() = runOnUiThread {
        val m = missing()
        when {
            "Notifications" in m || "Microphone" in m ->
                requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS, Manifest.permission.RECORD_AUDIO), 1)
            "Full-screen calls" in m ->
                startActivity(Intent(Settings.ACTION_MANAGE_APP_USE_FULL_SCREEN_INTENT, Uri.parse("package:$packageName")))
            "Battery: unrestricted" in m ->
                startActivity(Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:$packageName")))
        }
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        if (missing().isNotEmpty()) setUp() else ListenService.start(this)
    }

    fun mode(): String = Prefs.mode(this)
    fun setMode(mode: String) {
        Prefs.setMode(this, mode)
        ContactMode.publish(this, mode)
        ModeWidget.updateAll(this)
    }

    fun topic(): String = Prefs.topic(this)
    fun saveTopic(topic: String) = runOnUiThread {
        Prefs.setTopic(this, topic.trim())
        stopService(Intent(this, ListenService::class.java))
        ListenService.start(this)
    }

    fun canPinWidgets(): Boolean = getSystemService(AppWidgetManager::class.java).isRequestPinAppWidgetSupported
    fun pinWidget(wide: Boolean) = runOnUiThread {
        val cls = if (wide) ModeWidgetWide::class.java else ModeWidget::class.java
        getSystemService(AppWidgetManager::class.java).requestPinAppWidget(ComponentName(this, cls), null, null)
    }

    fun testRing() = Native.ring(applicationContext, CallActivity.TEST_LINK, "Test ring")
}
