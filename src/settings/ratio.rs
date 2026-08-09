//! Output shape, and the social-media sizes worth having one click away.

use serde::{Deserialize, Serialize};

/// `Auto` grows the canvas to fit the screenshot plus padding; the others pin
/// the canvas and fit the screenshot inside it.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub enum Ratio {
    Auto,
    /// width / height
    Aspect(f32),
    /// Exact pixel size, used by the social-media presets.
    Size(u32, u32),
}

pub struct RatioPreset {
    pub name: &'static str,
    pub ratio: Ratio,
}

pub const RATIO_PRESETS: &[RatioPreset] = &[
    RatioPreset {
        name: "Auto",
        ratio: Ratio::Auto,
    },
    RatioPreset {
        name: "4:3",
        ratio: Ratio::Aspect(4.0 / 3.0),
    },
    RatioPreset {
        name: "3:2",
        ratio: Ratio::Aspect(3.0 / 2.0),
    },
    RatioPreset {
        name: "16:9",
        ratio: Ratio::Aspect(16.0 / 9.0),
    },
    RatioPreset {
        name: "1:1",
        ratio: Ratio::Aspect(1.0),
    },
    RatioPreset {
        name: "Twitter",
        ratio: Ratio::Size(1600, 900),
    },
    RatioPreset {
        name: "Facebook",
        ratio: Ratio::Size(1200, 630),
    },
    RatioPreset {
        name: "Instagram",
        ratio: Ratio::Size(1080, 1080),
    },
    RatioPreset {
        name: "LinkedIn",
        ratio: Ratio::Size(1200, 627),
    },
    RatioPreset {
        name: "Youtube",
        ratio: Ratio::Size(1280, 720),
    },
    RatioPreset {
        name: "Pinterest",
        ratio: Ratio::Size(1000, 1500),
    },
    RatioPreset {
        name: "Reddit",
        ratio: Ratio::Size(1200, 628),
    },
    RatioPreset {
        name: "Snapchat",
        ratio: Ratio::Size(1080, 1920),
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ratio_preset_is_reachable_and_sane() {
        for p in RATIO_PRESETS {
            match p.ratio {
                Ratio::Aspect(r) => assert!(r > 0.0, "{} has a non-positive ratio", p.name),
                Ratio::Size(w, h) => assert!(w > 0 && h > 0, "{} has a zero dimension", p.name),
                Ratio::Auto => {}
            }
        }
    }
}
