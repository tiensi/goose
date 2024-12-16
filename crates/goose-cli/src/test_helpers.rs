/// Helper function to set up a temporary home directory for testing, returns path of that temp dir.
/// Also creates a default profiles.json to avoid obscure test failures when there are no profiles.
#[cfg(test)]
pub fn run_with_tmp_dir<F: FnOnce() -> T, T>(func: F) -> T {
    use std::ffi::OsStr;
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let temp_dir_path = temp_dir.path().to_path_buf();
    setup_profile(&temp_dir_path);

    temp_env::with_vars(
        [
            ("HOME", Some(temp_dir_path.as_os_str())),
            ("DATABRICKS_HOST", Some(OsStr::new("tmp_host_url"))),
        ],
        func,
    )
}

#[cfg(test)]
pub async fn run_with_tmp_dir_async<F, Fut, T>(func: F) -> T
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    use std::ffi::OsStr;
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let temp_dir_path = temp_dir.path().to_path_buf();
    setup_profile(&temp_dir_path);

    temp_env::async_with_vars(
        [
            ("HOME", Some(temp_dir_path.as_os_str())),
            ("DATABRICKS_HOST", Some(OsStr::new("tmp_host_url"))),
        ],
        func(),
    )
    .await
}

#[cfg(test)]
use std::path::PathBuf;
#[cfg(test)]
fn setup_profile(temp_dir_path: &PathBuf) {
    use std::fs;

    let profile_path = temp_dir_path
        .join(".config")
        .join("goose")
        .join("profiles.json");
    fs::create_dir_all(profile_path.parent().unwrap()).unwrap();
    let profile = r#"
{
    "profile_items": {
        "default": {
            "provider": "databricks",
            "model": "claude-3-5-sonnet-2",
            "additional_systems": []
        }
    }
}"#;
    fs::write(&profile_path, profile).unwrap();
}
