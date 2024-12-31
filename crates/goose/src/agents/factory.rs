use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use super::Agent;
use crate::errors::AgentError;
use crate::providers::base::Provider;

type AgentConstructor = Box<dyn Fn(Box<dyn Provider>) -> Box<dyn Agent> + Send + Sync>;

// Use std::sync::RwLock for interior mutability
static AGENT_REGISTRY: OnceLock<RwLock<HashMap<&'static str, AgentConstructor>>> = OnceLock::new();

/// Initialize the registry if it hasn't been initialized
fn registry() -> &'static RwLock<HashMap<&'static str, AgentConstructor>> {
    AGENT_REGISTRY.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Register a new agent version
pub fn register_agent(
    version: &'static str,
    constructor: impl Fn(Box<dyn Provider>) -> Box<dyn Agent> + Send + Sync + 'static,
) {
    let registry = registry();
    if let Ok(mut map) = registry.write() {
        map.insert(version, Box::new(constructor));
    }
}

pub struct AgentFactory;

impl AgentFactory {
    /// Create a new agent instance of the specified version
    pub fn create(
        version: &str,
        provider: Box<dyn Provider>,
    ) -> Result<Box<dyn Agent>, AgentError> {
        let registry = registry();
        if let Ok(map) = registry.read() {
            if let Some(constructor) = map.get(version) {
                Ok(constructor(provider))
            } else {
                Err(AgentError::VersionNotFound(version.to_string()))
            }
        } else {
            Err(AgentError::Internal(
                "Failed to access agent registry".to_string(),
            ))
        }
    }

    /// Get a list of all available agent versions
    pub fn available_versions() -> Vec<&'static str> {
        registry()
            .read()
            .map(|map| map.keys().copied().collect())
            .unwrap_or_default()
    }

    /// Get the default version name
    pub fn default_version() -> &'static str {
        "default"
    }
}

/// Macro to help with agent registration
#[macro_export]
macro_rules! register_agent {
    ($version:expr, $agent_type:ty) => {
        paste::paste! {
            #[ctor::ctor]
            #[allow(non_snake_case)]
            fn [<__register_agent_ $version>]() {
                $crate::agents::factory::register_agent($version, |provider| {
                    Box::new(<$agent_type>::new(provider))
                });
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::Message;
    use crate::providers::mock::MockProvider;
    use crate::providers::base::ProviderUsage;
    use crate::errors::AgentResult;
    use crate::systems::System;
    use async_trait::async_trait;
    use futures::stream::BoxStream;
    use serde_json::Value;
    use tokio::sync::Mutex;

    // Test agent implementation
    struct TestAgent {
        mcp_manager: Mutex<super::super::MCPManager>,
    }

    impl TestAgent {
        fn new(provider: Box<dyn Provider>) -> Self {
            Self {
                mcp_manager: Mutex::new(super::super::MCPManager::new(provider)),
            }
        }
    }

    #[async_trait]
    impl Agent for TestAgent {
        async fn add_system(&mut self, system: Box<dyn System>) -> AgentResult<()> {
            let mut manager = self.mcp_manager.lock().await;
            manager.add_system(system);
            Ok(())
        }

        async fn remove_system(&mut self, name: &str) -> AgentResult<()> {
            let mut manager = self.mcp_manager.lock().await;
            manager.remove_system(name)
        }

        async fn list_systems(&self) -> AgentResult<Vec<(String, String)>> {
            let manager = self.mcp_manager.lock().await;
            manager.list_systems().await
        }

        async fn passthrough(&self, _system: &str, _request: Value) -> AgentResult<Value> {
            Ok(Value::Null)
        }

        async fn reply(&self, _messages: &[Message]) -> anyhow::Result<BoxStream<'_, anyhow::Result<Message>>> {
            Ok(Box::pin(futures::stream::empty()))
        }

        async fn usage(&self) -> AgentResult<Vec<ProviderUsage>> {
            Ok(vec![])
        }
    }

    #[test]
    fn test_register_and_create_agent() {
        register_agent!("test_create", TestAgent);

        // Create a mock provider
        let provider = Box::new(MockProvider::new(vec![]));

        // Create an agent instance
        let result = AgentFactory::create("test_create", provider);
        assert!(result.is_ok());
    }

    #[test]
    fn test_version_not_found() {
        // Try to create an agent with a non-existent version
        let provider = Box::new(MockProvider::new(vec![]));
        let result = AgentFactory::create("nonexistent", provider);

        assert!(matches!(result, Err(AgentError::VersionNotFound(_))));
        if let Err(AgentError::VersionNotFound(version)) = result {
            assert_eq!(version, "nonexistent");
        }
    }

    #[test]
    fn test_available_versions() {
        register_agent!("test_available_1", TestAgent);
        register_agent!("test_available_2", TestAgent);

        // Get available versions
        let versions = AgentFactory::available_versions();

        assert!(versions.contains(&"test_available_1"));
        assert!(versions.contains(&"test_available_2"));
    }

    #[test]
    fn test_default_version() {
        assert_eq!(AgentFactory::default_version(), "base");
    }

    #[test]
    fn test_multiple_registrations() {
        register_agent!("test_duplicate", TestAgent);
        register_agent!("test_duplicate_other", TestAgent);

        // Create an agent instance
        let provider = Box::new(MockProvider::new(vec![]));
        let result = AgentFactory::create("test_duplicate", provider);

        // Should still work, last registration wins
        assert!(result.is_ok());
    }
}