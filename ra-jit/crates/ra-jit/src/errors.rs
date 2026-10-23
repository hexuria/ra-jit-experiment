//! Errors in the source, written as rustc writes them: what is wrong, where, and the line
//! with what is wrong marked under it.

use syntax::TextRange;

/// An error at `range` of `text`, the file shown as `path`. `code` is rustc's, as `E0308`,
/// for the ones that have one.
pub(crate) fn at(
    code: Option<&str>,
    message: &str,
    path: &str,
    text: &str,
    range: TextRange,
) -> String {
    let start = floor(text, usize::from(range.start()));
    let end = floor(text, usize::from(range.end())).max(start);
    let line_start = text[..start].rfind('\n').map_or(0, |it| it + 1);
    let line_end = text[start..].find('\n').map_or(text.len(), |it| start + it);
    let number = text[..start].matches('\n').count() + 1;
    let column = text[line_start..start].chars().count() + 1;
    let source = text[line_start..line_end].trim_end_matches('\r');
    // rustc shows a tab as four spaces, and marks what is under it to match.
    let width = |it: &str| it.chars().map(|c| if c == '\t' { 4 } else { 1 }).sum::<usize>();
    let before = width(&text[line_start..start]);
    let marked = width(&text[start..end.min(line_end)]).max(1);
    let pad = " ".repeat(number.to_string().len());
    format!(
        "{}\n{pad}--> {path}:{number}:{column}\n{pad} |\n{number} | {}\n{pad} | {}{}",
        header(code, message),
        source.replace('\t', "    "),
        " ".repeat(before),
        "^".repeat(marked),
    )
}

/// An error that is of no one place.
pub(crate) fn of_none(message: &str) -> String {
    header(None, message)
}

fn header(code: Option<&str>, message: &str) -> String {
    match code {
        Some(code) => format!("error[{code}]: {message}"),
        None => format!("error: {message}"),
    }
}

/// How many errors `said` has, as [`at`] and [`of_none`] write them.
pub(crate) fn count(said: &str) -> usize {
    said.lines().filter(|it| it.starts_with("error[") || it.starts_with("error: ")).count()
}

/// The last char boundary of `text` at or before `at`.
fn floor(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;
    use syntax::TextSize;

    fn range(text: &str, of: &str) -> TextRange {
        let start = text.find(of).unwrap();
        TextRange::at(TextSize::new(start as u32), TextSize::new(of.len() as u32))
    }

    #[test]
    fn as_rustc_writes_it() {
        let text = "fn main() {}\n\nfn broken() -> u32 { \"text\" }\n";
        let said = at(
            Some("E0308"),
            "expected u32, found &str",
            "app/src/main.rs",
            text,
            range(text, "\"text\""),
        );
        assert_eq!(
            said,
            "error[E0308]: expected u32, found &str\n\
             \x20--> app/src/main.rs:3:22\n\
             \x20 |\n\
             3 | fn broken() -> u32 { \"text\" }\n\
             \x20 |                      ^^^^^^"
        );
        assert_eq!(count(&said), 1);
    }

    #[test]
    fn tabs_lines_and_ranges_past_the_line() {
        let text = "\tlet x = é;\n\tfoo(\n";
        let said = at(None, "cannot find value", "a.rs", text, range(text, "é"));
        assert!(said.ends_with("1 |     let x = é;\n  |             ^"), "{said}");
        let text = format!("{}\n\tfoo(\n", "\n".repeat(8));
        let said = at(None, "unclosed", "a.rs", &text, range(&text, "foo(\n"));
        assert!(said.contains(" --> a.rs:10:2\n"), "{said}");
        assert!(said.ends_with("10 |     foo(\n   |     ^^^^"), "{said}");
        assert_eq!(count(&format!("{said}\n\n{}", of_none("x"))), 2);
    }
}
