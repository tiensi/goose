use anyhow::Result;
use developer::DeveloperRouter;
use mcp_server::router::RouterService;
use mcp_server::{ByteTransport, Server};
use tokio::io::{stdin, stdout};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{self, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    // TODO cohesive logs strategy
    // Set up file appender for logging
    let file_appender = RollingFileAppender::new(Rotation::DAILY, "logs", "mcp-server.log");

    // Initialize the tracing subscriber with file and stdout logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .with_writer(file_appender)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    tracing::info!("Starting MCP server");

    // Create an instance of our counter router
    let router = RouterService(DeveloperRouter::new());

    // Create and run the server
    let server = Server::new(router);
    let transport = ByteTransport::new(stdin(), stdout());

    tracing::info!("Server initialized and ready to handle requests");
    Ok(server.run(transport).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::sync::OnceCell;

    static DEV_ROUTER: OnceCell<DeveloperRouter> = OnceCell::const_new();

    fn get_first_message_text(value: &Value) -> &str {
        let messages = value.get("messages").unwrap().as_array().unwrap();
        let first = messages.first().unwrap();
        first.get("text").unwrap().as_str().unwrap()
    }

    async fn get_router() -> &'static DeveloperRouter {
        DEV_ROUTER
            .get_or_init(|| async { DeveloperRouter::new() })
            .await
    }

    #[tokio::test]
    async fn test_bash_missing_parameters() {
        let router = get_router().await;
        let result = router.call_tool("bash", json!({})).await;

        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(matches!(err, ToolError::InvalidParameters(_)));
    }

    #[tokio::test]
    async fn test_bash_change_directory() {
        let router = get_router().await;
        let result = router
            .call_tool("bash", json!({ "working_dir": ".", "command": "pwd" }))
            .await;
        assert!(result.is_ok());
        let output = result.unwrap();
        // Check that the output contains the current directory
        assert!(output.get("messages").unwrap().as_array().unwrap().len() > 0);
        let text = get_first_message_text(&output);
        assert!(text.contains(&std::env::current_dir().unwrap().display().to_string()));
    }

    #[tokio::test]
    async fn test_bash_invalid_directory() {
        let router = get_router().await;
        let result = router
            .call_tool("bash", json!({ "working_dir": "non_existent_dir" }))
            .await;
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(matches!(err, ToolError::InvalidParameters(_)));
    }

    #[tokio::test]
    async fn test_text_editor_size_limits() {
        let router = get_router().await;
        let temp_dir = tempfile::tempdir().unwrap();

        // Test file size limit
        {
            let large_file_path = temp_dir.path().join("large.txt");
            let large_file_str = large_file_path.to_str().unwrap();

            // Create a file larger than 2MB
            let content = "x".repeat(3 * 1024 * 1024); // 3MB
            std::fs::write(&large_file_path, content).unwrap();

            let result = router
                .call_tool(
                    "text_editor",
                    json!({
                        "command": "view",
                        "path": large_file_str
                    }),
                )
                .await;

            assert!(result.is_err());
            let err = result.err().unwrap();
            assert!(matches!(err, ToolError::ExecutionError(_)));
            assert!(err.to_string().contains("too large"));
        }

        // Test character count limit
        {
            let many_chars_path = temp_dir.path().join("many_chars.txt");
            let many_chars_str = many_chars_path.to_str().unwrap();

            // Create a file with more than 2^20 characters but less than 2MB
            let content = "x".repeat((1 << 20) + 1); // 2^20 + 1 characters
            std::fs::write(&many_chars_path, content).unwrap();

            let result = router
                .call_tool(
                    "text_editor",
                    json!({
                        "command": "view",
                        "path": many_chars_str
                    }),
                )
                .await;

            assert!(result.is_err());
            let err = result.err().unwrap();
            assert!(matches!(err, ToolError::ExecutionError(_)));
            assert!(err.to_string().contains("too many characters"));
        }

        temp_dir.close().unwrap();
    }

    #[tokio::test]
    async fn test_text_editor_write_and_view_file() {
        let router = get_router().await;

        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let file_path_str = file_path.to_str().unwrap();

        // Create a new file
        router
            .call_tool(
                "text_editor",
                json!({
                    "command": "write",
                    "path": file_path_str,
                    "file_text": "Hello, world!"
                }),
            )
            .await
            .unwrap();

        // View the file
        let view_result = router
            .call_tool(
                "text_editor",
                json!({
                    "command": "view",
                    "path": file_path_str
                }),
            )
            .await
            .unwrap();

        assert!(
            view_result
                .get("messages")
                .unwrap()
                .as_array()
                .unwrap()
                .len()
                > 0
        );
        let text = get_first_message_text(&view_result);
        assert!(text.contains("The file content for"));

        temp_dir.close().unwrap();
    }

    #[tokio::test]
    async fn test_text_editor_str_replace() {
        let router = get_router().await;

        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let file_path_str = file_path.to_str().unwrap();

        // Create a new file
        router
            .call_tool(
                "text_editor",
                json!({
                    "command": "write",
                    "path": file_path_str,
                    "file_text": "Hello, world!"
                }),
            )
            .await
            .unwrap();

        // View the file to make it active
        router
            .call_tool(
                "text_editor",
                json!({
                    "command": "view",
                    "path": file_path_str
                }),
            )
            .await
            .unwrap();

        // Replace string
        let replace_result = router
            .call_tool(
                "text_editor",
                json!({
                    "command": "str_replace",
                    "path": file_path_str,
                    "old_str": "world",
                    "new_str": "Rust"
                }),
            )
            .await
            .unwrap();

        let text = get_first_message_text(&replace_result);
        assert!(text.contains("Successfully replaced text"));

        // View the file again
        let view_result = router
            .call_tool(
                "text_editor",
                json!({
                    "command": "view",
                    "path": file_path_str
                }),
            )
            .await
            .unwrap();

        let text = get_first_message_text(&view_result);
        assert!(text.contains("The file content for"));

        temp_dir.close().unwrap();
    }

    #[tokio::test]
    async fn test_read_resource() {
        let router = get_router().await;

        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let test_content = "Hello, world!";
        std::fs::write(&file_path, test_content).unwrap();

        let uri = Url::from_file_path(&file_path).unwrap().to_string();

        // Test text mime type with file:// URI
        {
            let mut active_resources = router.active_resources.lock().unwrap();
            let resource = Resource::new(uri.clone(), Some("text".to_string()), None).unwrap();
            active_resources.insert(uri.clone(), resource);
        }
        let content = router.read_resource(&uri).await.unwrap();
        assert_eq!(content, test_content);

        // Test blob mime type with file:// URI
        let blob_path = temp_dir.path().join("test.bin");
        let blob_content = b"Binary content";
        std::fs::write(&blob_path, blob_content).unwrap();
        let blob_uri = Url::from_file_path(&blob_path).unwrap().to_string();
        {
            let mut active_resources = router.active_resources.lock().unwrap();
            let resource = Resource::new(blob_uri.clone(), Some("blob".to_string()), None).unwrap();
            active_resources.insert(blob_uri.clone(), resource);
        }
        let encoded_content = router.read_resource(&blob_uri).await.unwrap();
        assert_eq!(
            base64::prelude::BASE64_STANDARD
                .decode(encoded_content)
                .unwrap(),
            blob_content
        );

        // Test str:// URI with text mime type
        let str_uri = format!("str:///{}", test_content);
        {
            let mut active_resources = router.active_resources.lock().unwrap();
            let resource = Resource::new(str_uri.clone(), Some("text".to_string()), None).unwrap();
            active_resources.insert(str_uri.clone(), resource);
        }
        let str_content = router.read_resource(&str_uri).await.unwrap();
        assert_eq!(str_content, test_content);

        // Test str:// URI with blob mime type (should fail)
        let str_blob_uri = format!("str:///{}", test_content);
        {
            let mut active_resources = router.active_resources.lock().unwrap();
            let resource =
                Resource::new(str_blob_uri.clone(), Some("blob".to_string()), None).unwrap();
            active_resources.insert(str_blob_uri.clone(), resource);
        }
        let error = router.read_resource(&str_blob_uri).await.unwrap_err();
        assert!(matches!(error, ResourceError::ExecutionError(_)));
        assert!(error.to_string().contains("only supports text mime type"));

        // Test invalid URI
        let error = router.read_resource("invalid://uri").await.unwrap_err();
        assert!(matches!(error, ResourceError::NotFound(_)));

        // Test file:// URI without registration
        let non_registered = Url::from_file_path(temp_dir.path().join("not_registered.txt"))
            .unwrap()
            .to_string();
        let error = router.read_resource(&non_registered).await.unwrap_err();
        assert!(matches!(error, ResourceError::NotFound(_)));

        // Test file:// URI with non-existent file but registered
        let non_existent = Url::from_file_path(temp_dir.path().join("non_existent.txt"))
            .unwrap()
            .to_string();
        {
            let mut active_resources = router.active_resources.lock().unwrap();
            let resource =
                Resource::new(non_existent.clone(), Some("text".to_string()), None).unwrap();
            active_resources.insert(non_existent.clone(), resource);
        }
        let error = router.read_resource(&non_existent).await.unwrap_err();
        assert!(matches!(error, ResourceError::NotFound(_)));
        assert!(error.to_string().contains("does not exist"));

        // Test invalid mime type
        let invalid_mime = Url::from_file_path(&file_path).unwrap().to_string();
        {
            let mut active_resources = router.active_resources.lock().unwrap();
            let mut resource =
                Resource::new(invalid_mime.clone(), Some("text".to_string()), None).unwrap();
            resource.mime_type = "invalid".to_string();
            active_resources.insert(invalid_mime.clone(), resource);
        }
        let error = router.read_resource(&invalid_mime).await.unwrap_err();
        assert!(matches!(error, ResourceError::ExecutionError(_)));
        assert!(error.to_string().contains("Unsupported mime type"));

        temp_dir.close().unwrap();
    }

    #[tokio::test]
    async fn test_text_editor_undo_edit() {
        let router = get_router().await;

        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let file_path_str = file_path.to_str().unwrap();

        // Create a new file
        router
            .call_tool(
                "text_editor",
                json!({
                    "command": "write",
                    "path": file_path_str,
                    "file_text": "First line"
                }),
            )
            .await
            .unwrap();

        // View the file to make it active
        router
            .call_tool(
                "text_editor",
                json!({
                    "command": "view",
                    "path": file_path_str
                }),
            )
            .await
            .unwrap();

        // Replace string
        router
            .call_tool(
                "text_editor",
                json!({
                    "command": "str_replace",
                    "path": file_path_str,
                    "old_str": "First line",
                    "new_str": "Second line"
                }),
            )
            .await
            .unwrap();

        // Undo the edit
        let undo_result = router
            .call_tool(
                "text_editor",
                json!({
                    "command": "undo_edit",
                    "path": file_path_str
                }),
            )
            .await
            .unwrap();

        let text = get_first_message_text(&undo_result);
        assert!(text.contains("Undid the last edit"));

        // View the file again
        let view_result = router
            .call_tool(
                "text_editor",
                json!({
                    "command": "view",
                    "path": file_path_str
                }),
            )
            .await
            .unwrap();

        let text = get_first_message_text(&view_result);
        assert!(text.contains("The file content for"));

        temp_dir.close().unwrap();
    }
}
