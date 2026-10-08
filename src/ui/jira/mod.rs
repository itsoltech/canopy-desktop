pub(crate) mod field;
pub mod form;
pub mod panel;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JiraMode {
    Edit,
    Transition,
    Link,
    LogWork,
    Sprint,
    Delete,
}
impl JiraMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Edit => "Edit fields",
            Self::Transition => "Change status",
            Self::Link => "Link issue",
            Self::LogWork => "Log work",
            Self::Sprint => "Move to sprint",
            Self::Delete => "Delete issue",
        }
    }
}
