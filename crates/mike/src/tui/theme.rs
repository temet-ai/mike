use ratatui::style::Color;

pub const BG_0: Color = Color::Rgb(0x08, 0x06, 0x0f);
pub const BG_1: Color = Color::Rgb(0x0f, 0x0b, 0x1c);
pub const BG_2: Color = Color::Rgb(0x15, 0x10, 0x26);
pub const BG_3: Color = Color::Rgb(0x1a, 0x13, 0x2c);

pub const FG_1: Color = Color::Rgb(0xdc, 0xd6, 0xec);
pub const FG_2: Color = Color::Rgb(0xa7, 0x9d, 0xc4);
pub const FG_3: Color = Color::Rgb(0x86, 0x7a, 0xa8);
pub const FG_DIM: Color = Color::Rgb(0x4a, 0x40, 0x70);

pub const UV: Color = Color::Rgb(0x9d, 0x5c, 0xff);
pub const RUST: Color = Color::Rgb(0xff, 0x5c, 0x2e);
pub const ACID: Color = Color::Rgb(0xcf, 0xf5, 0x4d);
pub const ICE: Color = Color::Rgb(0x4d, 0x9f, 0xff);
pub const RED: Color = Color::Rgb(0xff, 0x2d, 0x55);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Uv,
    Rust,
    Acid,
    Ice,
}

impl Tone {
    pub fn color(self) -> Color {
        match self {
            Tone::Uv => UV,
            Tone::Rust => RUST,
            Tone::Acid => ACID,
            Tone::Ice => ICE,
        }
    }
}

pub mod glyph {
    pub const FILLED_CIRCLE: char = '●';
    pub const OPEN_CIRCLE: char = '○';
    pub const FILLED_DIAMOND: char = '◆';
    pub const OPEN_DIAMOND: char = '◇';
    pub const CURRENT_STEP: char = '◈';
    pub const SELECTION: char = '▌';
    pub const SEP: char = '·';
}
