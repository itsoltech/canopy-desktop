//! Window shell: Preferences-style navigation, page header, connection status and
//! the overlays owned by this view. Pages render their own content.
use super::*;
use canopy_desktop::motion;
use chrome::*;
use gpui_kit::component::{IconName, button::ButtonVariants, scroll::ScrollableElement};

impl RemoteWindow {
    fn disclosures_active(&self, now: Instant) -> bool {
        [
            &self.advanced,
            &self.details,
            &self.file_diagnostics,
            &self.git_remote,
            &self.worktrees,
        ]
        .iter()
        .any(|disclosure| disclosure.active(now))
    }
    /// Checkout name (branch when known) of the selected workspace.
    pub(super) fn workspace_label(&self) -> Option<String> {
        let reference = self.workspace.as_ref()?;
        self.host
            .as_ref()?
            .shared_catalog()?
            .projects
            .iter()
            .flat_map(|p| &p.checkouts)
            .find(|w| w.catalog_id == reference.catalog_id)
            .map(|w| w.branch.clone().unwrap_or_else(|| w.name.clone()))
    }
    fn heading(&self) -> (String, String) {
        let host = self
            .host
            .as_ref()
            .map(|h| h.name())
            .unwrap_or_else(|| "Remote computer".into());
        let place = self.selection_label();
        match self.page {
            Page::Hosts if self.opening_setup || self.host.is_none() => (
                "Add computer".into(),
                "Connect to Canopy running on another computer.".into(),
            ),
            Page::Hosts => (host, "Connection and routes on this device".into()),
            Page::Catalog | Page::Chat if self.embedded => ("New chat".into(), place),
            Page::Catalog | Page::Chat => ("Projects".into(), host),
            Page::Files => ("Files".into(), place),
            Page::Git => ("Changes".into(), place),
            Page::Notes => ("Notes".into(), place),
            Page::Tasks => ("Tasks".into(), place),
            Page::Settings => (
                "Host settings".into(),
                format!("Changes apply to {host}. Layout and zoom on this device stay separate."),
            ),
            Page::Terminal => ("Terminal".into(), place),
            Page::Terminals => ("Terminals".into(), place),
        }
    }
    fn connection_actions(&self, cx: &mut Context<Self>) -> Option<Div> {
        let host = self.host.clone()?;
        if self.page == Page::Hosts && self.opening_setup {
            return None;
        }
        let phase = host.phase();
        let connecting = phase == Phase::Connecting || self.pending == Some("connection");
        Some(
            row()
                .flex_shrink_0()
                .gap(px(8.))
                .child(
                    row()
                        .gap(px(6.))
                        .text_size(px(12.))
                        .text_color(t::secondary())
                        .child(status_dot(phase))
                        .child(phase_label(phase)),
                )
                .child(
                    button(
                        "connect",
                        if phase == Phase::Ready {
                            "Reconnect"
                        } else {
                            "Connect"
                        },
                    )
                    .loading(connecting)
                    .disabled(self.busy || phase == Phase::Connecting)
                    .on_click(cx.listener(|this, _, _, cx| this.connect(cx))),
                )
                .child(
                    // Also stops automatic retries while offline.
                    button("disconnect", "Disconnect")
                        .ghost()
                        .border_0()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(host) = &this.host
                                && let Err(error) = host.pause_connection()
                            {
                                this.error = Some(error);
                                this.scope = Some("connection");
                            }
                            this.detach();
                            cx.notify();
                        })),
                ),
        )
    }
    /// Offline/connecting explanation and connection failures, under the header.
    fn connection_status(&self, inset: f32) -> Option<Div> {
        let host = self.host.as_ref()?;
        if self.page == Page::Hosts && (self.opening_setup || self.host.is_none()) {
            return None;
        }
        let phase = host.phase();
        let mut items: Vec<AnyElement> = vec![];
        match phase {
            Phase::Offline => {
                let seen = host
                    .last_seen()
                    .and_then(|value| chrono::DateTime::from_timestamp(value, 0))
                    .map(|seen| format!(" Last seen {}.", seen.format("%Y-%m-%d %H:%M UTC")))
                    .unwrap_or_default();
                items.push(
                    hint(format!(
                        "Offline. Showing saved data; connect to make changes.{seen}"
                    ))
                    .into_any_element(),
                );
            }
            Phase::Connecting if self.pending != Some("connection") => {
                items.push(loading("Connecting…").into_any_element())
            }
            _ => {}
        }
        if phase != Phase::Ready
            && let Some(error) = host.connection_error()
        {
            items.push(failure(error).into_any_element());
        }
        items.extend(self.notice("connection"));
        (!items.is_empty()).then(|| {
            column()
                .flex_shrink_0()
                .gap(px(8.))
                .px(px(inset))
                .pt(px(12.))
                .children(items)
        })
    }
    /// Messages not owned by a specific control (layout saves, terminal events).
    fn general_notice(&self) -> Option<Div> {
        if self.scope.is_some() {
            return None;
        }
        if let Some(error) = &self.error {
            return Some(failure(error.clone()));
        }
        (!self.info.is_empty()).then(|| {
            if self.busy {
                loading(self.info.clone())
            } else {
                done(self.info.clone())
            }
        })
    }
    fn navigation(&self, cx: &mut Context<Self>) -> Div {
        let mut computers = nav_group("COMPUTERS", true);
        let hosts = self
            .registry
            .as_ref()
            .map(|registry| registry.hosts())
            .unwrap_or_default();
        if self.registry.is_none() {
            computers = computers.child(div().px(px(12.)).py(px(8.)).child(loading("Loading…")));
        }
        let mut items = column().gap(px(1.));
        for host in hosts {
            let active =
                !self.opening_setup && self.host.as_ref().is_some_and(|h| h.id() == host.id());
            let id = host.id().as_str().to_owned();
            items = items.child(
                host_item(
                    SharedString::from(id),
                    host.name(),
                    host.phase(),
                    active,
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| this.select_host(host.clone(), cx))),
            );
        }
        items = items.child(
            nav_item(
                "add-another-computer",
                "Add computer…",
                icon(IconName::Plus),
                self.page == Page::Hosts && (self.opening_setup || self.host.is_none()),
                cx,
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.opening_setup = true;
                this.page = Page::Hosts;
                this.error = None;
                this.scope = None;
                this.info.clear();
                this.fields["endpoint"].update(cx, |field, cx| field.set_value("", window, cx));
                cx.notify();
            })),
        );
        let mut body = column().child(computers.child(items));
        if self.host.is_some() && !self.opening_setup {
            let page = match self.page {
                Page::Terminal | Page::Terminals | Page::Chat => Page::Catalog,
                page => page,
            };
            for (group, entries) in [
                (
                    "WORKSPACE",
                    vec![
                        (
                            "catalog",
                            "Projects",
                            icon(IconName::LayoutDashboard),
                            Page::Catalog,
                        ),
                        ("files", "Files", icon(IconName::Folder), Page::Files),
                        ("git", "Changes", custom_icon("git-branch"), Page::Git),
                        ("notes", "Notes", icon(IconName::FileText), Page::Notes),
                        ("tasks", "Tasks", custom_icon("issue"), Page::Tasks),
                    ],
                ),
                (
                    "COMPUTER",
                    vec![
                        (
                            "add-host",
                            "Connection",
                            icon(IconName::Network),
                            Page::Hosts,
                        ),
                        (
                            "settings",
                            "Host settings",
                            icon(IconName::Settings2),
                            Page::Settings,
                        ),
                    ],
                ),
            ] {
                body = body.child(nav_group(group, false).child(column().gap(px(1.)).children(
                    entries.into_iter().map(|(id, label, glyph, target)| {
                        nav_item(id, label, glyph, page == target, cx)
                            .on_click(cx.listener(move |this, _, _, cx| this.show_page(target, cx)))
                    }),
                )));
            }
        }
        column()
            .w(px(NAV_WIDTH))
            .flex_shrink_0()
            .h_full()
            .border_r_1()
            .border_color(t::border())
            .child(
                column()
                    .id("remote-navigation")
                    .flex_1()
                    .overflow_y_scrollbar()
                    .py(px(12.))
                    .px(px(8.))
                    .child(body),
            )
    }
    /// Work pages own their inner scrolling; the rest scroll as one document.
    fn fills_height(&self) -> bool {
        matches!(self.page, Page::Files | Page::Git | Page::Notes)
    }
    fn page_content(&mut self, cx: &mut Context<Self>) -> AnyElement {
        match self.page {
            Page::Hosts => self.render_hosts(cx),
            Page::Catalog | Page::Chat => self.render_catalog(cx),
            Page::Files => self.render_files(cx),
            Page::Notes => self.render_notes(cx),
            Page::Git => self.render_git(cx),
            Page::Settings => self.render_settings(cx),
            Page::Tasks => self.render_tasks(cx),
            Page::Terminal => self.render_terminal(cx),
            Page::Terminals => self.render_terminal_picker(cx),
        }
    }
    /// Terminal, terminal list and Tasks own their full area: no inset, no outer scroll.
    fn owns_area(&self) -> bool {
        matches!(self.page, Page::Terminal | Page::Terminals | Page::Tasks)
    }
    fn content_area(&mut self, inset: f32, cx: &mut Context<Self>) -> Div {
        let notice = self.general_notice();
        let content = self.page_content(cx);
        if self.owns_area() {
            return column()
                .flex_1()
                .min_h_0()
                .children(notice.map(|notice| {
                    div()
                        .flex_shrink_0()
                        .px(px(inset))
                        .pt(px(12.))
                        .child(notice)
                }))
                .child(column().flex_1().min_h_0().size_full().child(content));
        }
        if self.fills_height() {
            return column()
                .flex_1()
                .min_h_0()
                .px(px(inset))
                .py(px(if inset == COMPACT_X { 12. } else { 20. }))
                .gap(px(12.))
                .children(notice)
                .child(content);
        }
        column().flex_1().min_h_0().child(
            column()
                .id("remote-content")
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .child(
                    column()
                        .flex_shrink_0()
                        .px(px(inset))
                        .py(px(if inset == COMPACT_X { 12. } else { 24. }))
                        .gap(px(16.))
                        .children(notice)
                        .child(content),
                ),
        )
    }
}

impl Render for RemoteWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let started = std::time::Instant::now();
        let now = Instant::now();
        motion::request_frame(window, self.disclosures_active(now));
        if let Some(inline) = self.inline {
            return self.render_inline(inline, cx);
        }
        self.features.apply_device_preferences(window, cx);
        let overlays = self
            .confirmation
            .clone()
            .map(|m| m.into_any_element())
            .into_iter()
            .chain(self.prompt.clone().map(|m| m.into_any_element()))
            .collect::<Vec<_>>();
        if self.compact_tools {
            let status = self.connection_status(COMPACT_X);
            let content = self.content_area(COMPACT_X, cx);
            return column()
                .track_focus(&self.focus)
                .size_full()
                .min_w_0()
                .min_h_0()
                .text_size(px(13.))
                .text_color(t::text())
                .children(status)
                .child(content)
                .children(overlays)
                .into_any_element();
        }
        let (title, subtitle) = self.heading();
        let inset = if self.embedded { 20. } else { CONTENT_X };
        let header = page_header(title, subtitle, inset).children(self.connection_actions(cx));
        let status = self.connection_status(inset);
        let tabs = self.embedded.then(|| self.embedded_navigation(inset, cx));
        let content = self.content_area(inset, cx);
        let navigation = (!self.embedded).then(|| self.navigation(cx));
        let element = column()
            .track_focus(&self.focus)
            .size_full()
            // Embedded in the main window it lies on the window background.
            .bg(if self.embedded {
                crate::ui::components::background::window_fill(t::bg(), cx)
            } else {
                t::bg()
            })
            .text_color(t::text())
            .text_size(px(13.))
            .when(!self.embedded, |d| {
                d.child(
                    window_titlebar("remote-titlebar", window)
                        .h(px(40.))
                        .flex_shrink_0()
                        .justify_center()
                        .text_size(px(12.))
                        .text_color(t::muted())
                        .border_b_1()
                        .border_color(t::border())
                        .child("Canopy — Remote"),
                )
            })
            .child(
                row()
                    .items_stretch()
                    .flex_1()
                    .min_h_0()
                    .children(navigation)
                    .child(
                        column()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(header)
                            .children(tabs)
                            .children(status)
                            .child(content),
                    ),
            )
            .children(overlays);
        if let Some(host) = &self.host {
            canopy_desktop::remote::metrics::record(
                host.id().as_str(),
                &canopy_desktop::remote::metrics::resource_id("remote_window"),
                None,
                self.generation,
                canopy_desktop::remote::metrics::Stage::Render,
                started,
                canopy_desktop::remote::metrics::Outcome::Ok,
            );
        }
        element.into_any_element()
    }
}
