use super::model::{
    Block, BlockKind, InlineFontFamily, InlineTextStyle, StyledBlockContent, TextStyleRange,
};

pub type MarkdownBlock = (BlockKind, String, bool);

pub fn parse(source: &str) -> (String, Vec<MarkdownBlock>) {
    parse_source(source, true)
}

/// Clipboard headings belong to the document body, including the first heading.
pub fn parse_blocks(source: &str) -> Vec<MarkdownBlock> {
    parse_source(source, false).1
}

pub fn parse_styled_blocks(source: &str) -> Vec<StyledBlockContent> {
    parse_blocks(source)
        .into_iter()
        .map(|(kind, content, checked)| {
            if kind == BlockKind::Code {
                (kind, content, checked, Vec::new())
            } else {
                let (plain, styles) = parse_inline_styles(&content);
                (kind, plain, checked, styles)
            }
        })
        .collect()
}

pub fn parse_inline_styles(source: &str) -> (String, Vec<TextStyleRange>) {
    let chars: Vec<char> = source.chars().collect();
    let mut plain = String::new();
    let mut plain_len = 0usize;
    let mut ranges = Vec::new();
    let mut index = 0usize;
    while index < chars.len() {
        if let Some((marker_len, end_idx, style)) = match_inline_token(&chars, index) {
            let inner: String = chars[index + marker_len..end_idx].iter().collect();
            let (inner_plain, inner_ranges) =
                if style.font_family == Some(InlineFontFamily::Monospace) {
                    (inner, Vec::new())
                } else {
                    parse_inline_styles(&inner)
                };
            let span_len = inner_plain.chars().count();
            if span_len > 0 {
                let start = plain_len;
                plain.push_str(&inner_plain);
                plain_len += span_len;
                ranges.push(TextStyleRange {
                    start,
                    end: plain_len,
                    style: style.clone(),
                });
                for mut nested in inner_ranges {
                    nested.start += start;
                    nested.end += start;
                    nested.style = merge_style(&style, &nested.style);
                    ranges.push(nested);
                }
            }
            index = end_idx + marker_len;
        } else {
            plain.push(chars[index]);
            plain_len += 1;
            index += 1;
        }
    }
    (plain, ranges)
}

fn merge_style(base: &InlineTextStyle, nested: &InlineTextStyle) -> InlineTextStyle {
    InlineTextStyle {
        bold: base.bold || nested.bold,
        italic: base.italic || nested.italic,
        underline: base.underline || nested.underline,
        strikethrough: base.strikethrough || nested.strikethrough,
        font_family: nested.font_family.or(base.font_family),
        foreground: nested.foreground.or(base.foreground),
        background: nested.background.or(base.background),
    }
}

fn match_inline_token(chars: &[char], start: usize) -> Option<(usize, usize, InlineTextStyle)> {
    const TOKENS: &[(&[char], InlineTextStyle)] = &[
        (
            &['*', '*', '*'],
            InlineTextStyle {
                bold: true,
                italic: true,
                underline: false,
                strikethrough: false,
                font_family: None,
                foreground: None,
                background: None,
            },
        ),
        (
            &['*', '*'],
            InlineTextStyle {
                bold: true,
                italic: false,
                underline: false,
                strikethrough: false,
                font_family: None,
                foreground: None,
                background: None,
            },
        ),
        (
            &['_', '_'],
            InlineTextStyle {
                bold: true,
                italic: false,
                underline: false,
                strikethrough: false,
                font_family: None,
                foreground: None,
                background: None,
            },
        ),
        (
            &['~', '~'],
            InlineTextStyle {
                bold: false,
                italic: false,
                underline: false,
                strikethrough: true,
                font_family: None,
                foreground: None,
                background: None,
            },
        ),
        (
            &['`'],
            InlineTextStyle {
                bold: false,
                italic: false,
                underline: false,
                strikethrough: false,
                font_family: Some(InlineFontFamily::Monospace),
                foreground: None,
                background: None,
            },
        ),
        (
            &['*'],
            InlineTextStyle {
                bold: false,
                italic: true,
                underline: false,
                strikethrough: false,
                font_family: None,
                foreground: None,
                background: None,
            },
        ),
    ];
    for (marker, style) in TOKENS {
        if chars[start..].starts_with(marker) {
            let after = start + marker.len();
            if after >= chars.len() || chars[after] == marker[0] || chars[after].is_whitespace() {
                continue;
            }
            if let Some(end) = find_closing(chars, after, marker) {
                return Some((marker.len(), end, style.clone()));
            }
        }
    }
    None
}

fn find_closing(chars: &[char], from: usize, marker: &[char]) -> Option<usize> {
    let mut index = from;
    while index + marker.len() <= chars.len() {
        if chars[index..].starts_with(marker) && !chars[index - 1].is_whitespace() {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn parse_source(source: &str, extract_title: bool) -> (String, Vec<MarkdownBlock>) {
    let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
    let mut title = String::new();
    let mut blocks = Vec::new();
    let mut code: Option<(usize, Vec<String>)> = None;
    for line in normalized.lines() {
        let fence_len = line.chars().take_while(|c| *c == '`').count();
        if let Some((opening_len, lines)) = &mut code {
            if fence_len >= *opening_len && line[fence_len..].trim().is_empty() {
                let (_, lines) = code.take().expect("code fence is open");
                blocks.push((BlockKind::Code, lines.join("\n"), false));
            } else {
                lines.push(line.to_owned());
            }
            continue;
        }
        if fence_len >= 3 {
            code = Some((fence_len, Vec::new()));
            continue;
        }
        if extract_title
            && blocks.is_empty()
            && title.is_empty()
            && let Some(value) = line.strip_prefix("# ")
        {
            title = value.to_owned();
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let (kind, content, checked) = if let Some(value) = line
            .strip_prefix("- [x] ")
            .or_else(|| line.strip_prefix("- [X] "))
        {
            (BlockKind::Todo, value, true)
        } else if let Some(value) = line.strip_prefix("- [ ] ") {
            (BlockKind::Todo, value, false)
        } else if let Some(value) = line.strip_prefix("### ") {
            (BlockKind::Heading3, value, false)
        } else if let Some(value) = line.strip_prefix("## ") {
            (BlockKind::Heading2, value, false)
        } else if let Some(value) = line.strip_prefix("# ") {
            (BlockKind::Heading1, value, false)
        } else if line.trim() == "---" {
            (BlockKind::Divider, "", false)
        } else if let Some(value) = line.strip_prefix("> [!NOTE] ") {
            (BlockKind::Callout, value, false)
        } else if let Some(value) = line.strip_prefix("> ") {
            (BlockKind::Quote, value, false)
        } else if let Some(value) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            (BlockKind::Bullet, value, false)
        } else if let Some((_, value)) = line
            .split_once(". ")
            .filter(|(prefix, _)| !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit()))
        {
            (BlockKind::Numbered, value, false)
        } else {
            (BlockKind::Text, line, false)
        };
        blocks.push((kind, content.to_owned(), checked));
    }
    if let Some((_, lines)) = code {
        blocks.push((BlockKind::Code, lines.join("\n"), false));
    }
    if blocks.is_empty() {
        blocks.push((BlockKind::Text, String::new(), false));
    }
    (title, blocks)
}

pub fn render_block(block: &Block, numbered_index: usize) -> String {
    let prefix = match block.kind {
        BlockKind::Heading1 => "# ",
        BlockKind::Heading2 => "## ",
        BlockKind::Heading3 => "### ",
        BlockKind::Bullet => "- ",
        BlockKind::Todo if block.checked => "- [x] ",
        BlockKind::Todo => "- [ ] ",
        BlockKind::Quote => "> ",
        BlockKind::Callout => "> [!NOTE] ",
        _ => "",
    };
    match block.kind {
        BlockKind::Code => {
            // Longer fences preserve code containing triple backticks.
            let longest = block
                .content
                .lines()
                .filter(|line| line.starts_with('`'))
                .map(|line| line.chars().take_while(|c| *c == '`').count())
                .max()
                .unwrap_or(0);
            let fence = "`".repeat((longest + 1).max(3));
            format!("{fence}\n{}\n{fence}\n\n", block.content)
        }
        BlockKind::Divider => "---\n\n".into(),
        BlockKind::PageLink => format!("[Page](noteon://page/{})\n\n", block.content),
        BlockKind::Numbered => format!("{numbered_index}. {}\n\n", block.content),
        BlockKind::Quote | BlockKind::Callout => {
            let continuation = block.content.replace('\n', "\n> ");
            format!("{prefix}{continuation}\n\n")
        }
        _ => format!("{prefix}{}\n\n", block.content),
    }
}
