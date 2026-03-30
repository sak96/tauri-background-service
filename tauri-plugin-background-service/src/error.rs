#[derive(Debug, thiserror::Error, Clone, serde::Serialize, serde::Deserialize)]
pub enum ServiceError {
    #[error("Service is already running")]
    AlreadyRunning,

    #[error("Service is not running")]
    NotRunning,

    #[error("Initialisation failed: {0}")]
    Init(String),

    #[error("Runtime error: {0}")]
    Runtime(String),

    #[error("Platform error: {0}")]
    Platform(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_already_running() {
        assert_eq!(ServiceError::AlreadyRunning.to_string(), "Service is already running");
    }

    #[test]
    fn display_not_running() {
        assert_eq!(ServiceError::NotRunning.to_string(), "Service is not running");
    }

    #[test]
    fn display_init() {
        let msg = "db connection failed".to_string();
        assert_eq!(
            ServiceError::Init(msg.clone()).to_string(),
            format!("Initialisation failed: {msg}")
        );
    }

    #[test]
    fn display_runtime() {
        let msg = "network timeout".to_string();
        assert_eq!(
            ServiceError::Runtime(msg.clone()).to_string(),
            format!("Runtime error: {msg}")
        );
    }

    #[test]
    fn display_platform() {
        let msg = "foreground service denied".to_string();
        assert_eq!(
            ServiceError::Platform(msg.clone()).to_string(),
            format!("Platform error: {msg}")
        );
    }

    #[test]
    fn convert_to_invoke_error_via_serialize() {
        // ServiceError derives Serialize, so Tauri's blanket From<T: Serialize> for InvokeError applies.
        // Verify the conversion compiles (type-level proof).
        let err = ServiceError::Init("test".into());
        let invoke_err: tauri::ipc::InvokeError = err.into();
        // InvokeError wraps serde_json::Value — verify it contains the serialized form
        let _val = &invoke_err.0;
        assert!(!invoke_err.0.is_null());
    }

    #[test]
    fn clone_roundtrip() {
        let err = ServiceError::Init("test".into());
        let cloned = err.clone();
        assert_eq!(err.to_string(), cloned.to_string());
    }

    #[test]
    fn serde_roundtrip_already_running() {
        let err = ServiceError::AlreadyRunning;
        let json = serde_json::to_string(&err).unwrap();
        let de: ServiceError = serde_json::from_str(&json).unwrap();
        assert!(matches!(de, ServiceError::AlreadyRunning));
    }

    #[test]
    fn serde_roundtrip_init() {
        let err = ServiceError::Init("boom".into());
        let json = serde_json::to_string(&err).unwrap();
        let de: ServiceError = serde_json::from_str(&json).unwrap();
        assert!(matches!(de, ServiceError::Init(ref s) if s == "boom"));
    }
}
