//! Escaping helpers for the strings mcopy writes into files that other
//! programs will later parse as code: shell scripts, XML plists, and
//! freedesktop `.desktop` entries.
//!
//! Every one of these files embeds an executable path chosen by the OS, not by
//! us. A path containing a quote, `$`, or a backslash would otherwise break the
//! generated file or let the surrounding interpreter run something we never
//! intended, so interpolation always goes through the matching helper here.

/// Quote a value for POSIX `sh`/`bash`/`zsh`.
///
/// Single quotes disable every expansion the shell performs, so the only case
/// needing care is an embedded single quote: close the literal, emit an escaped
/// quote, reopen it.
pub fn quote_posix(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Escape a value for use as XML text or an attribute value.
pub fn escape_xml(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

/// Quote a program path for a freedesktop `.desktop` `Exec=` key.
///
/// The Desktop Entry spec defines two layers that both apply here: the value is
/// first unescaped by the desktop-entry parser (`\\` sequences), then split
/// into an argument vector using its own quoting rules. Reserved characters
/// must be backslash-escaped *inside* the double quotes, and the backslash
/// itself must survive the first layer, hence the doubling.
///
/// See <https://specifications.freedesktop.org/desktop-entry-spec/latest/exec-variables.html>.
pub fn quote_desktop_exec(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for character in value.chars() {
        // Reserved by the Exec parser; each needs a literal backslash in the
        // parsed value, which is written as `\\` in the file.
        if matches!(character, '"' | '`' | '$' | '\\') {
            quoted.push_str("\\\\");
        }
        quoted.push(character);
    }
    quoted.push('"');
    quoted
}

/// Escape a value for a non-`Exec` `.desktop` key such as `Name`.
///
/// Only the C-style escape sequences the desktop-entry parser recognises need
/// handling; a literal backslash must be doubled so it is not read as the start
/// of one.
pub fn escape_desktop_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_posix_wraps_plain_values() {
        assert_eq!(quote_posix("/usr/bin/mcopy"), "'/usr/bin/mcopy'");
    }

    #[test]
    fn quote_posix_neutralizes_expansion_characters() {
        // `$`, backtick and `"` are inert inside single quotes, so they must
        // pass through untouched rather than being escaped twice.
        assert_eq!(
            quote_posix("/tmp/$HOME `id` \"x\""),
            "'/tmp/$HOME `id` \"x\"'"
        );
    }

    #[test]
    fn quote_posix_escapes_embedded_single_quote() {
        assert_eq!(quote_posix("/tmp/it's"), r"'/tmp/it'\''s'");
    }

    /// Minimal POSIX word unquoter: consumes exactly the constructs
    /// [`quote_posix`] emits (single-quoted runs and backslash-escaped quotes)
    /// and returns the single word a shell would produce, or `None` if the
    /// input is not one well-formed word.
    ///
    /// Round-tripping through this is a far stronger check than substring
    /// matching, which cannot tell a real break-out from the `'\''` escape
    /// sequence that legitimately contains a quote.
    fn unquote_posix_word(input: &str) -> Option<String> {
        let mut output = String::new();
        let mut chars = input.chars();

        while let Some(character) = chars.next() {
            match character {
                '\'' => {
                    // Opening quote: copy verbatim until the closing quote.
                    loop {
                        match chars.next() {
                            Some('\'') => break,
                            Some(inner) => output.push(inner),
                            // Unterminated literal: the word is malformed.
                            None => return None,
                        }
                    }
                },
                '\\' => output.push(chars.next()?),
                // An unquoted space would split the word in two.
                c if c.is_whitespace() => return None,
                c => output.push(c),
            }
        }

        Some(output)
    }

    #[test]
    fn quote_posix_survives_a_quote_break_out_attempt() {
        let payload = "/tmp/x'; rm -rf ~; echo '";
        let quoted = quote_posix(payload);

        assert_eq!(quoted, r"'/tmp/x'\''; rm -rf ~; echo '\'''");
        // The shell must see one word that is byte-for-byte the original, so
        // the injected `rm` is inert data rather than a command.
        assert_eq!(unquote_posix_word(&quoted).as_deref(), Some(payload));
    }

    #[test]
    fn quote_posix_round_trips_hostile_inputs() {
        for payload in [
            "/usr/bin/mcopy",
            "/home/u/My Apps/mcopy",
            "/tmp/it's",
            "/tmp/$HOME `id` \"x\"",
            r"/tmp/back\slash",
            "/tmp/a\nb",
            "''",
            "'",
            "",
        ] {
            assert_eq!(
                unquote_posix_word(&quote_posix(payload)).as_deref(),
                Some(payload),
                "round-trip failed for {payload:?}"
            );
        }
    }

    #[test]
    fn quote_posix_keeps_spaces_and_newlines_in_one_word() {
        assert_eq!(quote_posix("/tmp/a b\nc"), "'/tmp/a b\nc'");
    }

    #[test]
    fn escape_xml_covers_all_five_entities() {
        assert_eq!(
            escape_xml(r#"a&b<c>d"e'f"#),
            "a&amp;b&lt;c&gt;d&quot;e&apos;f"
        );
    }

    #[test]
    fn escape_xml_escapes_ampersand_once() {
        // A naive chained `replace` would turn `&lt;` into `&amp;lt;`.
        assert_eq!(escape_xml("<tag>"), "&lt;tag&gt;");
        assert_eq!(escape_xml("&amp;"), "&amp;amp;");
    }

    #[test]
    fn quote_desktop_exec_wraps_plain_paths() {
        assert_eq!(quote_desktop_exec("/usr/bin/mcopy"), "\"/usr/bin/mcopy\"");
    }

    #[test]
    fn quote_desktop_exec_escapes_reserved_characters() {
        assert_eq!(
            quote_desktop_exec(r#"/tmp/a"b$c`d\e"#),
            r#""/tmp/a\\"b\\$c\\`d\\\e""#
        );
    }

    #[test]
    fn quote_desktop_exec_keeps_spaces_in_one_argument() {
        assert_eq!(
            quote_desktop_exec("/home/u/My Apps/mcopy"),
            "\"/home/u/My Apps/mcopy\""
        );
    }

    #[test]
    fn escape_desktop_value_doubles_backslashes() {
        assert_eq!(escape_desktop_value(r"C:\tmp"), r"C:\\tmp");
        assert_eq!(escape_desktop_value("a\nb"), "a\\nb");
    }
}
