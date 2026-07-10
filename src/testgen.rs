//! Shared hegel generators for tests across modules.

use hegel::generators;

/// Reserved words that the identifier regex can produce but Python rejects
/// as names.
pub const PYTHON_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

#[hegel::composite]
pub fn identifier(tc: hegel::TestCase) -> String {
    let s = tc.draw(generators::from_regex("[a-zA-Z_][a-zA-Z0-9_]{0,10}").fullmatch(true));
    tc.assume(!PYTHON_KEYWORDS.contains(&s.as_str()));
    s
}

#[hegel::composite]
pub fn module_path(tc: hegel::TestCase) -> String {
    let depth = tc.draw(generators::integers::<usize>().min_value(1).max_value(4));
    let segments: Vec<String> = (0..depth).map(|_| tc.draw(identifier())).collect();
    segments.join(".")
}
