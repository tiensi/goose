mod agent;
mod default;
mod factory;
mod mcp_manager;

pub use agent::Agent;
pub use default::DefaultAgent;
pub use factory::{register_agent, AgentFactory};
pub use mcp_manager::MCPManager;