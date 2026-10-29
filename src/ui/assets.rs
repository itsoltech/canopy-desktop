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
            "canopy/bang.svg" => Some(include_bytes!("../../assets/icons/bang.svg")),
            "canopy/shield.svg" => Some(include_bytes!("../../assets/icons/shield.svg")),
            "canopy/keyboard.svg" => Some(include_bytes!("../../assets/icons/keyboard.svg")),
            "canopy/reply.svg" => Some(include_bytes!("../../assets/icons/reply.svg")),
            "canopy/rotate-ccw.svg" => Some(include_bytes!("../../assets/icons/rotate-ccw.svg")),
            "canopy/box.svg" => Some(include_bytes!("../../assets/icons/box.svg")),
            "canopy/monitor.svg" => Some(include_bytes!("../../assets/icons/monitor.svg")),
            "canopy/cpu.svg" => Some(include_bytes!("../../assets/icons/cpu.svg")),
            "canopy/target.svg" => Some(include_bytes!("../../assets/icons/target.svg")),
            "canopy/diamond.svg" => Some(include_bytes!("../../assets/icons/diamond.svg")),
            "canopy/braces.svg" => Some(include_bytes!("../../assets/icons/braces.svg")),
            "canopy/wrench.svg" => Some(include_bytes!("../../assets/icons/wrench.svg")),
            "canopy/sparkles.svg" => Some(include_bytes!("../../assets/icons/sparkles.svg")),
            "canopy/download.svg" => Some(include_bytes!("../../assets/icons/download.svg")),
            "canopy/upload.svg" => Some(include_bytes!("../../assets/icons/upload.svg")),
            "canopy/brain.svg" => Some(include_bytes!("../../assets/icons/brain.svg")),
            "canopy/pencil.svg" => Some(include_bytes!("../../assets/icons/pencil.svg")),
            "canopy/paperclip.svg" => Some(include_bytes!("../../assets/icons/paperclip.svg")),
            "canopy/quote.svg" => Some(include_bytes!("../../assets/icons/quote.svg")),
            "canopy/heart-filled.svg" => {
                Some(include_bytes!("../../assets/icons/heart-filled.svg"))
            }
            "canopy/git-pull-request.svg" => {
                Some(include_bytes!("../../assets/icons/git-pull-request.svg"))
            }
            "canopy/git-pull-request-draft.svg" => Some(include_bytes!(
                "../../assets/icons/git-pull-request-draft.svg"
            )),
            "canopy/git-merge.svg" => Some(include_bytes!("../../assets/icons/git-merge.svg")),
            "canopy/git-pull-request-closed.svg" => Some(include_bytes!(
                "../../assets/icons/git-pull-request-closed.svg"
            )),
            "canopy/square-slash.svg" => {
                Some(include_bytes!("../../assets/icons/square-slash.svg"))
            }
            "canopy/square-terminal.svg" => {
                Some(include_bytes!("../../assets/icons/square-terminal.svg"))
            }
            "canopy/arrow-right-left.svg" => {
                Some(include_bytes!("../../assets/icons/arrow-right-left.svg"))
            }
            "canopy/grip-vertical.svg" => {
                Some(include_bytes!("../../assets/icons/grip-vertical.svg"))
            }
            "canopy/message-circle-plus.svg" => {
                Some(include_bytes!("../../assets/icons/message-circle-plus.svg"))
            }
            _ => super::file_icons::load(path),
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
