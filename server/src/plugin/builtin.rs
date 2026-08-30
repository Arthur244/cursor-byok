//! Materializes built-in plugins bundled in the binary into the managed dir.
use std::path::PathBuf;

use super::definition::write_if_changed;
use crate::{config, Result};

/// 随二进制打包的内置插件文件;发布构建没有源码目录,靠这里落盘。
const CODEX_AUTH: &[(&str, &str)] = &[
    (
        "plugin.json",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/plugins/build-in/codex-auth/plugin.json"
        )),
    ),
    (
        "main.ts",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/plugins/build-in/codex-auth/main.ts"
        )),
    ),
    (
        "provider.ts",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/plugins/build-in/codex-auth/provider.ts"
        )),
    ),
    (
        "models.ts",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/plugins/build-in/codex-auth/models.ts"
        )),
    ),
    (
        "oauth.ts",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/plugins/build-in/codex-auth/oauth.ts"
        )),
    ),
    (
        "resources.ts",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/plugins/build-in/codex-auth/resources.ts"
        )),
    ),
    (
        "assets/codex.svg",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/plugins/build-in/codex-auth/assets/codex.svg"
        )),
    ),
];

/// 把内置插件写入受管目录并返回该目录,作为插件目录的扫描根之一。
pub(super) fn materialize() -> Result<PathBuf> {
    let root = config::managed_data_dir()?.join("plugins/build-in");
    write_plugin(&root.join("codex-auth"), CODEX_AUTH)?;
    Ok(root)
}

fn write_plugin(directory: &std::path::Path, files: &[(&str, &str)]) -> Result<()> {
    for (relative, content) in files {
        let path = directory.join(relative);
        let parent = path.parent().expect("plugin file path has a parent");
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
        write_if_changed(&path, content)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
    }
    Ok(())
}
