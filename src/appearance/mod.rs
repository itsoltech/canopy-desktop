//! Theme families, their dark/light variants and the active selection.
//!
//! Plain data shared by the UI token layer (`ui::theme`), the terminal emulator
//! (ANSI palette) and the settings contract. Colors are `0xRRGGBB`; derivation of
//! secondary UI tokens belongs to the UI layer.
mod background;
mod palettes;

pub use background::{
    Background, BackgroundMode, BackgroundTarget, Backgrounds, GRADIENT_MAX_STOPS,
    GRADIENT_MIN_STOPS, Gradient, GradientStop, HexColor,
};
use serde::Serialize;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A theme family; each has a dark and a light variant.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize)]
pub enum ThemeFamily {
    #[default]
    Canopy,
    Dracula,
    Monokai,
    Solarized,
    Nord,
    One,
    Gruvbox,
    TokyoNight,
    Catppuccin,
    GitHub,
    RosePine,
}
impl ThemeFamily {
    pub const ALL: [Self; 11] = [
        Self::Canopy,
        Self::Dracula,
        Self::Monokai,
        Self::Solarized,
        Self::Nord,
        Self::One,
        Self::Gruvbox,
        Self::TokyoNight,
        Self::Catppuccin,
        Self::GitHub,
        Self::RosePine,
    ];
    /// Stable identifier stored in `appearance.theme`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Canopy => "canopy",
            Self::Dracula => "dracula",
            Self::Monokai => "monokai",
            Self::Solarized => "solarized",
            Self::Nord => "nord",
            Self::One => "one",
            Self::Gruvbox => "gruvbox",
            Self::TokyoNight => "tokyo-night",
            Self::Catppuccin => "catppuccin",
            Self::GitHub => "github",
            Self::RosePine => "rose-pine",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|family| family.key() == key)
    }
    /// Family name shown in Settings.
    pub fn name(self) -> &'static str {
        match self {
            Self::Canopy => "Canopy",
            Self::Dracula => "Dracula",
            Self::Monokai => "Monokai",
            Self::Solarized => "Solarized",
            Self::Nord => "Nord",
            Self::One => "One",
            Self::Gruvbox => "Gruvbox",
            Self::TokyoNight => "Tokyo Night",
            Self::Catppuccin => "Catppuccin",
            Self::GitHub => "GitHub",
            Self::RosePine => "Rosé Pine",
        }
    }
    /// Colors of one variant of this family.
    pub fn spec(self, variant: ThemeVariant) -> &'static VariantSpec {
        &palettes::SPECS[index(self, variant)]
    }
    /// Electron's terminal theme names (`theme` preference), applied app-wide there.
    pub fn from_electron(name: &str) -> Option<(Self, ThemeVariant)> {
        Some(match name {
            "Default" => (Self::Canopy, ThemeVariant::Dark),
            "Dracula" => (Self::Dracula, ThemeVariant::Dark),
            "Monokai" => (Self::Monokai, ThemeVariant::Dark),
            "Solarized Dark" => (Self::Solarized, ThemeVariant::Dark),
            "Solarized Light" => (Self::Solarized, ThemeVariant::Light),
            "Nord" => (Self::Nord, ThemeVariant::Dark),
            "One Dark" => (Self::One, ThemeVariant::Dark),
            "Gruvbox Dark" => (Self::Gruvbox, ThemeVariant::Dark),
            "Tokyo Night" => (Self::TokyoNight, ThemeVariant::Dark),
            "Catppuccin Mocha" => (Self::Catppuccin, ThemeVariant::Dark),
            "GitHub Dark" => (Self::GitHub, ThemeVariant::Dark),
            "Rosé Pine" => (Self::RosePine, ThemeVariant::Dark),
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize)]
pub enum ThemeVariant {
    #[default]
    Dark,
    Light,
}
impl ThemeVariant {
    pub const ALL: [Self; 2] = [Self::Dark, Self::Light];
    pub fn key(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
    pub fn is_light(self) -> bool {
        self == Self::Light
    }
}

/// The user's choice: a fixed variant, or the OS appearance (`Auto`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize)]
pub enum AppearanceMode {
    Light,
    /// Keeps the look Canopy had before themes for existing users.
    #[default]
    Dark,
    Auto,
}
impl AppearanceMode {
    pub const ALL: [Self; 3] = [Self::Light, Self::Dark, Self::Auto];
    /// Stored value of `appearance.mode`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::Auto => "auto",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.key() == key)
    }
    /// The variant shown for this mode while the OS appearance is `system`.
    pub fn resolve(self, system: ThemeVariant) -> ThemeVariant {
        match self {
            Self::Light => ThemeVariant::Light,
            Self::Dark => ThemeVariant::Dark,
            Self::Auto => system,
        }
    }
}

/// Base colors of one variant; every other UI token is derived from these.
#[derive(Debug)]
pub struct VariantSpec {
    /// Variant name, e.g. "Catppuccin Latte".
    pub name: &'static str,
    pub bg: u32,
    pub sidebar: u32,
    pub text: u32,
    pub accent: u32,
    pub green: u32,
    pub red: u32,
    pub yellow: u32,
    pub terminal: TerminalColors,
}

/// The terminal's default colors: 16 ANSI colors plus background and foreground.
#[derive(Debug)]
pub struct TerminalColors {
    pub ansi: [u32; 16],
    pub background: u32,
    pub foreground: u32,
}
impl TerminalColors {
    /// `0xRRGGBB` of an indexed or named (alacritty `NamedColor`) terminal color:
    /// 0–15 from the theme, 16–231 the xterm cube, 232–255 the gray ramp, the
    /// named background as the theme's background and other named colors as its
    /// foreground.
    pub fn color(&self, index: usize) -> u32 {
        const BACKGROUND: usize = 257;
        if index < 16 {
            self.ansi[index]
        } else if index < 232 {
            let n = index - 16;
            let cube = |n: usize| if n == 0 { 0 } else { 55 + n * 40 } as u32;
            (cube(n / 36) << 16) | (cube((n / 6) % 6) << 8) | cube(n % 6)
        } else if index < 256 {
            let v = (8 + (index - 232) * 10) as u32;
            (v << 16) | (v << 8) | v
        } else if index == BACKGROUND {
            self.background
        } else {
            self.foreground
        }
    }
}

/// Number of (family, variant) pairs; `index` enumerates them densely.
pub const VARIANT_COUNT: usize = ThemeFamily::ALL.len() * 2;

/// Dense index of a (family, variant) pair: family-major, dark before light.
pub fn index(family: ThemeFamily, variant: ThemeVariant) -> usize {
    family as usize * 2 + variant as usize
}

static ACTIVE: AtomicUsize = AtomicUsize::new(0);

/// The variant every token and the terminal palette currently read.
pub fn active() -> (ThemeFamily, ThemeVariant) {
    let index = active_index();
    (ThemeFamily::ALL[index / 2], ThemeVariant::ALL[index % 2])
}
/// `index` of the active pair; a single relaxed load, cheap enough for render.
pub fn active_index() -> usize {
    ACTIVE.load(Ordering::Relaxed)
}
/// Switches the active variant. Only the UI theme owner calls this, then repaints.
pub fn set_active(family: ThemeFamily, variant: ThemeVariant) {
    ACTIVE.store(index(family, variant), Ordering::Relaxed);
}
/// Terminal colors of the active variant; safe from any thread.
pub fn active_terminal() -> &'static TerminalColors {
    &palettes::SPECS[active_index()].terminal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip_and_every_variant_has_a_spec() {
        for family in ThemeFamily::ALL {
            assert_eq!(ThemeFamily::from_key(family.key()), Some(family));
            for variant in ThemeVariant::ALL {
                let spec = family.spec(variant);
                // A light variant has a light background, a dark one a dark background.
                let luma = |c: u32| {
                    0.299 * ((c >> 16) & 255) as f32
                        + 0.587 * ((c >> 8) & 255) as f32
                        + 0.114 * (c & 255) as f32
                };
                assert_eq!(luma(spec.bg) > 128., variant.is_light(), "{}", spec.name);
                assert_eq!(
                    luma(spec.terminal.background) > 128.,
                    variant.is_light(),
                    "{} terminal",
                    spec.name
                );
            }
        }
        for mode in AppearanceMode::ALL {
            assert_eq!(AppearanceMode::from_key(mode.key()), Some(mode));
        }
        assert_eq!(ThemeFamily::from_key("Dracula"), None);
    }

    #[test]
    fn auto_follows_the_system_and_fixed_modes_ignore_it() {
        for system in ThemeVariant::ALL {
            assert_eq!(AppearanceMode::Auto.resolve(system), system);
            assert_eq!(AppearanceMode::Light.resolve(system), ThemeVariant::Light);
            assert_eq!(AppearanceMode::Dark.resolve(system), ThemeVariant::Dark);
        }
    }

    #[test]
    fn terminal_palette_comes_from_the_variant() {
        let canopy = ThemeFamily::Canopy.spec(ThemeVariant::Dark);
        // Canopy dark keeps the palette terminals had before themes.
        assert_eq!(canopy.terminal.color(1), 0xe06c75);
        assert_eq!(canopy.terminal.color(257), 0x202020);
        assert_eq!(canopy.terminal.color(256), 0xc8c8c8);
        let latte = ThemeFamily::Catppuccin.spec(ThemeVariant::Light);
        assert_eq!(latte.terminal.color(1), 0xd20f39);
        assert_eq!(latte.terminal.color(257), 0xeff1f5);
        assert_eq!(latte.terminal.color(256), 0x4c4f69);
        // The xterm cube and gray ramp are theme independent.
        assert_eq!(latte.terminal.color(16), 0x000000);
        assert_eq!(latte.terminal.color(231), 0xffffff);
        assert_eq!(canopy.terminal.color(232), latte.terminal.color(232));
    }

    #[test]
    fn electron_theme_names_map_to_families() {
        assert_eq!(
            ThemeFamily::from_electron("Solarized Light"),
            Some((ThemeFamily::Solarized, ThemeVariant::Light))
        );
        assert_eq!(
            ThemeFamily::from_electron("Default"),
            Some((ThemeFamily::Canopy, ThemeVariant::Dark))
        );
        assert_eq!(ThemeFamily::from_electron("Unknown"), None);
    }
}
