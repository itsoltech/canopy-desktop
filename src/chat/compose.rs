//! The message as the agent reads it (DEC-071): badges in the text become what each
//! stands for — a command at the start, quotes as quote blocks, pull requests by
//! number with their details after the text, a `!` command's result.
use super::provider::Provider;
use super::transcript::Attachment;
use std::path::PathBuf;

/// What goes to the agent for one message.
#[derive(Debug, Default, PartialEq)]
pub struct Composed {
    pub text: String,
    /// Codex skills to send along (`/name` and `$name` badges): name and path.
    pub skills: Vec<(String, PathBuf)>,
}

/// `text` with its badges expanded for `provider`. Badges are found in the order of
/// `attachments` (the order they appear in); one whose mention isn't in the text goes
/// before it.
pub fn expand(text: &str, attachments: &[Attachment], provider: Provider) -> Composed {
    let mut out = String::new();
    let mut skills = vec![];
    let mut pulls = vec![];
    let mut from = 0;
    for attachment in attachments {
        let found = attachment
            .label()
            .and_then(|label| Some((text[from..].find(label)? + from, label.len())));
        match (attachment, found) {
            (Attachment::Quote { text: quoted, .. }, Some((at, len))) => {
                // A quote is a block of its own: no spaces around it.
                out.push_str(text[from..at].trim_end_matches([' ', '\t']));
                block(quoted, &mut out);
                from = at + len;
                while text[from..].starts_with([' ', '\t']) {
                    from += 1;
                }
            }
            (Attachment::Quote { text: quoted, .. }, None) if from == 0 => block(quoted, &mut out),
            (Attachment::Command { name, path, .. }, found) => {
                if provider == Provider::Codex
                    && let Some(path) = path
                {
                    skills.push((name.clone(), path.clone()));
                }
                let word = match provider {
                    Provider::Claude => format!("/{name}"),
                    Provider::Codex => format!("${name}"),
                };
                match found {
                    Some((at, len)) => {
                        out.push_str(&text[from..at]);
                        out.push_str(&word);
                        from = at + len;
                    }
                    None if from == 0 => {
                        out.push_str(&word);
                        out.push(' ');
                    }
                    None => {}
                }
            }
            (Attachment::Pull { number, .. }, found) => {
                pulls.push(attachment);
                if let Some((at, len)) = found {
                    out.push_str(&text[from..at]);
                    out.push_str(&format!("PR #{number}"));
                    from = at + len;
                }
            }
            (
                Attachment::Shell {
                    command,
                    exit,
                    stopped,
                    output,
                    ..
                },
                found,
            ) => {
                let message = shell_message(command, *exit, *stopped, output);
                match found {
                    Some((at, len)) => {
                        out.push_str(&text[from..at]);
                        out.push_str(&message);
                        from = at + len;
                    }
                    None if from == 0 => {
                        out.push_str(&message);
                        out.push_str("\n\n");
                    }
                    None => {}
                }
            }
            _ => {}
        }
    }
    out.push_str(&text[from..]);
    let mut text = out.trim().to_owned();
    if !pulls.is_empty() {
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str("Referenced pull requests:");
        for pull in pulls {
            if let Attachment::Pull {
                number,
                title,
                url,
                head,
                base,
                state,
                body,
                ..
            } = pull
            {
                text.push_str(&format!(
                    "\n- #{number} {title} — {url} ({head} → {base}, {state})"
                ));
                let body = body.trim();
                if !body.is_empty() {
                    for line in body.lines() {
                        text.push_str("\n  ");
                        text.push_str(line);
                    }
                }
            }
        }
    }
    Composed { text, skills }
}

/// The message text as plain prose (for a summary): badges expanded the Claude way.
pub fn plain(text: &str, attachments: &[Attachment]) -> String {
    expand(text, attachments, Provider::Claude).text
}

/// What the agent is told about a `!` command the user ran.
pub fn shell_message(command: &str, exit: Option<i32>, stopped: bool, output: &str) -> String {
    let fence = fence_for(output);
    let status = match (stopped, exit) {
        (true, _) => "It was stopped before it finished.".to_owned(),
        (false, Some(code)) => format!("Exit code: {code}."),
        (false, None) => "It ended without an exit code (killed by a signal).".to_owned(),
    };
    let output = if output.trim().is_empty() {
        "It printed nothing.".to_owned()
    } else {
        format!("Output:\n{fence}\n{}\n{fence}", output.trim_end())
    };
    format!(
        "I ran this shell command myself (not you), in the chat's folder:\n```sh\n{command}\n```\n{status}\n{output}"
    )
}

/// A code fence longer than any backtick run in `text`.
fn fence_for(text: &str) -> String {
    let mut fence = "```".to_owned();
    while text.contains(fence.as_str()) {
        fence.push('`');
    }
    fence
}

/// `quoted` as a Markdown quote on lines of its own.
fn block(quoted: &str, out: &mut String) {
    let kept = out.trim_end().len();
    out.truncate(kept);
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    for line in quoted.lines() {
        out.push('>');
        if !line.is_empty() {
            out.push(' ');
            out.push_str(line);
        }
        out.push('\n');
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::quotes::quote;

    fn quoted(text: &str) -> (Attachment, String) {
        let quote = quote(text).expect("quote");
        let label = quote.label().unwrap().to_owned();
        (quote, label)
    }

    fn claude(text: &str, attachments: &[Attachment]) -> String {
        expand(text, attachments, Provider::Claude).text
    }

    #[test]
    fn quotes_become_blocks_where_their_badges_are() {
        let (first, a) = quoted("Rows::insert creates groups\nwhen a reply arrives");
        let (second, b) = quoted("short");
        let text = format!("why {a} and not {b} here?");
        assert_eq!(
            claude(&text, &[first, second]),
            "why\n\n> Rows::insert creates groups\n> when a reply arrives\n\nand not\n\n> short\n\nhere?"
        );
    }

    #[test]
    fn a_message_of_only_a_quote_and_one_not_mentioned() {
        let (only, label) = quoted("all of it");
        assert_eq!(claude(&format!("{label} "), &[only]), "> all of it");
        let (unmentioned, _) = quoted("lost badge");
        assert_eq!(claude("hello", &[unmentioned]), "> lost badge\n\nhello");
        assert_eq!(claude("plain", &[]), "plain");
    }

    #[test]
    fn commands_lead_the_message_for_each_agent() {
        let command = Attachment::Command {
            name: "review".into(),
            skill: false,
            path: Some("/skills/review/SKILL.md".into()),
            description: None,
            label: Some("[/review]".into()),
        };
        let text = "[/review] the last commit";
        assert_eq!(
            expand(text, std::slice::from_ref(&command), Provider::Claude),
            Composed {
                text: "/review the last commit".into(),
                skills: vec![],
            }
        );
        assert_eq!(
            expand(text, std::slice::from_ref(&command), Provider::Codex),
            Composed {
                text: "$review the last commit".into(),
                skills: vec![("review".into(), "/skills/review/SKILL.md".into())],
            }
        );
    }

    #[test]
    fn a_skill_inside_the_message_stays_where_it_was_typed() {
        let skill = Attachment::Command {
            name: "design".into(),
            skill: true,
            path: Some("/skills/design/SKILL.md".into()),
            description: None,
            label: Some("[$design]".into()),
        };
        let text = "restyle the dialog with [$design] please";
        assert_eq!(
            claude(text, std::slice::from_ref(&skill)),
            "restyle the dialog with /design please"
        );
        assert_eq!(
            expand(text, std::slice::from_ref(&skill), Provider::Codex),
            Composed {
                text: "restyle the dialog with $design please".into(),
                skills: vec![("design".into(), "/skills/design/SKILL.md".into())],
            }
        );
    }

    #[test]
    fn pull_requests_are_named_and_detailed_after_the_text() {
        let pull = Attachment::Pull {
            number: 42,
            title: "Add find".into(),
            url: "https://github.com/o/r/pull/42".into(),
            head: "feat/find".into(),
            base: "main".into(),
            state: "open".into(),
            body: "Adds ⌘F.\nAnd more.".into(),
            label: Some("[#42 Add find]".into()),
        };
        assert_eq!(
            claude("review [#42 Add find] please", &[pull]),
            "review PR #42 please\n\nReferenced pull requests:\n\
             - #42 Add find — https://github.com/o/r/pull/42 (feat/find → main, open)\n  Adds ⌘F.\n  And more."
        );
    }

    #[test]
    fn a_shell_result_tells_what_ran_and_how_it_ended() {
        let shell = Attachment::Shell {
            command: "cargo test".into(),
            exit: Some(101),
            stopped: false,
            output: "test failed\n```odd```".into(),
            label: None,
        };
        let text = claude("", &[shell]);
        assert!(
            text.starts_with("I ran this shell command myself (not you)"),
            "{text}"
        );
        assert!(text.contains("```sh\ncargo test\n```\nExit code: 101."));
        assert!(text.contains("Output:\n````\ntest failed\n```odd```\n````"));
        assert!(shell_message("sleep 9", None, true, "").contains("stopped before it finished"));
    }
}
