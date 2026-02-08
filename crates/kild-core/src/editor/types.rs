use serde::{Deserialize, Serialize};

/// Supported editor types.
///
/// Each variant represents a known editor with its own backend implementation.
/// Unknown editors fall back to `GenericBackend` (not represented here since
/// it's dynamically constructed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorType {
    Zed,
    VSCode,
    Vim,
}

impl EditorType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EditorType::Zed => "zed",
            EditorType::VSCode => "code",
            EditorType::Vim => "vim",
        }
    }
}

impl std::fmt::Display for EditorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for EditorType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "zed" => Ok(EditorType::Zed),
            "code" | "vscode" => Ok(EditorType::VSCode),
            "vim" | "nvim" | "neovim" | "helix" => Ok(EditorType::Vim),
            _ => Err(format!(
                "Unknown editor '{}'. Known editors: zed, code, vim",
                s
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_editor_type_as_str() {
        assert_eq!(EditorType::Zed.as_str(), "zed");
        assert_eq!(EditorType::VSCode.as_str(), "code");
        assert_eq!(EditorType::Vim.as_str(), "vim");
    }

    #[test]
    fn test_editor_type_display() {
        assert_eq!(format!("{}", EditorType::Zed), "zed");
        assert_eq!(format!("{}", EditorType::VSCode), "code");
        assert_eq!(format!("{}", EditorType::Vim), "vim");
    }

    #[test]
    fn test_editor_type_from_str_case_insensitive() {
        use std::str::FromStr;
        assert_eq!(EditorType::from_str("zed"), Ok(EditorType::Zed));
        assert_eq!(EditorType::from_str("ZED"), Ok(EditorType::Zed));
        assert_eq!(EditorType::from_str("Zed"), Ok(EditorType::Zed));

        assert_eq!(EditorType::from_str("code"), Ok(EditorType::VSCode));
        assert_eq!(EditorType::from_str("vscode"), Ok(EditorType::VSCode));
        assert_eq!(EditorType::from_str("VSCode"), Ok(EditorType::VSCode));

        assert_eq!(EditorType::from_str("vim"), Ok(EditorType::Vim));
        assert_eq!(EditorType::from_str("nvim"), Ok(EditorType::Vim));
        assert_eq!(EditorType::from_str("neovim"), Ok(EditorType::Vim));
        assert_eq!(EditorType::from_str("helix"), Ok(EditorType::Vim));

        assert!(EditorType::from_str("unknown").is_err());
        assert!(EditorType::from_str("").is_err());
    }

    #[test]
    fn test_editor_type_from_str_error_message() {
        use std::str::FromStr;
        let err = EditorType::from_str("unknown").unwrap_err();
        assert!(err.contains("Unknown editor 'unknown'"));
        assert!(err.contains("zed"));
    }

    #[test]
    fn test_editor_type_serde() {
        let zed = EditorType::Zed;
        let json = serde_json::to_string(&zed).unwrap();
        assert_eq!(json, "\"zed\"");

        let parsed: EditorType = serde_json::from_str("\"zed\"").unwrap();
        assert_eq!(parsed, EditorType::Zed);

        let parsed: EditorType = serde_json::from_str("\"vscode\"").unwrap();
        assert_eq!(parsed, EditorType::VSCode);

        let parsed: EditorType = serde_json::from_str("\"vim\"").unwrap();
        assert_eq!(parsed, EditorType::Vim);
    }

    #[test]
    fn test_editor_type_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(EditorType::Zed);
        set.insert(EditorType::Zed);
        assert_eq!(set.len(), 1);

        set.insert(EditorType::VSCode);
        set.insert(EditorType::Vim);
        assert_eq!(set.len(), 3);
    }
}
