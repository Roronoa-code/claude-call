package fyi.mani.callme;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;
import android.os.SystemClock;
import android.view.View;
import android.view.ViewGroup;
import android.webkit.WebView;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;

/** Opens the normal call activity for a bounded device check without ringing. */
public final class PhoneCheckInstrumentation extends Instrumentation {
    private String link;

    @Override public void onCreate(Bundle arguments) {
        super.onCreate(arguments);
        link = arguments == null ? null : arguments.getString("link");
        start();
    }

    @Override public void onStart() {
        Bundle result = new Bundle();
        Activity activity = null;
        int code = Activity.RESULT_CANCELED;
        try {
            Uri uri = link == null ? null : Uri.parse(link);
            if (uri == null || !"https".equals(uri.getScheme()) || uri.getHost() == null) {
                throw new IllegalArgumentException("Pass -e link with an HTTPS check URL");
            }
            runOnMainSync(() -> WebView.setWebContentsDebuggingEnabled(true));
            Intent intent = new Intent().setClassName(getTargetContext(), "fyi.mani.callme.CallActivity")
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK).putExtra("link", link).putExtra("answer", true);
            activity = startActivitySync(intent);
            final Activity call = activity;
            long deadline = SystemClock.elapsedRealtime() + 180_000;
            boolean done = false;
            while (SystemClock.elapsedRealtime() < deadline && !call.isFinishing() && !call.isDestroyed()) {
                CountDownLatch read = new CountDownLatch(1);
                AtomicBoolean complete = new AtomicBoolean();
                runOnMainSync(() -> {
                    WebView web = findWebView(call.getWindow().getDecorView());
                    if (web == null) { read.countDown(); return; }
                    web.evaluateJavascript("Boolean(window.__phoneCheckDone)", value -> {
                        complete.set("true".equals(value));
                        read.countDown();
                    });
                });
                read.await(1, TimeUnit.SECONDS);
                if (complete.get()) { done = true; break; }
                Thread.sleep(250);
            }
            if (!done) throw new IllegalStateException("Check did not signal completion within 180 seconds, or activity closed");
            result.putString("stream", "Phone check completed\n");
            code = Activity.RESULT_OK;
        } catch (Exception error) {
            result.putString("stream", "Phone check failed: " + error.getClass().getSimpleName() + ": " + error.getMessage() + "\n");
        } finally {
            final Activity call = activity;
            runOnMainSync(() -> {
                if (call != null && !call.isFinishing()) call.finish();
                WebView.setWebContentsDebuggingEnabled(false);
            });
        }
        finish(code, result);
    }

    private static WebView findWebView(View view) {
        if (view instanceof WebView) return (WebView) view;
        if (view instanceof ViewGroup) {
            ViewGroup group = (ViewGroup) view;
            for (int i = 0; i < group.getChildCount(); i++) {
                WebView web = findWebView(group.getChildAt(i));
                if (web != null) return web;
            }
        }
        return null;
    }
}
