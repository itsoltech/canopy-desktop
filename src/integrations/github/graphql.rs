use serde::Deserialize;
pub(super) fn validate_cursor(cursor: Option<&str>) -> Result<(), String> {
    if cursor.is_some_and(|s| s.is_empty() || s.len() > 4096 || s.chars().any(char::is_control)) {
        Err("Invalid page cursor. Refresh to restart.".into())
    } else {
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}
impl PageInfo {
    pub(super) fn continuation(self, previous: Option<&str>) -> Result<Option<String>, String> {
        if !self.has_next_page {
            return Ok(None);
        }
        let cursor = self
            .end_cursor
            .ok_or("GitHub returned an invalid pagination cursor. Refresh to restart.")?;
        validate_cursor(Some(&cursor))?;
        if Some(cursor.as_str()) == previous {
            return Err("GitHub repeated its pagination cursor. Refresh to restart.".into());
        }
        Ok(Some(cursor))
    }
}
