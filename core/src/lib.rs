use directories::ProjectDirs;
use std::{fs::create_dir_all, sync::OnceLock};

pub mod auth;
pub mod grpc;
pub mod tls;

pub fn project_dir() -> &'static ProjectDirs {
    static PROJECT_DIR: OnceLock<ProjectDirs> = OnceLock::new();

    PROJECT_DIR.get_or_init(|| {
        ProjectDirs::from("", "", "rrp").expect("failed to retreive home directory path")
    })
}

// attempts to generate all of the project directories
pub fn setup_project_dir() -> std::io::Result<()> {
    let pd = project_dir();
    create_dir_all(pd.config_dir())?;

    Ok(())
}
