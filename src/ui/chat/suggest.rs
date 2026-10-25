//! Suggestions in the composer (DEC-071): `/` the agent's commands and skills at the
//! start of the message, `$` its skills in any word, `@` files and folders of the
//! chat's folder, `#` pull requests of its GitHub repository. The chosen one replaces
//! the typed word with its badge.
use super::{mentions, view::ChatView};
use crate::app_state::AppState;
use crate::ui::{components::*, theme as t};
use canopy_desktop::chat::{closed, config::CommandInfo, goal, transcript::Attachment};
use canopy_desktop::motion::{self, Presence, PresenceSpec, presets};
use gpui_kit::component::IconName;
use gpui_kit::{prelude::FluentBuilder, *};
use std::{
    ops::Range,
    path::Path,
    time::{Duration, Instant},
};

const ROW: f32 = 32.;
/// The tray slides out and back like the decision panel.
const TRAY: PresenceSpec = presets::PANEL;
const VISIBLE: usize = 8;
const MAX: usize = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Command,
    Skill,
    File,
    Pull,
}

/// The word being typed after a trigger character, and what it asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Trigger {
    pub kind: Kind,
    /// The word with its trigger character, in the composer's text.
    pub range: Range<usize>,
    pub query: String,
}

/// The trigger word ending at `cursor`: `/` only at the very start of the message, `$`,
/// `@` and `#` at the start of any word. Badges (`tokens`) are never part of a word.
pub(super) fn trigger(text: &str, cursor: usize, tokens: &[Range<usize>]) -> Option<Trigger> {
    if cursor > text.len() || !text.is_char_boundary(cursor) {
        return None;
    }
    if tokens
        .iter()
        .any(|token| token.start < cursor && cursor < token.end)
    {
        return None;
    }
    // The word starts after the last space or badge before the cursor.
    let space = text[..cursor]
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map_or(0, |(at, c)| at + c.len_utf8());
    let badge = tokens
        .iter()
        .map(|token| token.end)
        .filter(|end| *end <= cursor)
        .max()
        .unwrap_or(0);
    let start = space.max(badge);
    let word = &text[start..cursor];
    let kind = match word.chars().next()? {
        '/' if start == 0 => Kind::Command,
        '$' => Kind::Skill,
        '@' => Kind::File,
        '#' => Kind::Pull,
        _ => return None,
    };
    Some(Trigger {
        kind,
        range: start..cursor,
        query: word[1..].to_owned(),
    })
}

/// Commands matching `query`: names starting with it, then containing it, then those
/// whose description mentions it.
pub(super) fn rank_commands<'a>(commands: &'a [CommandInfo], query: &str) -> Vec<&'a CommandInfo> {
    let query = query.to_lowercase();
    let mut ranked: Vec<(u8, &CommandInfo)> = commands
        .iter()
        .filter_map(|command| {
            let name = command.name.to_lowercase();
            let rank = if name.starts_with(&query) {
                0
            } else if name.contains(&query) {
                1
            } else if command.description.to_lowercase().contains(&query) {
                2
            } else {
                return None;
            };
            Some((rank, command))
        })
        .collect();
    ranked.sort_by_key(|(rank, _)| *rank);
    ranked.into_iter().take(MAX).map(|(_, c)| c).collect()
}

#[derive(Clone)]
enum Glyph {
    Custom(&'static str),
    Named(IconName),
}

/// One suggestion: how it reads in the list and the badge it becomes.
#[derive(Clone)]
pub(super) struct Choice {
    glyph: Glyph,
    title: String,
    detail: String,
    /// The badge's text in the composer.
    name: String,
    attachment: Attachment,
}

pub(super) struct Suggest {
    pub trigger: Trigger,
    /// Shown; false while the tray slides away.
    pub open: bool,
    choices: Vec<Choice>,
    selected: usize,
    loading: bool,
    error: Option<String>,
    presence: Presence,
    scroll: ScrollHandle,
    _task: Option<Task<()>>,
    _cancel: Option<canopy_desktop::files::CancelGuard>,
}

/// A command after `/`, or a skill after `$` (`skill`), and the badge it becomes.
fn command_choice(command: &CommandInfo, skill: bool) -> Choice {
    let description: String = command.description.chars().take(300).collect();
    let attachment = Attachment::Command {
        name: command.name.clone(),
        skill,
        path: command.path.clone(),
        description: Some(description.clone()).filter(|d| !d.is_empty()),
        label: None,
    };
    let label = mentions::mention_of(&attachment);
    if skill {
        return Choice {
            glyph: Glyph::Custom("box"),
            title: format!("${}", command.name),
            detail: description.lines().next().unwrap_or_default().to_owned(),
            name: mentions::skill_title(&command.name),
            attachment: attachment.with_label(label),
        };
    }
    Choice {
        glyph: Glyph::Custom("square-slash"),
        title: match &command.hint {
            Some(hint) => format!("/{} {hint}", command.name),
            None => format!("/{}", command.name),
        },
        detail: description.lines().next().unwrap_or_default().to_owned(),
        name: format!("/{}", command.name),
        attachment: attachment.with_label(label),
    }
}

fn file_choice(root: &Path, found: canopy_desktop::files::search::Mention) -> Choice {
    let relative = found.path.to_string_lossy().into_owned();
    let name = found
        .path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| relative.clone());
    let slash = if found.folder { "/" } else { "" };
    Choice {
        glyph: Glyph::Named(if found.folder {
            IconName::Folder
        } else {
            IconName::File
        }),
        title: format!("{name}{slash}"),
        detail: found
            .path
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_default(),
        name: format!("{name}{slash}"),
        attachment: Attachment::Path {
            path: root.join(&found.path),
            label: Some(format!("@{relative}{slash}")),
        },
    }
}

fn pull_choice(pull: canopy_desktop::integrations::github::PullRef) -> Choice {
    let detail = format!(
        "{} · {} → {} · {}",
        pull.author, pull.head, pull.base, pull.state
    );
    let attachment = Attachment::Pull {
        number: pull.number,
        title: pull.title.clone(),
        url: pull.url,
        head: pull.head,
        base: pull.base,
        state: pull.state.clone(),
        body: pull.body,
        label: None,
    };
    let label = mentions::mention_of(&attachment);
    Choice {
        glyph: Glyph::Custom(mentions::pull_icon(&pull.state)),
        title: format!("#{} {}", pull.number, pull.title),
        detail,
        name: format!("#{} {}", pull.number, pull.title),
        attachment: attachment.with_label(label),
    }
}

impl ChatView {
    /// The composer changed: show, update or close the suggestions for the word at the
    /// cursor. Searches run in the background; the last one wins.
    pub(super) fn update_suggest(&mut self, cx: &mut Context<Self>) {
        if self.shell_mode(cx) || !self.threads.is_empty() {
            self.close_suggest(cx);
            return;
        }
        let state = self.composer.read(cx);
        let tokens: Vec<Range<usize>> = state.tokens().iter().map(|span| span.range()).collect();
        let Some(trigger) = trigger(&state.value(), state.cursor(), &tokens) else {
            self.close_suggest(cx);
            self.suggest_dismissed = None;
            return;
        };
        if self.suggest_dismissed == Some(trigger.range.start)
            || self
                .suggest
                .as_ref()
                .is_some_and(|s| s.open && s.trigger == trigger)
        {
            return;
        }
        self.suggest_dismissed = None;
        // The previous list stays while the next search runs, so it doesn't flicker.
        let (choices, presence) = match self.suggest.take() {
            Some(old) if old.open && old.trigger.kind == trigger.kind => {
                (old.choices, old.presence)
            }
            _ => {
                let now = Instant::now();
                let mut presence = Presence::new(false, TRAY, now);
                presence.set_open(true, now, motion::policy(cx));
                (vec![], presence)
            }
        };
        let mut suggest = Suggest {
            trigger: trigger.clone(),
            open: true,
            choices,
            selected: 0,
            loading: false,
            error: None,
            presence,
            scroll: ScrollHandle::new(),
            _task: None,
            _cancel: None,
        };
        match trigger.kind {
            Kind::Command => {
                let commands = self.backend(cx).map(|s| s.commands()).unwrap_or_default();
                // `/goal` is Canopy's too (goal mode): offered even where the agent
                // doesn't list it (Codex), never to an agent without goals.
                let mut listed = commands.to_vec();
                let reported = !commands.is_empty();
                if self.capabilities(cx).goal && !listed.iter().any(|c| c.name == goal::COMMAND) {
                    listed.push(CommandInfo {
                        name: goal::COMMAND.into(),
                        description: "Keep working until a condition is met".into(),
                        hint: Some("<condition>".into()),
                        path: None,
                        skill: false,
                    });
                }
                // `/resume` brings back a closed Canopy chat (DEC-094); the agent's own
                // picker doesn't work headless, so Canopy's takes its place.
                listed.retain(|c| c.name != closed::COMMAND);
                listed.push(CommandInfo {
                    name: closed::COMMAND.into(),
                    description: "Resume a closed chat".into(),
                    hint: None,
                    path: None,
                    skill: false,
                });
                let commands = listed.as_slice();
                suggest.choices = rank_commands(commands, &trigger.query)
                    .into_iter()
                    .map(|command| command_choice(command, false))
                    .collect();
                // The agent reports them once it has started.
                suggest.loading = !reported
                    && self.backend(cx).is_none_or(|s| {
                        matches!(s.status(), canopy_desktop::chat::session::Status::Starting)
                    });
            }
            Kind::Skill => {
                let skills: Vec<CommandInfo> = self
                    .backend(cx)
                    .map(|s| s.commands())
                    .unwrap_or_default()
                    .iter()
                    .filter(|command| command.skill)
                    .cloned()
                    .collect();
                suggest.choices = rank_commands(&skills, &trigger.query)
                    .into_iter()
                    .map(|command| command_choice(command, true))
                    .collect();
                suggest.loading = skills.is_empty()
                    && self.backend(cx).is_none_or(|s| {
                        matches!(s.status(), canopy_desktop::chat::session::Status::Starting)
                    });
            }
            Kind::File => match self.cwd(cx) {
                Some(root) => {
                    let cancel = canopy_desktop::files::CancelGuard::default();
                    let signal = cancel.0.clone();
                    let query = trigger.query.clone();
                    let wanted = trigger.clone();
                    suggest.loading = true;
                    suggest._cancel = Some(cancel);
                    suggest._task = Some(cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(80))
                            .await;
                        let search = root.clone();
                        let found = cx
                            .background_executor()
                            .spawn(async move {
                                canopy_desktop::files::search::mentions(
                                    &search, &query, MAX, &signal,
                                )
                            })
                            .await;
                        let _ = this.update(cx, |this, cx| {
                            this.suggested(
                                &wanted,
                                found.map(|found| {
                                    found.into_iter().map(|f| file_choice(&root, f)).collect()
                                }),
                                cx,
                            )
                        });
                    }));
                }
                None => suggest.error = Some("This chat has no folder.".into()),
            },
            Kind::Pull => match self.cwd(cx) {
                Some(cwd) => {
                    let integrations = cx.global::<AppState>().integrations.clone();
                    let query = trigger.query.clone();
                    let wanted = trigger.clone();
                    suggest.loading = true;
                    suggest._task = Some(cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(250))
                            .await;
                        let found = integrations
                            .update(cx, |integrations, cx| {
                                integrations.pull_requests(cwd, query, cx)
                            })
                            .await;
                        let _ = this.update(cx, |this, cx| {
                            this.suggested(
                                &wanted,
                                found.map(|pulls| pulls.into_iter().map(pull_choice).collect()),
                                cx,
                            )
                        });
                    }));
                }
                None => suggest.error = Some("This chat has no folder.".into()),
            },
        }
        self.suggest = Some(suggest);
        cx.notify();
    }

    /// A search for `trigger` finished; a newer word has its own.
    fn suggested(
        &mut self,
        trigger: &Trigger,
        found: Result<Vec<Choice>, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(suggest) = self
            .suggest
            .as_mut()
            .filter(|s| s.open && s.trigger == *trigger)
        else {
            return;
        };
        suggest.loading = false;
        suggest.selected = 0;
        match found {
            Ok(choices) => {
                suggest.choices = choices;
                suggest.error = None;
            }
            Err(error) => {
                suggest.choices.clear();
                suggest.error = Some(error);
            }
        }
        cx.notify();
    }

    /// The tray slides away; its last list stays visible until then.
    pub(super) fn close_suggest(&mut self, cx: &mut Context<Self>) {
        if let Some(suggest) = self.suggest.as_mut().filter(|s| s.open) {
            suggest.open = false;
            suggest._task = None;
            suggest._cancel = None;
            suggest
                .presence
                .set_open(false, Instant::now(), motion::policy(cx));
            cx.notify();
        }
    }

    /// A key of the message field while the tray is open: ↑/↓ choose, Enter/Tab put the
    /// badge in, Esc closes it. `false` leaves the key to the field (Enter with
    /// nothing to choose sends the message as usual).
    pub(super) fn suggest_key(
        &mut self,
        key: SuggestKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(suggest) = self.suggest.as_mut().filter(|s| s.open) else {
            return false;
        };
        let count = suggest.choices.len();
        match key {
            SuggestKey::Up | SuggestKey::Down if count > 0 => {
                suggest.selected = if key == SuggestKey::Up {
                    (suggest.selected + count - 1) % count
                } else {
                    (suggest.selected + 1) % count
                };
                suggest.scroll.scroll_to_item(suggest.selected);
                cx.notify();
                true
            }
            SuggestKey::Accept if count > 0 => {
                let selected = suggest.selected;
                self.choose_suggestion(selected, window, cx);
                true
            }
            SuggestKey::Close => {
                self.suggest_dismissed = Some(suggest.trigger.range.start);
                self.close_suggest(cx);
                true
            }
            _ => false,
        }
    }

    fn choose_suggestion(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(suggest) = self.suggest.as_ref().filter(|s| s.open) else {
            return;
        };
        let Some(choice) = suggest.choices.get(index).cloned() else {
            return;
        };
        let range = suggest.trigger.range.clone();
        self.close_suggest(cx);
        if matches!(&choice.attachment, Attachment::Command { name, skill: false, .. } if name == closed::COMMAND)
        {
            // The list of closed chats instead of a badge; the typed word goes.
            self.composer.update(cx, |state, cx| {
                let text = state.value().to_string();
                let rest = text.get(range.end..).unwrap_or_default().trim_start();
                state.set_value(rest.to_owned(), window, cx);
            });
            self.request_resume(cx);
            cx.notify();
            return;
        }
        if matches!(&choice.attachment, Attachment::Command { name, skill: false, .. } if name == goal::COMMAND)
        {
            // Goal mode instead of a badge: the rest of the message is the condition.
            let value = self.composer.read(cx).value().to_string();
            let rest = value
                .get(range.end..)
                .unwrap_or_default()
                .trim_start()
                .to_owned();
            self.enter_goal_mode(rest, window, cx);
            cx.notify();
            return;
        }
        self.put_mention(range, choice.attachment, &choice.name, window, cx);
        cx.notify();
    }

    /// The tray on the composer's top edge, like the queue, laid over the conversation
    /// (and any other tray): it slides out with the first suggestions and eases its
    /// height as the list changes.
    pub(super) fn suggest_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let now = Instant::now();
        let suggest = self.suggest.as_ref()?;
        let progress = suggest.presence.progress(now);
        let animating = suggest.presence.is_animating(now);
        if !suggest.open && !animating && progress <= 0. {
            self.suggest = None;
            return None;
        }
        motion::request_frame(window, animating);
        let title = match suggest.trigger.kind {
            Kind::Command => format!("{} commands and skills", self.agent_name(cx)),
            Kind::Skill => format!("{} skills", self.agent_name(cx)),
            Kind::File => "Files and folders".to_owned(),
            Kind::Pull => "Pull requests".to_owned(),
        };
        let note = |text: String| {
            row()
                .h(px(ROW))
                .px(px(6.))
                .text_size(px(12.))
                .text_color(t::muted())
                .child(text)
        };
        let body: AnyElement = if let Some(error) = &suggest.error {
            note(error.clone()).into_any_element()
        } else if suggest.choices.is_empty() {
            note(
                if suggest.loading {
                    "Loading…"
                } else {
                    "No matches"
                }
                .to_owned(),
            )
            .into_any_element()
        } else {
            let selected = suggest.selected;
            let open = suggest.open;
            div()
                .id("chat-suggest-list")
                .max_h(px(ROW * VISIBLE as f32))
                .overflow_y_scroll()
                .track_scroll(&suggest.scroll)
                .children(suggest.choices.iter().enumerate().map(|(index, choice)| {
                    row()
                        .id(("chat-suggest", index))
                        .h(px(ROW))
                        .px(px(6.))
                        .gap(px(8.))
                        .rounded(px(6.))
                        .when(open, |row| row.cursor_pointer())
                        .when(index == selected, |row| row.bg(t::selected()))
                        .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                            if *hovered
                                && let Some(suggest) = this.suggest.as_mut().filter(|s| s.open)
                            {
                                suggest.selected = index;
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.choose_suggestion(index, window, cx)
                        }))
                        .child(match &choice.glyph {
                            Glyph::Custom(name) => custom_icon(name)
                                .size(px(14.))
                                .text_color(if *name == "box" {
                                    t::skill()
                                } else {
                                    t::muted()
                                })
                                .into_any_element(),
                            Glyph::Named(name) => icon(name.clone())
                                .size(px(14.))
                                .text_color(t::muted())
                                .into_any_element(),
                        })
                        .child(
                            div()
                                .flex_shrink_0()
                                .max_w(px(320.))
                                .truncate()
                                .text_size(px(13.))
                                .text_color(t::text())
                                .child(choice.title.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(12.))
                                .text_color(t::muted())
                                .child(choice.detail.clone()),
                        )
                }))
                .into_any_element()
        };
        let panel = column()
            .id("chat-suggest-panel")
            // Clicks and the wheel between the rows don't reach the chat beneath.
            .occlude()
            .w_full()
            .px(px(6.))
            .py(px(3.))
            .rounded_t(px(12.))
            .border_1()
            .border_b_0()
            .border_color(t::border())
            // The composer's surface: the tray reads as part of the message field.
            .bg(t::input_bg())
            .child(
                row()
                    .h(px(24.))
                    .px(px(6.))
                    .gap(px(8.))
                    .text_size(px(11.))
                    .text_color(t::faint())
                    .child(title)
                    .child(div().flex_1())
                    .child(if suggest.loading && !suggest.choices.is_empty() {
                        "Searching…"
                    } else {
                        "↑↓ choose · ⏎ insert · esc close"
                    }),
            )
            .child(AnimatedHeight::new(
                "chat-suggest-height",
                motion::policy(cx) == motion::MotionPolicy::Full,
                body,
            ));
        let reveal = gpui_kit::base::MotionReveal::new(
            "chat-suggest-reveal",
            progress,
            div()
                .w_full()
                .relative()
                .top(px(motion::distance::BASE * (1. - progress)))
                .opacity(progress)
                // The composer's tint is translucent; over the conversation it needs the
                // window's solid background beneath it to stay readable.
                .rounded_t(px(12.))
                .bg(t::bg())
                .child(panel)
                .into_any_element(),
        );
        Some(super::render::centered(div().mx(px(14.)).child(reveal)).mb(px(-4.)))
    }
}

/// Keys of the message field the tray takes while it is open.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SuggestKey {
    Up,
    Down,
    Accept,
    Close,
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn triggers_start_words_and_slash_starts_the_message() {
        let at = |text: &str| trigger(text, text.len(), &[]);
        assert_eq!(
            at("/rev").map(|t| (t.kind, t.query)),
            Some((Kind::Command, "rev".into()))
        );
        assert_eq!(at("see /rev"), None);
        assert_eq!(
            at("see @src/ch").map(|t| (t.kind, t.range)),
            Some((Kind::File, 4..11))
        );
        assert_eq!(
            at("fix #12").map(|t| (t.kind, t.query)),
            Some((Kind::Pull, "12".into()))
        );
        assert_eq!(
            at("restyle it with $des").map(|t| (t.kind, t.range, t.query)),
            Some((Kind::Skill, 16..20, "des".into()))
        );
        assert_eq!(at("mail a@b"), None);
        assert_eq!(
            at("@"),
            Some(Trigger {
                kind: Kind::File,
                range: 0..1,
                query: String::new()
            })
        );
        assert_eq!(at("done @x "), None);
        // A badge is never part of the word typed.
        let badge = std::iter::once(0..6).collect::<Vec<_>>();
        assert_eq!(trigger("[@a b]@x", 8, &badge).map(|t| t.range), Some(6..8));
        let badge = std::iter::once(1..6).collect::<Vec<_>>();
        assert_eq!(trigger("@[x y]", 6, &badge), None);
        assert_eq!(
            trigger("żółw @ś", "żółw @ś".len(), &[]).map(|t| t.query),
            Some("ś".into())
        );
    }

    #[test]
    fn commands_rank_by_name_then_description() {
        let command = |name: &str, description: &str| CommandInfo {
            name: name.into(),
            description: description.into(),
            ..Default::default()
        };
        let commands = [
            command("code-review", "Review a PR"),
            command("pr-review", "Review a pull request"),
            command("review", "Look over changes"),
            command("deploy", "Ship it after review"),
        ];
        let names: Vec<_> = rank_commands(&commands, "rev")
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(names, ["review", "code-review", "pr-review", "deploy"]);
        assert_eq!(rank_commands(&commands, "").len(), 4);
    }
}
