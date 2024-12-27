use anyhow::Result;
use clap::Args;
use goose::agents::AgentFactory;
use std::fmt::Write;

#[derive(Args)]
pub struct AgentCommand {
    /// List available agent versions
    #[arg(short, long)]
    list: bool,
}

impl AgentCommand {
    pub fn run(&self) -> Result<()> {
        if self.list {
            let mut output = String::new();
            writeln!(output, "Available agent versions:")?;
            
            let versions = AgentFactory::available_versions();
            let default_version = AgentFactory::default_version();
            
            for version in versions {
                if version == default_version {
                    writeln!(output, "* {} (default)", version)?;
                } else {
                    writeln!(output, "  {}", version)?;
                }
            }
            
            print!("{}", output);
        } else {
            // When no flags are provided, show the default version
            println!("Default version: {}", AgentFactory::default_version());
        }
        Ok(())
    }
}