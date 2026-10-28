//! Custom window/sidebar backgrounds, stored per variant. Persisted as JSON in
//! `appearance.background.<dark|light>.<window|sidebar>`; the UI paints them.
use super::ThemeVariant;
use serde::{Deserialize, Serialize};

pub const GRADIENT_MIN_STOPS: usize = 2;
pub const GRADIENT_MAX_STOPS: usize = 6;
/// Upper bound of one stored background value; a valid one is far smaller.
const ENCODED_LIMIT: usize = 2048;

/// An opaque `0xRRGGBB` color, written as `#rrggbb`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HexColor(pub u32);
impl HexColor {
    /// `#rrggbb` (either case); anything else is rejected.
    pub fn parse(value: &str) -> Option<Self> {
        let hex = value.strip_prefix('#')?;
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        u32::from_str_radix(hex, 16).ok().map(Self)
    }
    pub fn to_hex(self) -> String {
        format!("#{:06x}", self.0 & 0xffffff)
    }
}
impl Serialize for HexColor {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundMode {
    /// The theme's own surface color.
    #[default]
    Theme,
    Solid,
    Gradient,
}
impl BackgroundMode {
    pub const ALL: [Self; 3] = [Self::Theme, Self::Solid, Self::Gradient];
    pub fn key(self) -> &'static str {
        match self {
            Self::Theme => "theme",
            Self::Solid => "solid",
            Self::Gradient => "gradient",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.key() == key)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum BackgroundTarget {
    Window,
    Sidebar,
}
impl BackgroundTarget {
    pub const ALL: [Self; 2] = [Self::Window, Self::Sidebar];
    pub fn key(self) -> &'static str {
        match self {
            Self::Window => "window",
            Self::Sidebar => "sidebar",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct GradientStop {
    pub color: HexColor,
    /// Percent along the gradient line, 0–100.
    pub position: u8,
}

/// A linear gradient: CSS angle (0 = towards the top, clockwise) and 2–6 stops.
/// Stops keep the order the user gave; `sorted_stops` orders them for painting.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct Gradient {
    angle: u16,
    stops: Vec<GradientStop>,
}
impl Gradient {
    /// `None` unless the angle is 0–359, there are 2–6 stops and every position is 0–100.
    pub fn new(angle: u16, stops: Vec<GradientStop>) -> Option<Self> {
        let valid = angle < 360
            && (GRADIENT_MIN_STOPS..=GRADIENT_MAX_STOPS).contains(&stops.len())
            && stops.iter().all(|stop| stop.position <= 100);
        valid.then_some(Self { angle, stops })
    }
    pub fn angle(&self) -> u16 {
        self.angle
    }
    pub fn stops(&self) -> &[GradientStop] {
        &self.stops
    }
    /// Stops by position (stable for equal positions).
    pub fn sorted_stops(&self) -> Vec<GradientStop> {
        let mut stops = self.stops.clone();
        stops.sort_by_key(|stop| stop.position);
        stops
    }
}

/// One surface's background. Solid color and gradient are kept while another
/// mode is selected, so switching back restores them.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct Background {
    pub mode: BackgroundMode,
    pub solid: HexColor,
    pub gradient: Gradient,
}
impl Background {
    /// The stored JSON form.
    pub fn encode(&self) -> String {
        serde_json::to_string(self).expect("background serialization")
    }
    /// Strict decoding; unknown fields or out-of-range values are rejected.
    pub fn decode(value: &str) -> Option<Self> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RawStop {
            color: String,
            position: u8,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RawGradient {
            angle: u16,
            stops: Vec<RawStop>,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            mode: String,
            solid: String,
            gradient: RawGradient,
        }
        if value.len() > ENCODED_LIMIT {
            return None;
        }
        let raw: Raw = serde_json::from_str(value).ok()?;
        if raw.gradient.stops.len() > GRADIENT_MAX_STOPS {
            return None;
        }
        let stops = raw
            .gradient
            .stops
            .into_iter()
            .map(|stop| {
                Some(GradientStop {
                    color: HexColor::parse(&stop.color)?,
                    position: stop.position,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            mode: BackgroundMode::from_key(&raw.mode)?,
            solid: HexColor::parse(&raw.solid)?,
            gradient: Gradient::new(raw.gradient.angle, stops)?,
        })
    }
    /// The theme surface, with starting colors for solid and gradient modes
    /// that suit the variant.
    pub fn default_for(variant: ThemeVariant, target: BackgroundTarget) -> Self {
        let stop = |color, position| GradientStop {
            color: HexColor(color),
            position,
        };
        let (solid, angle, stops) = match (variant, target) {
            (ThemeVariant::Dark, BackgroundTarget::Window) => (
                0x1b2230,
                135,
                vec![stop(0x141a26, 0), stop(0x1d2a3f, 55), stop(0x2a2240, 100)],
            ),
            (ThemeVariant::Dark, BackgroundTarget::Sidebar) => {
                (0x12161c, 180, vec![stop(0x11151b, 0), stop(0x1a1f2b, 100)])
            }
            (ThemeVariant::Light, BackgroundTarget::Window) => (
                0xf4f6fa,
                135,
                vec![stop(0xf6f8fc, 0), stop(0xeef2fa, 55), stop(0xf3eef8, 100)],
            ),
            (ThemeVariant::Light, BackgroundTarget::Sidebar) => {
                (0xeaedf2, 180, vec![stop(0xeef1f5, 0), stop(0xe6eaf0, 100)])
            }
        };
        Self {
            mode: BackgroundMode::Theme,
            solid: HexColor(solid),
            gradient: Gradient::new(angle, stops).expect("valid default gradient"),
        }
    }
}

/// Backgrounds of both targets for both variants.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Backgrounds {
    pub dark_window: Background,
    pub dark_sidebar: Background,
    pub light_window: Background,
    pub light_sidebar: Background,
}
impl Default for Backgrounds {
    fn default() -> Self {
        use {BackgroundTarget::*, ThemeVariant::*};
        Self {
            dark_window: Background::default_for(Dark, Window),
            dark_sidebar: Background::default_for(Dark, Sidebar),
            light_window: Background::default_for(Light, Window),
            light_sidebar: Background::default_for(Light, Sidebar),
        }
    }
}
impl Backgrounds {
    pub fn get(&self, variant: ThemeVariant, target: BackgroundTarget) -> &Background {
        match (variant, target) {
            (ThemeVariant::Dark, BackgroundTarget::Window) => &self.dark_window,
            (ThemeVariant::Dark, BackgroundTarget::Sidebar) => &self.dark_sidebar,
            (ThemeVariant::Light, BackgroundTarget::Window) => &self.light_window,
            (ThemeVariant::Light, BackgroundTarget::Sidebar) => &self.light_sidebar,
        }
    }
    pub fn get_mut(&mut self, variant: ThemeVariant, target: BackgroundTarget) -> &mut Background {
        match (variant, target) {
            (ThemeVariant::Dark, BackgroundTarget::Window) => &mut self.dark_window,
            (ThemeVariant::Dark, BackgroundTarget::Sidebar) => &mut self.dark_sidebar,
            (ThemeVariant::Light, BackgroundTarget::Window) => &mut self.light_window,
            (ThemeVariant::Light, BackgroundTarget::Sidebar) => &mut self.light_sidebar,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backgrounds_round_trip_through_their_stored_form() {
        let mut background = Background::default_for(ThemeVariant::Dark, BackgroundTarget::Window);
        background.mode = BackgroundMode::Gradient;
        let encoded = background.encode();
        assert_eq!(Background::decode(&encoded), Some(background));
        assert!(encoded.contains("\"mode\":\"gradient\""));
        assert!(encoded.contains("\"#141a26\""));
    }

    #[test]
    fn invalid_backgrounds_are_rejected() {
        let valid = r##"{"mode":"solid","solid":"#AABBCC","gradient":{"angle":359,"stops":[{"color":"#000000","position":0},{"color":"#ffffff","position":100}]}}"##;
        let decoded = Background::decode(valid).unwrap();
        assert_eq!(decoded.solid, HexColor(0xaabbcc));
        for invalid in [
            valid.replace("359", "360"),
            valid.replace("\"position\":100", "\"position\":101"),
            valid.replace("solid\",", "tiled\","),
            valid.replace("#AABBCC", "#abc"),
            valid.replace("#AABBCC", "red"),
            valid.replace(",{\"color\":\"#ffffff\",\"position\":100}", ""),
            valid.replace("\"angle\"", "\"extra\":1,\"angle\""),
            "x".repeat(4096),
        ] {
            assert_eq!(Background::decode(&invalid), None, "{invalid}");
        }
        let seven = (0..7)
            .map(|i| format!("{{\"color\":\"#000000\",\"position\":{i}}}"))
            .collect::<Vec<_>>()
            .join(",");
        let too_many = format!(
            r##"{{"mode":"theme","solid":"#000000","gradient":{{"angle":0,"stops":[{seven}]}}}}"##
        );
        assert_eq!(Background::decode(&too_many), None);
    }
}
