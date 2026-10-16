use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkType {
    Web { url: String },
    FilePath { path: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectedLink {
    pub char_range: Range<usize>,
    pub link_type: LinkType,
    pub raw_text: String,
}

pub fn is_windows_drive_path(s: &str) -> bool {
    let mut chars = s.chars();
    if let (Some(drive), Some(colon), Some(slash)) = (chars.next(), chars.next(), chars.next()) {
        drive.is_ascii_alphabetic() && colon == ':' && (slash == '\\' || slash == '/')
    } else {
        false
    }
}

pub fn is_file_path(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return false;
    }

    if is_windows_drive_path(trimmed) {
        return true;
    }

    if trimmed.starts_with(r"\\") && trimmed.len() > 3 && trimmed[2..].contains('\\') {
        return true;
    }

    if trimmed.starts_with("file:///") || trimmed.starts_with("file://") {
        return true;
    }

    false
}

pub fn normalize_file_path(raw: &str) -> String {
    let clean = raw.trim().trim_matches('"').trim_matches('\'');
    if let Some(stripped) = clean.strip_prefix("file:///") {
        stripped.to_string()
    } else if let Some(stripped) = clean.strip_prefix("file://") {
        stripped.to_string()
    } else {
        clean.to_string()
    }
}

pub fn normalize_web_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else if trimmed.starts_with("www.") {
        format!("https://{trimmed}")
    } else {
        trimmed.to_string()
    }
}

pub fn classify_link(s: &str) -> Option<LinkType> {
    let trimmed = s.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") || trimmed.starts_with("www.") {
        return Some(LinkType::Web {
            url: normalize_web_url(trimmed),
        });
    }

    if is_file_path(trimmed) {
        return Some(LinkType::FilePath {
            path: normalize_file_path(trimmed),
        });
    }

    None
}

pub fn parse_token_link(raw_token: &str) -> Option<(&str, usize, LinkType)> {
    let mut token = raw_token;
    let mut leading_trim_len = 0;

    while let Some(ch) = token.chars().next() {
        if matches!(ch, '(' | '[' | '{' | '<' | '\'' | '"') {
            token = &token[ch.len_utf8()..];
            leading_trim_len += ch.len_utf8();
        } else {
            break;
        }
    }

    if token.is_empty() {
        return None;
    }

    let mut clean = token;
    loop {
        if clean.is_empty() {
            break;
        }
        let last_ch = clean.chars().last().unwrap();
        if last_ch == ')' {
            let open_count = clean.chars().filter(|c| *c == '(').count();
            let close_count = clean.chars().filter(|c| *c == ')').count();
            if close_count > open_count {
                clean = &clean[..clean.len() - last_ch.len_utf8()];
                continue;
            }
            break;
        }
        if matches!(
            last_ch,
            ',' | '.' | ';' | ':' | '!' | '?' | ']' | '}' | '>' | '\'' | '"'
        ) {
            clean = &clean[..clean.len() - last_ch.len_utf8()];
            continue;
        }
        break;
    }

    if clean.is_empty() {
        return None;
    }

    classify_link(clean).map(|link_type| (clean, leading_trim_len, link_type))
}

pub fn find_links_and_paths(text: &str) -> Vec<DetectedLink> {
    if text.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::new();
    let char_indices: Vec<(usize, char)> = text.char_indices().collect();
    let total_chars = char_indices.len();

    // 1. Markdown bağlantıları: [metin](hedef)
    let mut i = 0;
    while i < total_chars {
        if char_indices[i].1 == '[' {
            let start_char = i;
            let mut close_bracket = None;
            let mut j = i + 1;
            while j < total_chars {
                if char_indices[j].1 == ']' {
                    close_bracket = Some(j);
                    break;
                }
                if char_indices[j].1 == '\n' {
                    break;
                }
                j += 1;
            }

            if let Some(cb) = close_bracket {
                if cb + 1 < total_chars && char_indices[cb + 1].1 == '(' {
                    let mut close_paren = None;
                    let mut k = cb + 2;
                    while k < total_chars {
                        if char_indices[k].1 == ')' {
                            close_paren = Some(k);
                            break;
                        }
                        if char_indices[k].1 == '\n' {
                            break;
                        }
                        k += 1;
                    }

                    if let Some(cp) = close_paren {
                        let target_byte_start = char_indices[cb + 2].0;
                        let target_byte_end = char_indices[cp].0;
                        let target = &text[target_byte_start..target_byte_end].trim();

                        if let Some(link_type) = classify_link(target) {
                            let raw_byte_start = char_indices[start_char].0;
                            let raw_byte_end = if cp + 1 < total_chars {
                                char_indices[cp + 1].0
                            } else {
                                text.len()
                            };
                            results.push(DetectedLink {
                                char_range: start_char..cp + 1,
                                link_type,
                                raw_text: text[raw_byte_start..raw_byte_end].to_string(),
                            });
                            i = cp + 1;
                            continue;
                        }
                    }
                }
            }
        }
        i += 1;
    }

    let is_in_existing = |char_idx: usize, res: &[DetectedLink]| -> bool {
        res.iter().any(|r| r.char_range.contains(&char_idx))
    };

    // 2. Tırnaklı dosya yolları ve tokenler
    let mut idx = 0;
    while idx < total_chars {
        if is_in_existing(idx, &results) {
            idx += 1;
            continue;
        }

        let ch = char_indices[idx].1;

        if ch == '"' || ch == '\'' {
            let quote_char = ch;
            let start_char = idx;
            let mut end_char = None;
            let mut j = idx + 1;
            while j < total_chars {
                if char_indices[j].1 == quote_char {
                    end_char = Some(j);
                    break;
                }
                if char_indices[j].1 == '\n' {
                    break;
                }
                j += 1;
            }

            if let Some(quote_end) = end_char {
                let inner_byte_start = char_indices[start_char + 1].0;
                let inner_byte_end = char_indices[quote_end].0;
                let inner = &text[inner_byte_start..inner_byte_end];
                if is_file_path(inner) {
                    let raw_byte_start = char_indices[start_char].0;
                    let raw_byte_end = if quote_end + 1 < total_chars {
                        char_indices[quote_end + 1].0
                    } else {
                        text.len()
                    };
                    results.push(DetectedLink {
                        char_range: start_char..quote_end + 1,
                        link_type: LinkType::FilePath {
                            path: normalize_file_path(inner),
                        },
                        raw_text: text[raw_byte_start..raw_byte_end].to_string(),
                    });
                    idx = quote_end + 1;
                    continue;
                }
            }
        }

        if !ch.is_whitespace() {
            let start_scan = idx;
            let mut end_scan = idx;
            while end_scan < total_chars
                && !char_indices[end_scan].1.is_whitespace()
                && !is_in_existing(end_scan, &results)
            {
                end_scan += 1;
            }

            let token_byte_start = char_indices[start_scan].0;
            let token_byte_end = if end_scan < total_chars {
                char_indices[end_scan].0
            } else {
                text.len()
            };
            let raw_token = &text[token_byte_start..token_byte_end];

            if let Some((clean_token, leading_trim_bytes, link_type)) = parse_token_link(raw_token) {
                let leading_chars = raw_token[..leading_trim_bytes].chars().count();
                let actual_start_char = start_scan + leading_chars;
                let actual_end_char = actual_start_char + clean_token.chars().count();

                results.push(DetectedLink {
                    char_range: actual_start_char..actual_end_char,
                    link_type,
                    raw_text: clean_token.to_string(),
                });
                idx = actual_end_char;
                continue;
            }

            idx = end_scan;
            continue;
        }

        idx += 1;
    }

    results.sort_by_key(|r| r.char_range.start);
    results
}

pub fn open_url_in_browser(url: &str) {
    let target = normalize_web_url(url);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", &target])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }

    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(&target).spawn();
    }
}

pub fn open_in_file_explorer(raw_path: &str) {
    let clean = normalize_file_path(raw_path);
    let path = std::path::Path::new(&clean);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let mut cmd = std::process::Command::new("explorer");
        if path.is_file() {
            cmd.arg(format!("/select,{}", path.display()));
        } else if path.is_dir() {
            cmd.arg(path.as_os_str());
        } else if let Some(parent) = path.parent().filter(|p| p.is_dir()) {
            cmd.arg(parent.as_os_str());
        } else {
            cmd.arg(path.as_os_str());
        }
        cmd.creation_flags(CREATE_NO_WINDOW);
        let _ = cmd.spawn();
    }

    #[cfg(not(windows))]
    {
        let target = if path.is_file() {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        let _ = std::process::Command::new("xdg-open").arg(target).spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_link_detection() {
        let text = "Web sitemiz https://noteon.local ve www.example.com/test dir.";
        let links = find_links_and_paths(text);
        assert_eq!(links.len(), 2);
        assert_eq!(
            links[0].link_type,
            LinkType::Web {
                url: "https://noteon.local".to_string()
            }
        );
        assert_eq!(
            links[1].link_type,
            LinkType::Web {
                url: "https://www.example.com/test".to_string()
            }
        );
    }

    #[test]
    fn test_file_path_detection() {
        let text = "Dosyalar C:\\Users\\kargalar\\Documents ve D:/Projeler/noteon yolunda.";
        let links = find_links_and_paths(text);
        assert_eq!(links.len(), 2);
        assert_eq!(
            links[0].link_type,
            LinkType::FilePath {
                path: "C:\\Users\\kargalar\\Documents".to_string()
            }
        );
        assert_eq!(
            links[1].link_type,
            LinkType::FilePath {
                path: "D:/Projeler/noteon".to_string()
            }
        );
    }

    #[test]
    fn test_quoted_file_path() {
        let text = "Bakınız \"C:\\Program Files\\Noteon\\app.exe\" burada.";
        let links = find_links_and_paths(text);
        assert_eq!(links.len(), 1);
        assert_eq!(
            links[0].link_type,
            LinkType::FilePath {
                path: "C:\\Program Files\\Noteon\\app.exe".to_string()
            }
        );
    }

    #[test]
    fn test_markdown_links() {
        let text = "Metin [Google](https://google.com) ve [Notlar](C:\\Notlar) içerir.";
        let links = find_links_and_paths(text);
        assert_eq!(links.len(), 2);
        assert_eq!(
            links[0].link_type,
            LinkType::Web {
                url: "https://google.com".to_string()
            }
        );
        assert_eq!(
            links[1].link_type,
            LinkType::FilePath {
                path: "C:\\Notlar".to_string()
            }
        );
    }
}

