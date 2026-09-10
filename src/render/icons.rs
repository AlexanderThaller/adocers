//! Admonition icons.
//!
//! Asciidoctor draws these with Font Awesome, which means a page has to pull in
//! a webfont before a note looks like anything. These are inline SVG instead:
//! they cost about a hundred bytes each, need no network, and take their colour
//! from `currentColor`, so one rule per admonition type themes them and they
//! follow the reader's light or dark scheme without a second set.

use crate::render::html::escape_attr;

/// The icon drawn for an admonition, by its Asciidoctor context name.
///
/// Returns `None` for a name with no icon of its own, so a variant added to the
/// parser later degrades to the text label rather than to a blank column.
pub fn admonition(name: &str, label: &str) -> Option<String> {
    let paths = match name {
        // An "i" — the dot above the stem.
        "note" => {
            r#"<circle cx="12" cy="12" r="9"/><path d="M12 11.5v5"/><circle cx="12" cy="7.6" r="1.15" fill="currentColor" stroke="none"/>"#
        }

        // A bulb over the two lines of its base.
        "tip" => {
            r#"<path d="M12 3a5.5 5.5 0 0 1 3.2 9.97c-.55.4-.95 1.02-.95 1.7V16h-4.5v-1.33c0-.68-.4-1.3-.95-1.7A5.5 5.5 0 0 1 12 3Z"/><path d="M9.75 18.75h4.5"/><path d="M10.75 21.25h2.5"/>"#
        }

        // A "!" — the stem above the dot, so it does not read as the note "i".
        "important" => {
            r#"<circle cx="12" cy="12" r="9"/><path d="M12 7v6"/><circle cx="12" cy="16.4" r="1.15" fill="currentColor" stroke="none"/>"#
        }

        // A flame.
        "caution" => {
            r#"<path d="M12 2.75c.6 3 2.2 4.6 3.7 6.1 1.6 1.6 2.55 3.2 2.55 5.4a6.25 6.25 0 0 1-12.5 0c0-1.7.6-3 1.7-4.2.15 1.2.7 2 1.6 2.35.5-3.4 1.6-6.4 2.95-9.65Z"/>"#
        }

        // A "!" inside a triangle.
        "warning" => {
            r#"<path d="M10.6 4.3 2.5 18.5A1.6 1.6 0 0 0 3.9 21h16.2a1.6 1.6 0 0 0 1.4-2.5L13.4 4.3a1.6 1.6 0 0 0-2.8 0Z"/><path d="M12 10v4"/><circle cx="12" cy="17.3" r="1.15" fill="currentColor" stroke="none"/>"#
        }

        _ => return None,
    };

    // The label is the icon's only text, so it has to reach a screen reader.
    Some(format!(
        r#"<svg class="icon icon-{name}" viewBox="0 0 24 24" width="36" height="36" role="img" aria-label="{label}" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">{paths}</svg>"#,
        label = escape_attr(label)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_admonition_the_parser_produces_has_an_icon() {
        for name in ["note", "tip", "important", "caution", "warning"] {
            assert!(admonition(name, "Label").is_some(), "no icon for {name}");
        }
    }

    #[test]
    fn an_unknown_admonition_has_none() {
        assert!(admonition("hazard", "Hazard").is_none());
    }

    #[test]
    fn the_label_is_escaped_into_the_accessible_name() {
        let svg = admonition("note", "\"Note\" & <b>").unwrap_or_default();

        assert!(svg.contains("aria-label=\"&quot;Note&quot; &amp; &lt;b&gt;\""));
    }
}
