use std::path::PathBuf;

pub fn home_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
}

pub fn expand_home(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        home_dir().join(rest)
    } else {
        PathBuf::from(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_dir_not_empty() {
        let home = home_dir();
        assert!(!home.as_os_str().is_empty());
    }

    #[test]
    fn expand_home_tilde_prefix() {
        let result = expand_home("~/Documents");
        let home = home_dir();
        assert_eq!(result, home.join("Documents"));
    }

    #[test]
    fn expand_home_absolute_unchanged() {
        let result = expand_home("/etc/config");
        assert_eq!(result, PathBuf::from("/etc/config"));
    }

    #[test]
    fn expand_home_relative_unchanged() {
        let result = expand_home("relative/path");
        assert_eq!(result, PathBuf::from("relative/path"));
    }

    #[test]
    fn expand_home_tilde_only_not_expanded() {
        let result = expand_home("~notslash");
        assert_eq!(result, PathBuf::from("~notslash"));
    }

    #[test]
    fn expand_home_empty() {
        let result = expand_home("");
        assert_eq!(result, PathBuf::from(""));
    }
}
