use super::comment_card::CommentCard;
use crate::{
    app_state::{AppState, WriteFinished},
    ui::{
        components::integrations::integration_message, components::task_detail as view,
        components::*, theme as t,
    },
};
use canopy_desktop::integrations::{
    TaskComment, TaskRef, TaskWrite, WriteReceipt, client, credentials,
};
use gpui_kit::{
    base::Disableable,
    component::{IconName, scroll::ScrollableElement},
    *,
};
const LIMIT: usize = 200;
pub struct CommentsView {
    task: TaskRef,
    composer: Entity<crate::ui::task_edit::comment_editor::CommentEditor>,
    writing: bool,
    _events: Vec<Subscription>,
    items: Vec<TaskComment>,
    cards: Vec<Entity<CommentCard>>,
    list: ListState,
    pub total: Option<usize>,
    next: Option<String>,
    loading: bool,
    error: Option<String>,
    interactive: bool,
    closed: bool,
    read: Option<Task<()>>,
}
impl CommentsView {
    pub fn new(task: TaskRef, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let composer = cx.new(|cx| {
            crate::ui::task_edit::comment_editor::CommentEditor::new(task.clone(), None, window, cx)
        });
        let state = cx.global::<AppState>().integrations.clone();
        let events = vec![
            cx.subscribe(&state, |this, _, event: &WriteFinished, cx| {
                if !event
                    .command
                    .task()
                    .is_some_and(|t| t.same_task(&this.task))
                {
                    return;
                }
                if let Ok(receipt) = &event.result {
                    this.apply_write(&event.command, receipt, cx);
                }
            }),
            cx.observe(&state, |_, _, cx| cx.notify()),
        ];
        let mut this = Self {
            task,
            composer,
            writing: false,
            _events: events,
            items: vec![],
            cards: vec![],
            list: ListState::new(0, ListAlignment::Top, px(300.)),
            total: None,
            next: None,
            loading: false,
            error: None,
            interactive: false,
            closed: false,
            read: None,
        };
        this.load(true, cx);
        this
    }
    pub fn enable(&mut self, interactive: bool, cx: &mut Context<Self>) {
        self.interactive = interactive;
        self.composer.update(cx, |e, cx| e.enable(interactive, cx));
        for card in &self.cards {
            card.update(cx, |card, cx| card.enable(interactive, cx));
        }
        cx.notify();
    }
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        self.closed = true;
        self.read = None;
        self.loading = false;
        self.enable(false, cx);
    }
    fn sync_footer(&self) {
        let wanted = self.items.len() + usize::from(self.next.is_some());
        let current = self.list.item_count();
        if current > wanted {
            self.list.splice(wanted..current, 0);
        } else if current < wanted {
            self.list.splice(current..current, wanted - current);
        }
    }
    fn merge(&mut self, comment: TaskComment, cx: &mut Context<Self>) {
        if let Some(index) = self.items.iter().position(|c| c.id == comment.id) {
            self.items[index] = comment.clone();
            self.cards[index].update(cx, |s, cx| s.set_comment(comment, self.interactive, cx));
            return;
        }
        if self.items.len() >= LIMIT {
            return;
        }
        let index = self
            .items
            .partition_point(|c| (&c.created_at, &c.id) <= (&comment.created_at, &comment.id));
        self.list.splice(index..index, 1);
        self.items.insert(index, comment.clone());
        let list = self.list.clone();
        let task = self.task.clone();
        let interactive = self.interactive;
        self.cards.insert(
            index,
            cx.new(|cx| CommentCard::new(task, comment, interactive, index, list, cx)),
        );
        for (i, card) in self.cards.iter().enumerate().skip(index + 1) {
            card.read(cx).reindex(i);
        }
    }
    fn apply_write(&mut self, command: &TaskWrite, receipt: &WriteReceipt, cx: &mut Context<Self>) {
        if self.closed || (receipt.comment.is_none() && receipt.deleted_comment.is_none()) {
            return;
        }
        self.read = None;
        self.loading = false;

        if let Some(comment) = &receipt.comment {
            let new = !self.items.iter().any(|c| c.id == comment.id);
            self.merge(comment.clone(), cx);
            if new {
                self.total = self.total.map(|n| n + 1);
            }
            self.sync_footer();
            if matches!(command, TaskWrite::AddComment { .. }) {
                self.list.scroll_to_end();
            }
        }
        if let Some(id) = &receipt.deleted_comment
            && let Some(index) = self.items.iter().position(|c| &c.id == id)
        {
            self.items.remove(index);
            self.cards.remove(index);
            self.list.splice(index..index + 1, 0);
            for (i, card) in self.cards.iter().enumerate().skip(index) {
                card.read(cx).reindex(i);
            }
            self.total = self.total.map(|n| n.saturating_sub(1));
        }
        if self.total.is_none() {
            self.load(false, cx);
        }
        cx.notify();
    }
    fn load(&mut self, refresh: bool, cx: &mut Context<Self>) {
        if self.closed || self.loading {
            return;
        }
        let state = cx.global::<AppState>().integrations.read(cx);
        let Some(account) = state.config.account_for(&self.task.project).cloned() else {
            self.error = Some("Connect this repository in Preferences → Integrations.".into());
            cx.notify();
            return;
        };
        let cursor = if refresh { None } else { self.next.clone() };
        let task = self.task.clone();
        let http = cx.http_client();
        self.loading = true;
        self.error = None;
        cx.notify();
        self.read = Some(cx.spawn(async move |this, cx| {
            let result = async {
                let token = cx
                    .background_executor()
                    .spawn({
                        let credential = account.credential.clone();
                        async move { credentials::load(&credential) }
                    })
                    .await?;
                client(http, &account, token)?
                    .comments(&task, cursor.as_deref())
                    .await
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                if this.closed {
                    return;
                }
                this.loading = false;
                match result {
                    Ok(page) => {
                        if refresh {
                            this.items.clear();
                            this.cards.clear();
                            this.list.reset(0);
                        }
                        this.total = Some(page.total_count);
                        this.next = page.next_cursor;
                        for comment in page.comments {
                            this.merge(comment, cx);
                        }
                        if this.items.len() >= LIMIT {
                            this.next = None;
                        }
                        this.sync_footer();
                    }
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        }));
    }
}
impl Render for CommentsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        column()
            .size_full()
            .gap(px(8.))
            .child(view::comments_toolbar(
                self.items.len(),
                self.total,
                icon_button(
                    "refresh-task-comments",
                    IconName::RotateCw,
                    "Refresh comments",
                )
                .disabled(!self.interactive || self.loading)
                .on_click(cx.listener(|this, _, _, cx| this.load(true, cx))),
            ))
            .children(
                self.error
                    .clone()
                    .map(|error| integration_message(error, true)),
            )
            .children((self.loading && self.items.is_empty()).then(view::comments_loading))
            .children(
                (!self.loading && self.error.is_none() && self.items.is_empty())
                    .then(|| view::comments_empty().flex_1().justify_center()),
            )
            .children((!self.cards.is_empty() || self.next.is_some()).then(|| {
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        list(
                            self.list.clone(),
                            cx.processor(|this, index: usize, _, cx| {
                                if let Some(card) = this.cards.get(index) {
                                    return card.clone().into_any_element();
                                }
                                row()
                                    .h(px(48.))
                                    .justify_center()
                                    .child(
                                        button(
                                            "more-task-comments",
                                            if this.loading {
                                                "Loading…"
                                            } else {
                                                "Load more comments"
                                            },
                                        )
                                        .disabled(!this.interactive || this.loading)
                                        .on_click(
                                            cx.listener(|this, _, _, cx| this.load(false, cx)),
                                        ),
                                    )
                                    .into_any_element()
                            }),
                        )
                        .size_full(),
                    )
                    .vertical_scrollbar(&self.list)
            }))
            .child(
                column()
                    .flex_shrink_0()
                    .gap(px(8.))
                    .pt(px(12.))
                    .border_t_1()
                    .border_color(t::border())
                    .child(
                        button(
                            "toggle-comment-composer",
                            if self.writing {
                                "Hide comment editor"
                            } else {
                                "Add a comment"
                            },
                        )
                        .disabled(!self.interactive)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.writing = !this.writing;
                            cx.notify();
                        })),
                    )
                    .children(self.writing.then(|| self.composer.clone())),
            )
            .children(
                (self.items.len() == LIMIT && self.total.is_some_and(|n| n > LIMIT)).then(|| {
                    div().text_size(px(11.)).text_color(t::muted()).child(
                        "Showing 200 comments. Open the task in your browser to read the rest.",
                    )
                }),
            )
    }
}
