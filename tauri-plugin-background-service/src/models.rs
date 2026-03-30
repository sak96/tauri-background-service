use serde::{Deserialize, Serialize};
use tauri::Runtime;
use tokio_util::sync::CancellationToken;

use crate::notifier::Notifier;

/// Passed into both `init` and `run`.
/// Gives your service everything it needs to interact with the outside world.
pub struct ServiceContext<R: Runtime> {
    /// Fire a local notification. Works on all platforms.
    pub notifier: Notifier<R>,

    /// Emit an event to the JS UI layer.
    pub app: tauri::AppHandle<R>,

    /// Cancelled when `stopService()` is called.
    pub shutdown: CancellationToken,
}

/// Optional startup configuration forwarded from JS through the plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartConfig {
    /// Text shown in the Android persistent foreground notification.
    #[serde(default = "default_label")]
    pub service_label: String,
}

fn default_label() -> String {
    "Service running".into()
}

impl Default for StartConfig {
    fn default() -> Self {
        Self {
            service_label: default_label(),
        }
    }
}

/// Built-in event types emitted by the runner itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum PluginEvent {
    /// init() completed successfully
    Started,
    /// run() returned or was cancelled
    Stopped { reason: String },
    /// init() or run() returned an error
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- StartConfig tests ---

    #[test]
    fn start_config_default_label() {
        let config = StartConfig::default();
        assert_eq!(config.service_label, "Service running");
    }

    #[test]
    fn start_config_custom_label() {
        let config = StartConfig {
            service_label: "Syncing data".into(),
        };
        assert_eq!(config.service_label, "Syncing data");
    }

    #[test]
    fn start_config_serde_roundtrip_default() {
        let config = StartConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let de: StartConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(de.service_label, config.service_label);
    }

    #[test]
    fn start_config_serde_roundtrip_custom() {
        let config = StartConfig {
            service_label: "My service".into(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let de: StartConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(de.service_label, "My service");
    }

    #[test]
    fn start_config_deserialize_missing_field_uses_default() {
        // An empty JSON object should produce the default label
        let json = "{}";
        let de: StartConfig = serde_json::from_str(json).unwrap();
        assert_eq!(de.service_label, "Service running");
    }

    #[test]
    fn start_config_json_key_is_camel_case() {
        let config = StartConfig {
            service_label: "test".into(),
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("serviceLabel"), "JSON should use camelCase: {json}");
    }

    // --- PluginEvent tests ---

    #[test]
    fn plugin_event_started_serde_roundtrip() {
        let event = PluginEvent::Started;
        let json = serde_json::to_string(&event).unwrap();
        let de: PluginEvent = serde_json::from_str(&json).unwrap();
        assert!(matches!(de, PluginEvent::Started));
    }

    #[test]
    fn plugin_event_stopped_serde_roundtrip() {
        let event = PluginEvent::Stopped {
            reason: "cancelled".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let de: PluginEvent = serde_json::from_str(&json).unwrap();
        match de {
            PluginEvent::Stopped { reason } => assert_eq!(reason, "cancelled"),
            other => panic!("Expected Stopped, got {other:?}"),
        }
    }

    #[test]
    fn plugin_event_error_serde_roundtrip() {
        let event = PluginEvent::Error {
            message: "init failed".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let de: PluginEvent = serde_json::from_str(&json).unwrap();
        match de {
            PluginEvent::Error { message } => assert_eq!(message, "init failed"),
            other => panic!("Expected Error, got {other:?}"),
        }
    }

    #[test]
    fn plugin_event_tagged_json_format() {
        let event = PluginEvent::Started;
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"started\""), "Tagged JSON: {json}");
    }

    #[test]
    fn plugin_event_stopped_json_keys_camel_case() {
        let event = PluginEvent::Stopped {
            reason: "done".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"stopped\""), "Tag: {json}");
        assert!(json.contains("\"reason\":\"done\""), "Reason: {json}");
    }

    #[test]
    fn plugin_event_error_json_keys_camel_case() {
        let event = PluginEvent::Error {
            message: "oops".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"error\""), "Tag: {json}");
        assert!(json.contains("\"message\":\"oops\""), "Message: {json}");
    }
}
