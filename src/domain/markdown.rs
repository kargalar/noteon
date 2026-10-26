use super::model::{Block, BlockKind};

pub type MarkdownBlock = (BlockKind, String, bool);

pub fn parse(source: &str) -> (String, Vec<MarkdownBlock>) {
    parse_source(source, true)
}

/// Clipboard headings belong to the document body, including the first heading.
pub fn parse_blocks(source: &str) -> Vec<MarkdownBlock> {
    parse_source(source, false).1
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
