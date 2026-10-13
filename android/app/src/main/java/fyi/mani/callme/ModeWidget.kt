package fyi.mani.callme

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.BroadcastReceiver
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.text.format.DateFormat
import android.util.SizeF
import android.util.TypedValue
import android.view.View
import android.widget.RemoteViews

/** "Aura" home-screen widget: four tiles for the contact mode, the chosen one lit in its colour. */
open class ModeWidget : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) {
        ids.forEach { manager.updateAppWidget(it, views(context, manager.getAppWidgetOptions(it))) }
    }

    /** Resized, or the launcher reported its real size: rebuild for that size. */
    override fun onAppWidgetOptionsChanged(context: Context, manager: AppWidgetManager, id: Int, options: Bundle) {
        manager.updateAppWidget(id, views(context, options))
    }

    private class Mode(
        val id: String, val label: String, val title: String, val line: String,
        val glyph: Int, val lit: Int, val outline: Int, val tile: Int, val icon: Int, val text: Int,
    )

    companion object {
        private const val WHITE = 0xFFFFFFFF.toInt()
        private const val DIM = 0x8CFFFFFF.toInt() // white at 55%

        // Copy and colours from the approved preview (design-preview/src/widgets.jsx).
        private val MODES = listOf(
            Mode("call", "Calls", "Calls only", "Claude can call you",
                R.drawable.glyph_call, R.drawable.widget_lit_call, R.drawable.widget_outline_call, R.id.tile_call, R.id.icon_call, R.id.label_call),
            Mode("text", "Texts", "Texts only", "Claude can message you",
                R.drawable.glyph_text, R.drawable.widget_lit_text, R.drawable.widget_outline_text, R.id.tile_text, R.id.icon_text, R.id.label_text),
            Mode("both", "Both", "Calls and texts", "Claude can call or message you",
                R.drawable.glyph_both, R.drawable.widget_lit_both, R.drawable.widget_outline_both, R.id.tile_both, R.id.icon_both, R.id.label_both),
            Mode("none", "None", "Do not disturb", "Claude saves its questions",
                R.drawable.glyph_none, R.drawable.widget_lit_none, R.drawable.widget_outline_none, R.id.tile_none, R.id.icon_none, R.id.label_none),
        )

        /** Redraws every placed widget (both sizes) from Prefs. */
        fun updateAll(context: Context) {
            val manager = AppWidgetManager.getInstance(context)
            listOf(ModeWidget::class.java, ModeWidgetWide::class.java).forEach { cls ->
                manager.getAppWidgetIds(ComponentName(context, cls))
                    .forEach { manager.updateAppWidget(it, views(context, manager.getAppWidgetOptions(it))) }
            }
        }

        // The preview's cards (design-preview, 412dp screen): 2x2 is 178x212, 4x2 is 380x212.
        private const val PREVIEW_H = 212f
        private const val COMPACT_ASPECT = 0.92f // preview is 178/212; a touch wider keeps One UI tiles near square
        private const val WIDE_ASPECT = 2f // preview is 380/212; a touch wider keeps One UI tiles near square
        private const val WIDE_MIN_WIDTH = 300f // below this the card's title no longer fits beside the tiles

        /**
         * One layout per size the launcher may show (API 31+). One UI cells are taller than the preview's, so the
         * card keeps the preview's aspect ratio and is centred in the cell, and its contents scale with the card.
         */
        private fun views(context: Context, options: Bundle?): RemoteViews {
            if (Build.VERSION.SDK_INT < 31) return build(context, false, null)
            @Suppress("DEPRECATION")
            val sizes = options?.getParcelableArrayList<SizeF>(AppWidgetManager.OPTION_APPWIDGET_SIZES)
            if (sizes.isNullOrEmpty()) return RemoteViews(mapOf(
                SizeF(100f, 100f) to build(context, false, null),
                SizeF(WIDE_MIN_WIDTH, 100f) to build(context, true, null),
            ))
            return RemoteViews(sizes.distinct().associateWith { build(context, it.width >= WIDE_MIN_WIDTH, it) })
        }

        private fun build(context: Context, wide: Boolean, size: SizeF?): RemoteViews {
            val current = Prefs.mode(context)
            val v = RemoteViews(context.packageName, if (wide) R.layout.widget_mode_wide else R.layout.widget_mode_compact)
            var k = 1f // scale against the preview card
            if (size != null && Build.VERSION.SDK_INT >= 31) {
                val cardH = minOf(size.height, size.width / if (wide) WIDE_ASPECT else COMPACT_ASPECT)
                k = (cardH / PREVIEW_H).coerceIn(0.8f, 1.4f)
                v.setViewLayoutHeight(android.R.id.background, cardH, TypedValue.COMPLEX_UNIT_DIP)
            }
            val px = { dp: Float -> (dp * k * context.resources.displayMetrics.density).toInt() }
            MODES.forEach { m ->
                val on = m.id == current
                v.setInt(m.tile, "setBackgroundResource", if (!on) R.drawable.widget_tile else if (wide) m.outline else m.lit)
                v.setInt(m.icon, "setImageAlpha", if (on) 255 else 140) // white glyph at 55%, like the label (tint was ignored on One UI)
                v.setTextColor(m.text, if (on) WHITE else DIM)
                v.setTextViewTextSize(m.text, TypedValue.COMPLEX_UNIT_SP, 14f * k)
                v.setContentDescription(m.tile, if (on) "${m.label}, selected" else m.label)
                v.setOnClickPendingIntent(m.tile, tap(context, m.id))
                if (!wide) v.setViewPadding(m.tile, px(12f), px(12f), px(12f), px(10f))
                if (Build.VERSION.SDK_INT >= 31) {
                    val icon = (if (wide) 20f else if (on) 26f else 22f) * k
                    v.setViewLayoutWidth(m.icon, icon, TypedValue.COMPLEX_UNIT_DIP)
                    v.setViewLayoutHeight(m.icon, icon, TypedValue.COMPLEX_UNIT_DIP)
                }
            }
            if (wide) {
                val m = MODES.firstOrNull { it.id == current } ?: MODES[2]
                v.setInt(R.id.hero, "setBackgroundResource", m.lit)
                v.setViewPadding(R.id.hero, px(14f), px(14f), px(14f), px(12f))
                v.setImageViewResource(R.id.hero_icon, m.glyph)
                v.setTextViewText(R.id.hero_title, m.title)
                v.setTextViewText(R.id.hero_line, m.line)
                v.setTextViewTextSize(R.id.hero_line, TypedValue.COMPLEX_UNIT_SP, 13f * k)
                v.setTextViewTextSize(R.id.hero_since, TypedValue.COMPLEX_UNIT_SP, 12f * k)
                if (Build.VERSION.SDK_INT >= 31) {
                    v.setViewLayoutWidth(R.id.hero_icon, 30f * k, TypedValue.COMPLEX_UNIT_DIP)
                    v.setViewLayoutHeight(R.id.hero_icon, 30f * k, TypedValue.COMPLEX_UNIT_DIP)
                    v.setViewLayoutHeight(R.id.hero_title, 23f * k, TypedValue.COMPLEX_UNIT_DIP) // title auto-sizes to this
                }
                val at = Prefs.modeChangedAt(context)
                v.setViewVisibility(R.id.hero_since, if (at > 0) View.VISIBLE else View.GONE)
                if (at > 0) {
                    val pattern = if (DateFormat.is24HourFormat(context)) "H:mm" else "h:mm a"
                    v.setTextViewText(R.id.hero_since, "Changed " + DateFormat.format(pattern, at))
                }
            }
            return v
        }

        private fun tap(context: Context, mode: String): PendingIntent = PendingIntent.getBroadcast(
            context, mode.hashCode(),
            Intent(context, ModeTap::class.java).setData(Uri.parse("callme://mode/$mode")),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
    }
}

/** Same widget, offered at 4x2 in the picker. */
class ModeWidgetWide : ModeWidget()

/** Tile taps. Not exported, so only this app's own PendingIntents can change the mode. */
class ModeTap : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val mode = intent.data?.lastPathSegment ?: return
        if (mode !in ContactMode.MODES || mode == Prefs.mode(context)) return
        Prefs.setMode(context, mode)
        ContactMode.publish(context, mode)
        ModeWidget.updateAll(context)
    }
}
