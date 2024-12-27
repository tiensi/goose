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
        "base"
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
    use crate::providers::base::ProviderUsage;
    use crate::providers::mock::MockProvider;
    use crate::systems::System;
    use async_trait::async_trait;
    use tokio::sync::Mutex;

    // Test agent implementation
    struct TestAgent {
        systems: Vec<Box<dyn System>>,
        provider: Box<dyn Provider>,
        provider_usage: Mutex<Vec<ProviderUsage>>,
    }

    impl TestAgent {
        fn new(provider: Box<dyn Provider>) -> Self {
            Self {
                systems: Vec::new(),
                provider,
                provider_usage: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl Agent for TestAgent {
        fn add_system(&mut self, system: Box<dyn System>) {
            self.systems.push(system);
        }

        fn get_systems(&self) -> &Vec<Box<dyn System>> {
            &self.systems
        }

        fn get_provider(&self) -> &Box<dyn Provider> {
            &self.provider
        }

        fn get_provider_usage(&self) -> &Mutex<Vec<ProviderUsage>> {
            &self.provider_usage
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

    #[test]
    fn test_agent_with_provider() {
        register_agent!("test_provider_check", TestAgent);

        // Create a mock provider with specific configuration
        let provider = Box::new(MockProvider::new(vec![]));

        // Create an agent instance
        let agent = AgentFactory::create("test_provider_check", provider).unwrap();

        // Verify the provider is correctly passed to the agent
        assert_eq!(agent.get_provider().get_model_config().model_name, "mock");
    }
}
