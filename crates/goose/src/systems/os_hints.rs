use anyhow::Result as AnyhowResult;
use async_trait::async_trait;
use std::process::Command;

use crate::errors::{AgentError, AgentResult};
use crate::systems::System;
use mcp_core::{Content, Resource, Tool, ToolCall};

pub struct OsHintsSystem {
    instructions: String,
}

impl Default for OsHintsSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl OsHintsSystem {
    pub fn new() -> Self {
        let mut hints = Vec::new();

        // Detect OS
        let os_type = std::env::consts::OS;
        hints.push(format!("Operating System: {}", os_type));

        hints.push("Following are some of the installed SDKs which can be used additionally to make one off scripts to assist user tasks:".to_string());

        // Add OS-specific detection logic
        match os_type {
            "macos" => Self::detect_macos_tools(&mut hints),
            "linux" => Self::detect_linux_tools(&mut hints),
            "windows" => Self::detect_windows_tools(&mut hints),
            _ => hints.push("Unknown operating system".to_string()),
        }

        // Join all hints with newlines
        let instructions = hints.join("\n");

        Self { instructions }
    }

    fn detect_macos_tools(hints: &mut Vec<String>) {
        // Check for Homebrew
        if Command::new("brew").arg("--version").output().is_ok() {
            hints.push("Package Manager: Homebrew is installed".to_string());
        }

        // Check for Python
        if let Ok(output) = Command::new("python3").arg("--version").output() {
            if let Ok(version) = String::from_utf8(output.stdout) {
                hints.push(format!("has Python: {}", version.trim()));
            }
        }

        // Check for Node.js
        if let Ok(output) = Command::new("node").arg("--version").output() {
            if let Ok(version) = String::from_utf8(output.stdout) {
                hints.push(format!("has Node.js: {}", version.trim()));
            }
        }

        // Check for Rust
        if let Ok(output) = Command::new("rustc").arg("--version").output() {
            if let Ok(version) = String::from_utf8(output.stdout) {
                hints.push(format!("has Rust: {}", version.trim()));
            }
        }

        // Check for Go
        if let Ok(output) = Command::new("go").arg("version").output() {
            if let Ok(version) = String::from_utf8(output.stdout) {
                hints.push(format!("has Go: {}", version.trim()));
            }
        }

        // Check for Java
        if let Ok(output) = Command::new("java").arg("-version").output() {
            if let Ok(version) = String::from_utf8(output.stderr) {
                // Java outputs version to stderr
                hints.push(format!(
                    "has Java: {}",
                    version.lines().next().unwrap_or("").trim()
                ));
            }
        }

        // Check for Xcode Command Line Tools
        if Command::new("xcode-select")
            .arg("--print-path")
            .output()
            .is_ok()
        {
            hints.push("Xcode Command Line Tools are installed".to_string());
        }
        hints.push("You can use bash scripting on macos with common CLI tools.".to_string())
    }

    fn detect_linux_tools(hints: &mut Vec<String>) {
        // TODO: Implement Linux-specific detection
        hints.push("You can use shell scripting on linux with common CLI tools.".to_string())
    }

    fn detect_windows_tools(hints: &mut Vec<String>) {
        // TODO: Implement Windows-specific detection
        hints.push("Windows detection not yet implemented".to_string());
    }
}

#[async_trait]
impl System for OsHintsSystem {
    fn name(&self) -> &str {
        "OsHintsSystem"
    }

    fn description(&self) -> &str {
        "A system that provides context about the operating system and installed development tools."
    }

    fn instructions(&self) -> &str {
        &self.instructions
    }

    fn tools(&self) -> &[Tool] {
        &[]
    }

    async fn status(&self) -> AnyhowResult<Vec<Resource>> {
        Ok(Vec::new())
    }

    async fn call(&self, tool_call: ToolCall) -> AgentResult<Vec<Content>> {
        Err(AgentError::ToolNotFound(tool_call.name))
    }

    async fn read_resource(&self, _uri: &str) -> AgentResult<String> {
        Ok("".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_os_hints_system() {
        let system = OsHintsSystem::new();

        // Basic test to ensure we get some instructions
        assert!(!system.instructions().is_empty());

        // Verify OS detection
        assert!(system.instructions().contains("Operating System:"));
    }
}
