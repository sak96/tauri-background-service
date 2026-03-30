package app.tauri.backgroundservice

import android.app.*
import android.content.Intent
import android.os.IBinder
import androidx.core.app.NotificationCompat

class LifecycleService : Service() {

    companion object {
        const val CHANNEL_ID   = "bg_keepalive"
        const val NOTIF_ID     = 9001
        const val EXTRA_LABEL  = "label"
        const val ACTION_START = "START"
        const val ACTION_STOP  = "STOP"

        @Volatile var isRunning = false
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) { stopSelf(); return START_NOT_STICKY }

        val label = intent?.getStringExtra(EXTRA_LABEL) ?: "Service running"
        createChannel()
        startForeground(NOTIF_ID, buildNotification(label))
        isRunning = true

        // If the OS kills this process under memory pressure, it will
        // restart it automatically with a null intent.
        return START_STICKY
    }

    override fun onDestroy()          { isRunning = false; super.onDestroy() }
    override fun onBind(i: Intent?)   = null

    private fun buildNotification(label: String): Notification {
        val pi = packageManager.getLaunchIntentForPackage(packageName)
            ?.let { PendingIntent.getActivity(this, 0, it,
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT) }

        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle(applicationInfo.loadLabel(packageManager).toString())
            .setContentText(label)
            .setSmallIcon(android.R.drawable.stat_notify_sync)
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .apply { pi?.let { setContentIntent(it) } }
            .build()
    }

    private fun createChannel() {
        getSystemService(NotificationManager::class.java)
            .createNotificationChannel(
                NotificationChannel(CHANNEL_ID, "Service Status",
                    NotificationManager.IMPORTANCE_LOW)
                    .apply { setShowBadge(false) }
            )
    }
}
