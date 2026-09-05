//! Port of the reference crawler `pkg/utils/extensions` — file extension allow/deny validator.

/// the reference crawler's default denylist of extensions to skip while crawling.
pub const DEFAULT_EXT_FILTER: &[&str] = &[
    ".3g2", ".3gp", ".7z", ".apk", ".arj", ".avi", ".axd", ".bmp", ".csv", ".deb", ".dll", ".doc",
    ".drv", ".eot", ".exe", ".flv", ".gif", ".gifv", ".gz", ".h264", ".ico", ".iso", ".jar",
    ".jpeg", ".jpg", ".lock", ".m4a", ".m4v", ".map", ".mkv", ".mov", ".mp3", ".mp4", ".mpeg",
    ".mpg", ".msi", ".ogg", ".ogm", ".ogv", ".otf", ".pdf", ".pkg", ".png", ".ppt", ".psd", ".rar",
    ".rm", ".rpm", ".svg", ".swf", ".sys", ".tar.gz", ".tar", ".tif", ".tiff", ".ttf", ".txt",
    ".vob", ".wav", ".webm", ".webp", ".wmv", ".woff", ".woff2", ".xcf", ".xls", ".xlsx", ".zip",
];

/// Validator for URL file extensions (reference crawler `extensions.Validator`).
///
/// When `extensions_match` is non-empty only those extensions are allowed.
/// Otherwise any extension not present in the filter list is allowed.
#[derive(Debug, Clone, Default)]
pub struct ExtensionValidator {
    extensions_match: std::collections::HashSet<String>,
    extensions_filter: std::collections::HashSet<String>,
}

impl ExtensionValidator {
    pub fn new(
        extensions_match: &[String],
        extensions_filter: &[String],
        no_default_ext_filter: bool,
    ) -> Self {
        let mut validator = ExtensionValidator::default();
        for e in extensions_match {
            validator.extensions_match.insert(normalize_extension(e));
        }
        if !no_default_ext_filter {
            for e in DEFAULT_EXT_FILTER {
                validator.extensions_filter.insert((*e).to_string());
            }
        }
        for e in extensions_filter {
            validator.extensions_filter.insert(normalize_extension(e));
        }
        validator
    }

    /// Returns true if the URL's extension is allowed by the validator
    /// (reference crawler `Validator.ValidatePath`).
    pub fn validate_path(&self, item: &str) -> bool {
        let path = url::Url::parse(item)
            .map(|u| u.path().to_string())
            .unwrap_or_else(|_| item.to_string());

        let extension = std::path::Path::new(&path)
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
            .unwrap_or_default();

        if extension.is_empty() {
            if !self.extensions_match.is_empty() {
                return self.extensions_match.contains("");
            }
            return true;
        }

        if !self.extensions_match.is_empty() {
            return self.extensions_match.contains(&extension);
        }

        !self.extensions_filter.contains(&extension)
    }
}

/// Normalize an extension to lowercase with a leading dot; `none` maps to ""
/// (reference crawler `normalizeExtension`).
pub fn normalize_extension(extension: &str) -> String {
    let e = extension.to_lowercase();
    if e == "none" {
        return String::new();
    }
    if e.starts_with('.') {
        e
    } else {
        format!(".{e}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sv(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_default_filter_blocks_images() {
        let v = ExtensionValidator::new(&[], &[], false);
        assert!(!v.validate_path("https://example.com/image.png"));
        assert!(!v.validate_path("https://example.com/archive.tar.gz"));
        assert!(v.validate_path("https://example.com/page.php"));
        assert!(v.validate_path("https://example.com/page"));
    }

    #[test]
    fn test_extension_match_allows_only_listed() {
        let v = ExtensionValidator::new(&sv(&["php", "html"]), &[], false);
        assert!(v.validate_path("https://example.com/index.php"));
        assert!(v.validate_path("https://example.com/index.html"));
        assert!(!v.validate_path("https://example.com/index.js"));
    }

    #[test]
    fn test_extension_match_none() {
        // "none" matches URLs without an extension
        let v = ExtensionValidator::new(&sv(&["none"]), &[], false);
        assert!(v.validate_path("https://example.com/path/page"));
        assert!(!v.validate_path("https://example.com/page.js"));
    }

    #[test]
    fn test_no_default_filter() {
        let v = ExtensionValidator::new(&[], &[], true);
        assert!(v.validate_path("https://example.com/image.png"));
    }

    #[test]
    fn test_custom_filter() {
        let v = ExtensionValidator::new(&[], &sv(&["css"]), false);
        assert!(!v.validate_path("https://example.com/style.css"));
    }

    #[test]
    fn test_normalize() {
        assert_eq!(normalize_extension("PHP"), ".php");
        assert_eq!(normalize_extension(".js"), ".js");
        assert_eq!(normalize_extension("none"), "");
    }
}
