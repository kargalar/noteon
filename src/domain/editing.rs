use std::ops::Range;

/// egui cursor positions count Unicode scalar values; String slices count bytes.
/// Enter removes the selection and splits the remaining text without corrupting UTF-8.
pub fn split_at_selection(content: &str, selection: Range<usize>) -> (String, String) {
    let byte_offset = |character| {
        content
            .char_indices()
            .nth(character)
            .map(|(byte, _)| byte)
            .unwrap_or(content.len())
    };
    let start = byte_offset(selection.start);
    let end = byte_offset(selection.end.max(selection.start));
    (content[..start].to_owned(), content[end..].to_owned())
}
