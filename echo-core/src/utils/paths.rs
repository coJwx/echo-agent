//! Filesystem path helpers shared across workspace crates.

use std::path::PathBuf;

/// Return the user-level Echo Agent root directory.
///
/// Resolution order:
/// 1. `ROOT_AGENT_DIR`
/// 2. `$HOME/.echo-agent`
/// 3. `%USERPROFILE%/.echo-agent`
/// 4. `~/.echo-agent`
pub fn root_agent_dir() -> PathBuf {
    if let Ok(path) = std::env::var("ROOT_AGENT_DIR")
        && !path.trim().is_empty()
    {
        return expand_home(PathBuf::from(path));
    }

    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(|home| PathBuf::from(home).join(".echo-agent"))
        .unwrap_or_else(|_| PathBuf::from("~/.echo-agent"))
}

/// Return a file path under [`root_agent_dir`].
pub fn root_agent_file(file_name: &str) -> PathBuf {
    root_agent_dir().join(file_name)
}

fn expand_home(path: PathBuf) -> PathBuf {
    let Some(s) = path.to_str() else {
        return path;
    };

    if s == "~" {
        return home_dir().unwrap_or(path);
    }

    if let Some(rest) = s.strip_prefix("~/").or_else(|| s.strip_prefix("~\\")) {
        if let Some(home) = home_dir() {
            return home.join(rest);
        }
    }

    path
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .filter(|home| !home.trim().is_empty())
        .map(PathBuf::from)
}
