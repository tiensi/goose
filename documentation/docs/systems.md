# Systems Design Guide

This document describes the design and implementation of the Systems framework in Goose, which enables AI agents to interact with different systems through a unified tool-based interface.

## Core Concepts

### System
A System represents any component that can be operated by an AI agent. Systems expose their capabilities through Tools and maintain their own state. The core interface is defined by the `System` trait:

```rust
#[async_trait]
pub trait System: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn instructions(&self) -> &str;
    fn tools(&self) -> &[Tool];
    async fn status(&self) -> AnyhowResult<HashMap<String, Value>>;
    async fn call_tool(&self, tool_name: &str, parameters: HashMap<String, Value>) -> ToolResult<Value>;
}
```

### Tools
Tools are the primary way Systems expose functionality to agents. Each tool has:
- A name
- A description
- A set of parameters
- An implementation that executes the tool's functionality

A tool must take a Value and return an `AgentResult<Value>` (it must also be async). This
is what makes it compatible with the tool calling framework from the agent. 

```rust
async fn echo(&self, params: Value) -> AgentResult<Value>
```

## Architecture

### Component Overview

1. **System Trait**: The core interface that all systems must implement
2. **Error Handling**: Specialized error types for tool execution
3. **Proc Macros**: Simplify tool definition and registration [*not yet implemented*]

### Error Handling

The system uses two main error types:
- `ToolError`: Specific errors related to tool execution
- `anyhow::Error`: General purpose errors for system status and other operations

This split allows precise error handling for tool execution while maintaining flexibility for general system operations.

## Best Practices

### Tool Design

1. **Clear Names**: Use clear, action-oriented names for tools (e.g., "create_user" not "user")
2. **Descriptive Parameters**: Each parameter should have a clear description
3. **Error Handling**: Return specific errors when possible, the errors become "prompts"
4. **State Management**: Be explicit about state modifications

### System Implementation

1. **State Encapsulation**: Keep system state private and controlled
2. **Error Propagation**: Use `?` operator with `ToolError` for tool execution
3. **Status Clarity**: Provide clear, structured status information
4. **Documentation**: Document all tools and their effects

### Example Implementation

Here's a complete example of a simple system:

```rust
use goose_macros::tool;

struct FileSystem {
    registry: ToolRegistry,
    root_path: PathBuf,
}

impl FileSystem {
    #[tool(
        name = "read_file",
        description = "Read contents of a file"
    )]
    async fn read_file(&self, path: String) -> ToolResult<Value> {
        let full_path = self.root_path.join(path);
        let content = tokio::fs::read_to_string(full_path)
            .await
            .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
            
        Ok(json!({ "content": content }))
    }
}

#[async_trait]
impl System for FileSystem {
    // ... implement trait methods ...
}
```

## Testing

Systems should be tested at multiple levels:
1. Unit tests for individual tools
2. Integration tests for system behavior
3. Property tests for tool invariants

Example test:
```rust
#[tokio::test]
async fn test_echo_tool() {
    let system = TestSystem::new();
    let result = system.call_tool(
        "echo",
        hashmap!{ "message" => json!("hello") }
    ).await;
    
    assert_eq!(result.unwrap(), json!({ "response": "hello" }));
}
```
