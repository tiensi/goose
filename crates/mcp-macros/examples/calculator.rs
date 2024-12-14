use mcp_core::{Result, Tool};
use mcp_macros::tool;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Create an instance of our tool
    let calculator = Add::default();

    // Print tool information
    println!("Tool name: {}", Add::name());
    println!("Tool description: {}", Add::description());
    println!("Tool schema: {}", Add::schema());

    // Test the tool with some sample input
    let input = serde_json::json!({
        "a": 5,
        "b": 3
    });

    let result = calculator.call(input).await?;
    println!("Result: {}", result);

    Ok(())
}

#[tool(name = "add", description = "Add two numbers together")]
async fn add(a: i32, b: i32) -> Result<i32> {
    Ok(a + b)
}
