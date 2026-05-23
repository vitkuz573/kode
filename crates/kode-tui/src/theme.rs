use ratatui::style::Color;

/// A complete UI color theme
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: &'static str,
    pub base:      Color,
    pub mantle:    Color,
    pub crust:     Color,
    pub surface0:  Color,
    pub surface1:  Color,
    pub surface2:  Color,
    pub overlay0:  Color,
    pub overlay1:  Color,
    pub text:      Color,
    pub subtext0:  Color,
    pub subtext1:  Color,
    pub accent:    Color,   // primary accent (mauve/purple/blue)
    pub accent2:   Color,   // secondary accent
    pub green:     Color,
    pub yellow:    Color,
    pub red:       Color,
    pub blue:      Color,
    pub sapphire:  Color,
}

impl Theme {
    pub fn all() -> Vec<&'static str> {
        vec![
            "catppuccin-mocha",
            "catppuccin-macchiato",
            "catppuccin-frappe",
            "catppuccin-latte",
            "nord",
            "dracula",
            "gruvbox",
            "tokyo-night",
        ]
    }

    pub fn by_name(name: &str) -> Theme {
        match name {
            "catppuccin-macchiato" => CATPPUCCIN_MACCHIATO,
            "catppuccin-frappe"    => CATPPUCCIN_FRAPPE,
            "catppuccin-latte"     => CATPPUCCIN_LATTE,
            "nord"                 => NORD,
            "dracula"              => DRACULA,
            "gruvbox"              => GRUVBOX,
            "tokyo-night"          => TOKYO_NIGHT,
            _                      => CATPPUCCIN_MOCHA,
        }
    }
}

// ── Catppuccin Mocha (dark) ───────────────────────────────────────────────────
pub const CATPPUCCIN_MOCHA: Theme = Theme {
    name:     "catppuccin-mocha",
    base:     Color::Rgb(30,  30,  46),
    mantle:   Color::Rgb(24,  24,  37),
    crust:    Color::Rgb(17,  17,  27),
    surface0: Color::Rgb(49,  50,  68),
    surface1: Color::Rgb(69,  71,  90),
    surface2: Color::Rgb(88,  91,  112),
    overlay0: Color::Rgb(108, 112, 134),
    overlay1: Color::Rgb(127, 132, 156),
    text:     Color::Rgb(205, 214, 244),
    subtext0: Color::Rgb(166, 173, 200),
    subtext1: Color::Rgb(186, 194, 222),
    accent:   Color::Rgb(203, 166, 247),  // mauve
    accent2:  Color::Rgb(180, 190, 254),  // lavender
    green:    Color::Rgb(166, 227, 161),
    yellow:   Color::Rgb(249, 226, 175),
    red:      Color::Rgb(243, 139, 168),
    blue:     Color::Rgb(137, 180, 250),
    sapphire: Color::Rgb(116, 199, 236),
};

// ── Catppuccin Macchiato ──────────────────────────────────────────────────────
pub const CATPPUCCIN_MACCHIATO: Theme = Theme {
    name:     "catppuccin-macchiato",
    base:     Color::Rgb(36,  39,  58),
    mantle:   Color::Rgb(30,  32,  48),
    crust:    Color::Rgb(24,  25,  38),
    surface0: Color::Rgb(54,  58,  79),
    surface1: Color::Rgb(73,  77,  100),
    surface2: Color::Rgb(91,  96,  120),
    overlay0: Color::Rgb(110, 115, 141),
    overlay1: Color::Rgb(128, 135, 162),
    text:     Color::Rgb(202, 211, 245),
    subtext0: Color::Rgb(165, 173, 206),
    subtext1: Color::Rgb(184, 192, 224),
    accent:   Color::Rgb(198, 160, 246),
    accent2:  Color::Rgb(183, 189, 248),
    green:    Color::Rgb(166, 218, 149),
    yellow:   Color::Rgb(238, 212, 159),
    red:      Color::Rgb(237, 135, 150),
    blue:     Color::Rgb(138, 173, 244),
    sapphire: Color::Rgb(125, 196, 228),
};

// ── Catppuccin Frappé ─────────────────────────────────────────────────────────
pub const CATPPUCCIN_FRAPPE: Theme = Theme {
    name:     "catppuccin-frappe",
    base:     Color::Rgb(48,  52,  70),
    mantle:   Color::Rgb(41,  44,  60),
    crust:    Color::Rgb(35,  38,  52),
    surface0: Color::Rgb(65,  69,  89),
    surface1: Color::Rgb(81,  87,  109),
    surface2: Color::Rgb(98,  104, 128),
    overlay0: Color::Rgb(115, 121, 148),
    overlay1: Color::Rgb(131, 139, 167),
    text:     Color::Rgb(198, 208, 245),
    subtext0: Color::Rgb(165, 173, 206),
    subtext1: Color::Rgb(181, 191, 226),
    accent:   Color::Rgb(202, 158, 230),
    accent2:  Color::Rgb(186, 187, 241),
    green:    Color::Rgb(166, 209, 137),
    yellow:   Color::Rgb(229, 200, 144),
    red:      Color::Rgb(231, 130, 132),
    blue:     Color::Rgb(140, 170, 238),
    sapphire: Color::Rgb(133, 193, 220),
};

// ── Catppuccin Latte (light) ──────────────────────────────────────────────────
pub const CATPPUCCIN_LATTE: Theme = Theme {
    name:     "catppuccin-latte",
    base:     Color::Rgb(239, 241, 245),
    mantle:   Color::Rgb(230, 233, 239),
    crust:    Color::Rgb(220, 224, 232),
    surface0: Color::Rgb(204, 208, 218),
    surface1: Color::Rgb(188, 192, 204),
    surface2: Color::Rgb(172, 176, 190),
    overlay0: Color::Rgb(156, 160, 176),
    overlay1: Color::Rgb(140, 143, 161),
    text:     Color::Rgb(76,  79,  105),
    subtext0: Color::Rgb(108, 111, 133),
    subtext1: Color::Rgb(92,  95,  119),
    accent:   Color::Rgb(136, 57,  239),
    accent2:  Color::Rgb(114, 135, 253),
    green:    Color::Rgb(64,  160, 43),
    yellow:   Color::Rgb(223, 142, 29),
    red:      Color::Rgb(210, 15,  57),
    blue:     Color::Rgb(30,  102, 245),
    sapphire: Color::Rgb(32,  159, 181),
};

// ── Nord ──────────────────────────────────────────────────────────────────────
pub const NORD: Theme = Theme {
    name:     "nord",
    base:     Color::Rgb(46,  52,  64),
    mantle:   Color::Rgb(39,  44,  54),
    crust:    Color::Rgb(36,  41,  51),
    surface0: Color::Rgb(59,  66,  82),
    surface1: Color::Rgb(67,  76,  94),
    surface2: Color::Rgb(76,  86,  106),
    overlay0: Color::Rgb(96,  108, 128),
    overlay1: Color::Rgb(111, 124, 144),
    text:     Color::Rgb(236, 239, 244),
    subtext0: Color::Rgb(216, 222, 233),
    subtext1: Color::Rgb(229, 233, 240),
    accent:   Color::Rgb(180, 142, 173),  // nord purple
    accent2:  Color::Rgb(136, 192, 208),  // nord frost
    green:    Color::Rgb(163, 190, 140),
    yellow:   Color::Rgb(235, 203, 139),
    red:      Color::Rgb(191, 97,  106),
    blue:     Color::Rgb(129, 161, 193),
    sapphire: Color::Rgb(136, 192, 208),
};

// ── Dracula ───────────────────────────────────────────────────────────────────
pub const DRACULA: Theme = Theme {
    name:     "dracula",
    base:     Color::Rgb(40,  42,  54),
    mantle:   Color::Rgb(33,  34,  44),
    crust:    Color::Rgb(26,  27,  35),
    surface0: Color::Rgb(52,  54,  70),
    surface1: Color::Rgb(64,  66,  84),
    surface2: Color::Rgb(76,  79,  100),
    overlay0: Color::Rgb(98,  101, 124),
    overlay1: Color::Rgb(110, 113, 138),
    text:     Color::Rgb(248, 248, 242),
    subtext0: Color::Rgb(191, 191, 191),
    subtext1: Color::Rgb(220, 220, 220),
    accent:   Color::Rgb(189, 147, 249),  // purple
    accent2:  Color::Rgb(255, 121, 198),  // pink
    green:    Color::Rgb(80,  250, 123),
    yellow:   Color::Rgb(241, 250, 140),
    red:      Color::Rgb(255, 85,  85),
    blue:     Color::Rgb(139, 233, 253),
    sapphire: Color::Rgb(98,  209, 255),
};

// ── Gruvbox Dark ──────────────────────────────────────────────────────────────
pub const GRUVBOX: Theme = Theme {
    name:     "gruvbox",
    base:     Color::Rgb(40,  40,  40),
    mantle:   Color::Rgb(29,  32,  33),
    crust:    Color::Rgb(24,  24,  24),
    surface0: Color::Rgb(60,  56,  54),
    surface1: Color::Rgb(80,  73,  69),
    surface2: Color::Rgb(102, 92,  84),
    overlay0: Color::Rgb(124, 111, 100),
    overlay1: Color::Rgb(146, 131, 116),
    text:     Color::Rgb(235, 219, 178),
    subtext0: Color::Rgb(213, 196, 161),
    subtext1: Color::Rgb(189, 174, 147),
    accent:   Color::Rgb(211, 134, 155),  // gruvbox purple
    accent2:  Color::Rgb(131, 165, 152),  // gruvbox aqua
    green:    Color::Rgb(184, 187, 38),
    yellow:   Color::Rgb(250, 189, 47),
    red:      Color::Rgb(251, 73,  52),
    blue:     Color::Rgb(131, 165, 152),
    sapphire: Color::Rgb(142, 192, 124),
};

// ── Tokyo Night ───────────────────────────────────────────────────────────────
pub const TOKYO_NIGHT: Theme = Theme {
    name:     "tokyo-night",
    base:     Color::Rgb(26,  27,  38),
    mantle:   Color::Rgb(22,  22,  30),
    crust:    Color::Rgb(16,  16,  24),
    surface0: Color::Rgb(41,  46,  66),
    surface1: Color::Rgb(54,  59,  82),
    surface2: Color::Rgb(65,  72,  104),
    overlay0: Color::Rgb(86,  95,  137),
    overlay1: Color::Rgb(101, 111, 158),
    text:     Color::Rgb(192, 202, 245),
    subtext0: Color::Rgb(169, 177, 214),
    subtext1: Color::Rgb(180, 190, 230),
    accent:   Color::Rgb(187, 154, 247),  // purple
    accent2:  Color::Rgb(122, 162, 247),  // blue
    green:    Color::Rgb(158, 206, 106),
    yellow:   Color::Rgb(224, 175, 104),
    red:      Color::Rgb(247, 118, 142),
    blue:     Color::Rgb(122, 162, 247),
    sapphire: Color::Rgb(125, 207, 255),
};
