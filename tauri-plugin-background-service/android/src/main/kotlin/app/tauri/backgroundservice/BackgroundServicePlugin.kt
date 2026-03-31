package app.tauri.backgroundservice

import android.app.Activity
import android.content.Intent
import android.os.Build
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

@InvokeArg class StartKeepaliveArgs { var label: String = "Service running" }

@TauriPlugin
class BackgroundServicePlugin(private val activity: Activity) : Plugin(activity) {

    override fun load(webView: android.webkit.WebView) {
        super.load(webView)
        // Request POST_NOTIFICATIONS once so Rust's Notifier can fire freely
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            activity.checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS)
            != android.content.pm.PackageManager.PERMISSION_GRANTED
        ) {
            activity.requestPermissions(
                arrayOf(android.Manifest.permission.POST_NOTIFICATIONS), 1001)
        }
    }

    @Command
    fun startKeepalive(invoke: Invoke) {
        val args  = invoke.parseArgs(StartKeepaliveArgs::class.java)
        val intent = Intent(activity, LifecycleService::class.java).apply {
            action = LifecycleService.ACTION_START
            putExtra(LifecycleService.EXTRA_LABEL, args.label)
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O)
            activity.startForegroundService(intent)
        else
            activity.startService(intent)
        invoke.resolve()
    }

    @Command
    fun stopKeepalive(invoke: Invoke) {
        activity.startService(Intent(activity, LifecycleService::class.java)
            .apply { action = LifecycleService.ACTION_STOP })
        invoke.resolve()
    }
}
