use std::ops::Range;

use super::model::BlockId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PageFindMatch {
    Title(Range<usize>),
    Block {
        position: usize,
        block_id: BlockId,
        range: Range<usize>,
    },
}

pub fn find_char_ranges(text: &str, query: &str) -> Vec<Range<usize>> {
    let query: Vec<char> = query.trim().chars().collect();
    if query.is_empty() || text.is_empty() {
        return Vec::new();
    }
    let text: Vec<char> = text.chars().collect();
    let mut ranges = Vec::new();
    let mut start = 0;
    while start + query.len() <= text.len() {
        let end = start + query.len();
        if text[start..end]
            .iter()
            .zip(&query)
            .all(|(&a, &b)| char_eq_case_insensitive(a, b))
        {
            ranges.push(start..end);
            start = end;
        } else {
            start += 1;
        }
    }
    ranges
}

fn char_eq_case_insensitive(a: char, b: char) -> bool {
    if a == b || a.eq_ignore_ascii_case(&b) {
        return true;
    }
    match (a, b) {
        ('İ', 'i' | 'I') | ('i' | 'I', 'İ') | ('I', 'ı') | ('ı', 'I') => true,
        _ => a.to_lowercase().eq(b.to_lowercase()),
    }
}
