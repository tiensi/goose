use mcp_core::{Result, Tool};
use mcp_macros::tool;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Create an instance of our tool
    let calculator = Calculator::default();

    // Print tool information
    println!("Tool name: {}", Calculator::name());
    println!("Tool description: {}", Calculator::description());
    println!("Tool schema: {}", Calculator::schema());

    // Test the tool with some sample input
    let input = serde_json::json!({
        "x": 5,
        "y": 3,
        "operation": "multiply"
    });

    let result = calculator.call(input).await?;
    println!("Result: {}", result);

    Ok(())
}

#[tool(
    name = "calculator",
    description = "Perform basic arithmetic operations",
    params(
        x = "First number in the calculation",
        y = "Second number in the calculation",
        operation = "The operation to perform (add, subtract, multiply, divide)"
    )
)]
async fn calculator(x: i32, y: i32, operation: String) -> Result<i32> {
    match operation.as_str() {
        "add" => Ok(x + y),
        "subtract" => Ok(x - y),
        "multiply" => Ok(x * y),
        "divide" => {
            if y == 0 {
                Err(mcp_core::ToolError::ExecutionError(
                    "Division by zero".into(),
                ))
            } else {
                Ok(x / y)
            }
        }
        _ => Err(mcp_core::ToolError::InvalidParameters(format!(
            "Unknown operation: {}",
            operation
        ))),
    }
}
