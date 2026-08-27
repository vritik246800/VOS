use ratatui::style::Color;

#[derive(Clone, Debug)]
pub struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub accent: Color,
    pub selection: Color,
    pub error: Color,
    pub warn: Color,
    pub ok: Color,
    pub dim: Color,
    pub name: &'static str,
}

impl Theme {
    pub fn by_name(name: &str) -> Theme {
        match name {
            "light" => Theme::light(),
            "neon" => Theme::neon(),
            "solarized" => Theme::solarized(),
            "gruvbox" => Theme::gruvbox(),
            _ => Theme::dark(),
        }
    }

    pub fn all() -> &'static [&'static str] {
        &["dark", "light", "neon", "solarized", "gruvbox"]
    }

    fn dark() -> Theme {
        Theme {
            name: "dark",
            bg: Color::Black,
            fg: Color::White,
            accent: Color::Cyan,
            selection: Color::DarkGray,
            error: Color::Red,
            warn: Color::Yellow,
            ok: Color::Green,
            dim: Color::DarkGray,
        }
    }

    fn light() -> Theme {
        Theme {
            name: "light",
            bg: Color::White,
            fg: Color::Black,
            accent: Color::Blue,
            selection: Color::Gray,
            error: Color::Red,
            warn: Color::Yellow,
            ok: Color::Green,
            dim: Color::Gray,
        }
    }

    fn neon() -> Theme {
        Theme {
            name: "neon",
            bg: Color::Black,
            fg: Color::White,
            accent: Color::Magenta,
            selection: Color::DarkGray,
            error: Color::Red,
            warn: Color::Yellow,
            ok: Color::Green,
            dim: Color::DarkGray,
        }
    }

    fn solarized() -> Theme {
        Theme {
            name: "solarized",
            bg: Color::Rgb(0, 43, 54),
            fg: Color::Rgb(131, 148, 150),
            accent: Color::Rgb(38, 139, 210),
            selection: Color::Rgb(7, 54, 66),
            error: Color::Rgb(220, 50, 47),
            warn: Color::Rgb(181, 137, 0),
            ok: Color::Rgb(133, 153, 0),
            dim: Color::Rgb(88, 110, 117),
        }
    }

    fn gruvbox() -> Theme {
        Theme {
            name: "gruvbox",
            bg: Color::Rgb(40, 40, 40),
            fg: Color::Rgb(235, 219, 178),
            accent: Color::Rgb(250, 189, 47),
            selection: Color::Rgb(60, 56, 54),
            error: Color::Rgb(204, 36, 29),
            warn: Color::Rgb(215, 153, 33),
            ok: Color::Rgb(152, 151, 26),
            dim: Color::Rgb(124, 111, 100),
        }
    }
}
