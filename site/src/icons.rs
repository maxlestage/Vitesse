//! Les icônes (traits SVG) et le logo, l'éclair.

use active::prelude::*;

fn paths(name: &str) -> &'static [&'static str] {
    match name {
        "engine" => &["M13 2 4 14h7l-1 8 9-12h-7z"],
        "cores" => &[
            "M4 4h6v6H4z",
            "M14 4h6v6h-6z",
            "M4 14h6v6H4z",
            "M14 14h6v6h-6z",
        ],
        "tree" => &[
            "M12 3v6",
            "M12 9 6 15",
            "M12 9l6 6",
            "M6 15v4",
            "M18 15v4",
            "M12 9v10",
        ],
        "layers" => &["m12 3 9 5-9 5-9-5z", "m3 13 9 5 9-5"],
        "files" => &["M14 3H6v18h12V7z", "M14 3v4h4", "M9 13h6", "M9 17h6"],
        "shield" => &[
            "M12 3 4 6v6c0 5 3.5 8 8 9 4.5-1 8-4 8-9V6z",
            "m9 12 2 2 4-4",
        ],
        "flask" => &["M9 3h6", "M10 3v6L4 20h16L14 9V3", "M7 15h10"],
        "power" => &["M12 3v9", "M6.3 7.5a8 8 0 1 0 11.4 0"],
        "github" => &[
            "M9 19c-5 1.5-5-2.5-7-3m14 6v-3.9a3.4 3.4 0 0 0-.9-2.6c3-.3 6.1-1.5 6.1-6.6a5.1 5.1 0 0 0-1.4-3.6 4.8 4.8 0 0 0-.1-3.5s-1.1-.3-3.6 1.4a12.3 12.3 0 0 0-6.4 0C6.2 1.6 5.1 1.9 5.1 1.9a4.8 4.8 0 0 0-.1 3.5A5.1 5.1 0 0 0 3.6 9c0 5.1 3.1 6.3 6.1 6.6a3.4 3.4 0 0 0-.9 2.6V22",
        ],
        "arrow" => &["M5 12h14", "m13 6 6 6-6 6"],
        "up" => &["M12 19V5", "m6 11 6-6 6 6"],
        "search" => &["M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14z", "m20 20-4.2-4.2"],
        "edit" => &["M4 20h4L19 9l-4-4L4 16z", "m13.5 6.5 4 4"],
        "book" => &[
            "M4 5.5A2.5 2.5 0 0 1 6.5 3H20v15H6.5A2.5 2.5 0 0 0 4 20.5z",
            "M4 20.5A2.5 2.5 0 0 0 6.5 23H20v-5",
            "M8 7h8",
        ],
        "phone" => &[
            "M8 2h8a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2z",
            "M11 18h2",
        ],
        "menu" => &["M4 7h16", "M4 12h16", "M4 17h10"],
        "globe" => &[
            "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18z",
            "M3 12h18",
            "M12 3c2.5 2.5 3.8 5.5 3.8 9s-1.3 6.5-3.8 9c-2.5-2.5-3.8-5.5-3.8-9S9.5 5.5 12 3z",
        ],
        "offline" => &[
            "M2 8.8a15 15 0 0 1 4.2-2.7",
            "M10.7 5.1A15 15 0 0 1 22 8.8",
            "M5 12.9a10 10 0 0 1 5.2-2.8",
            "M16.8 11.4a10 10 0 0 1 2.2 1.5",
            "M8.5 16.4a5 5 0 0 1 7 0",
            "M12 20h.01",
            "m3 3 18 18",
        ],
        _ => &[],
    }
}

/// Une icône de 24 × 24 dessinée au trait.
pub fn icon(name: &str) -> Element {
    svg()
        .class("icon")
        .attr("viewBox", "0 0 24 24")
        .attr("fill", "none")
        .attr("stroke", "currentColor")
        .attr("stroke-width", "1.6")
        .attr("stroke-linecap", "round")
        .attr("stroke-linejoin", "round")
        .attr("aria-hidden", "true")
        .children(paths(name).iter().map(|d| path().attr("d", *d)))
}

/// Le logo : un éclair en dégradé.
pub fn logo() -> Element {
    svg()
        .class("logo-mark")
        .attr("viewBox", "0 0 32 32")
        .attr("aria-hidden", "true")
        .child(
            defs().child(
                linear_gradient()
                    .id("logo-grad")
                    .attr("x1", "0")
                    .attr("y1", "0")
                    .attr("x2", "1")
                    .attr("y2", "1")
                    .child(stop().attr("offset", "0%").attr("stop-color", "#ffd23f"))
                    .child(stop().attr("offset", "55%").attr("stop-color", "#ff7a1a"))
                    .child(stop().attr("offset", "100%").attr("stop-color", "#ff2e63")),
            ),
        )
        .child(
            path()
                .attr("d", "M18.5 2 6 18h8.5l-2 12L26 13h-8.7z")
                .attr("fill", "url(#logo-grad)"),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_have_their_paths() {
        let html = Node::from(icon("arrow")).render();
        assert!(html.starts_with("<svg class=\"icon\" viewBox=\"0 0 24 24\""));
        assert_eq!(html.matches("<path").count(), 2);
        assert_eq!(
            Node::from(icon("nope")).render().matches("<path").count(),
            0
        );
        assert!(Node::from(logo()).render().contains("url(#logo-grad)"));
    }
}
