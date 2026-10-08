use gpui_kit::*;
use std::borrow::Cow;

pub struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let custom: Option<&'static [u8]> = match path {
            "canopy/jira.svg" => Some(include_bytes!("../../assets/icons/jira.svg")),
            "canopy/git-branch.svg" => Some(include_bytes!("../../assets/icons/git-branch.svg")),
            "canopy/code.svg" => Some(include_bytes!("../../assets/icons/code.svg")),
            "canopy/terminal.svg" => Some(include_bytes!("../../assets/icons/terminal.svg")),
            "canopy/claude.svg" => Some(include_bytes!("../../assets/icons/claude.svg")),
            "canopy/openai.svg" => Some(include_bytes!("../../assets/icons/openai.svg")),
            "canopy/gemini.svg" => Some(include_bytes!("../../assets/icons/gemini.svg")),
            "canopy/shield.svg" => Some(include_bytes!("../../assets/icons/shield.svg")),
            "canopy/keyboard.svg" => Some(include_bytes!("../../assets/icons/keyboard.svg")),
            "canopy/diamond.svg" => Some(include_bytes!("../../assets/icons/diamond.svg")),
            "canopy/braces.svg" => Some(include_bytes!("../../assets/icons/braces.svg")),
            "canopy/wrench.svg" => Some(include_bytes!("../../assets/icons/wrench.svg")),
            "canopy/sparkles.svg" => Some(include_bytes!("../../assets/icons/sparkles.svg")),
            "canopy/download.svg" => Some(include_bytes!("../../assets/icons/download.svg")),
            "canopy/upload.svg" => Some(include_bytes!("../../assets/icons/upload.svg")),
            _ => None,
        };
        match custom {
            Some(bytes) => Ok(Some(Cow::Borrowed(bytes))),
            None => gpui_kit::assets::Assets.load(path),
        }
    }
    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        gpui_kit::assets::Assets.list(path)
    }
}
