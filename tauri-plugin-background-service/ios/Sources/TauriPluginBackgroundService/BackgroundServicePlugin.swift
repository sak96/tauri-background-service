import UIKit
import BackgroundTasks
import UserNotifications
import WebKit

@objc public class BackgroundServicePlugin: Plugin {

    private var taskId: String {
        "\(Bundle.main.bundleIdentifier ?? "app").bg-refresh"
    }

    public override func load(webView: WKWebView) {
        super.load(webView)

        // Request notification permission once.
        // After this, Rust's Notifier can post notifications freely.
        UNUserNotificationCenter.current()
            .requestAuthorization(options: [.alert, .sound, .badge]) { _, _ in }

        // Register background task handler before the app finishes launching.
        BGTaskScheduler.shared.register(forTaskWithIdentifier: taskId, using: .main) {
            [weak self] task in
            // The OS has granted a background execution window.
            // The Rust Tokio runtime is already in this process and will
            // use this window to run its async work naturally.
            task.setTaskCompleted(success: true)
            self?.scheduleNext()
        }
    }

    @objc public func startKeepalive(_ invoke: Invoke) {
        scheduleNext()
        invoke.resolve()
    }

    @objc public func stopKeepalive(_ invoke: Invoke) {
        BGTaskScheduler.shared.cancel(taskRequestWithIdentifier: taskId)
        invoke.resolve()
    }

    private func scheduleNext() {
        let req = BGAppRefreshTaskRequest(identifier: taskId)
        req.earliestBeginDate = Date(timeIntervalSinceNow: 15 * 60)
        try? BGTaskScheduler.shared.submit(req)
    }
}
