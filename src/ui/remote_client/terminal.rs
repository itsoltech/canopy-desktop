//! Remote terminal page: the local terminal pane's tabs, surface, grid painter
//! and status bars over host-rendered frames.
use super::*;
use canopy_desktop::{
    remote::client::rpc_error,
    terminal::{
        geometry::{CellSize, GridGeometry},
        session::Size,
    },
};
use gpui_kit::component::{
    IconName,
    menu::{DropdownMenu, PopupMenuItem},
};
use prost::Message;
impl RemoteWindow {
    pub(super) fn open_terminal(&mut self, cx: &mut Context<Self>) {
        let (Some(host), Some(terminal)) = (self.host.clone(), self.terminal.clone()) else {
            return;
        };
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        self.terminal_tx = Some(tx.clone());
        let (send, receive) = async_channel::bounded(1);
        let task = self.global.runtime.spawn(async move {
            let result = async {
                tx.send(p::TerminalClientEvent {
                    event: Some(p::terminal_client_event::Event::Subscribe(
                        p::TerminalSubscribe {
                            terminal: Some(terminal.clone()),
                            since: None,
                        },
                    )),
                })
                .await
                .map_err(|_| "Terminal input closed.")?;
                let mut stream =
                    p::terminal_service_client::TerminalServiceClient::new(host.channel()?)
                        .terminal_session(tokio_stream::wrappers::ReceiverStream::new(rx))
                        .await
                        .map_err(rpc_error)?
                        .into_inner();
                let mut fragment: Option<(String, u64, Vec<u8>, Vec<u8>)> = None;
                let mut cursor: Option<p::Cursor> = None;
                while let Some(mut event) = stream.message().await.map_err(rpc_error)? {
                    if event.terminal.as_ref() != Some(&terminal) {
                        return Err("Terminal identity mismatch.".into());
                    }
                    if let Some(next) = &event.cursor {
                        if let Some(old) = &cursor
                            && (old.runtime_epoch != next.runtime_epoch
                                || old.stream_id != next.stream_id
                                || old.sequence + 1 != next.sequence)
                        {
                            return Err("Terminal requires resynchronization.".into());
                        }
                        cursor = Some(next.clone());
                    }
                    if matches!(
                        event.event,
                        Some(p::terminal_server_event::Event::FrameFragment(_))
                    ) {
                        let Some(p::terminal_server_event::Event::FrameFragment(part)) =
                            event.event.take()
                        else {
                            unreachable!()
                        };
                        use sha2::{Digest, Sha256};
                        if part.total_bytes > 32 * 1024 * 1024 || part.data.len() > 65536 {
                            return Err("Terminal frame exceeds the limit.".into());
                        }
                        let f = fragment.get_or_insert_with(|| {
                            (
                                part.frame_id.clone(),
                                part.total_bytes,
                                part.sha256.clone(),
                                vec![],
                            )
                        });
                        if f.0 != part.frame_id
                            || f.1 != part.total_bytes
                            || f.2 != part.sha256
                            || f.3.len() as u64 != part.offset
                            || part.offset + part.data.len() as u64 > part.total_bytes
                        {
                            return Err("Terminal fragment conflict.".into());
                        }
                        f.3.extend(part.data);
                        if f.3.len() as u64 != f.1 {
                            continue;
                        }
                        let (_, _, hash, bytes) = fragment.take().unwrap();
                        if Sha256::digest(&bytes).as_slice() != hash {
                            return Err("Terminal checksum mismatch.".into());
                        }
                        event.event = Some(p::terminal_server_event::Event::Frame(
                            p::TerminalFrame::decode(bytes.as_slice())
                                .map_err(|_| "Invalid terminal frame.")?,
                        ));
                    }
                    if send.send(Ok(event)).await.is_err() {
                        return Ok(());
                    }
                }
                Err::<(), String>("Terminal stream disconnected; input is no longer owned.".into())
            }
            .await;
            if let Err(error) = result {
                let _ = send.send(Err(error)).await;
            }
        });
        self.stream = Some(task.abort_handle());
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            while let Ok(result) = receive.recv().await {
                if this
                    .update(cx, |this, cx| {
                        if this.generation != generation {
                            return;
                        }
                        match result {
                            Ok(event) => {
                                if this.terminal_run != event.run_id {
                                    if let Some(task) = this.renew.take() {
                                        task.abort();
                                    }
                                    this.terminal_lease = None;
                                    this.terminal_frame = None;
                                    this.terminal_history = None;
                                }
                                this.terminal_run = event.run_id;
                                match event.event {
                                    Some(p::terminal_server_event::Event::Frame(frame)) => {
                                        if frame.complete {
                                            this.terminal_retries = 0;
                                            this.terminal_frame = Some(frame);
                                        } else if let Some(current) = this.terminal_frame.as_mut() {
                                            for cell in frame.cells {
                                                current.cells.retain(|old| {
                                                    old.row != cell.row || old.column != cell.column
                                                });
                                                current.cells.push(cell);
                                            }
                                            current.cursor_column = frame.cursor_column;
                                            current.cursor_row = frame.cursor_row;
                                            current.input_modes = frame.input_modes;
                                        }
                                    }
                                    Some(p::terminal_server_event::Event::Lease(lease)) => {
                                        this.install_lease(lease)
                                    }
                                    Some(p::terminal_server_event::Event::InputAck(ack)) => {
                                        this.info = if ack.queued {
                                            "Terminal input queued".into()
                                        } else {
                                            ack.error
                                                .map(|e| e.message)
                                                .unwrap_or("Input rejected".into())
                                        }
                                    }
                                    Some(p::terminal_server_event::Event::Error(error)) => {
                                        this.error = Some(error.message)
                                    }
                                    Some(p::terminal_server_event::Event::Resync(error)) => {
                                        this.error = Some(error.reason);
                                        this.schedule_terminal_resync(cx);
                                    }
                                    _ => {}
                                }
                            }
                            Err(error) => {
                                let retry = error.contains("resynchronization")
                                    || error.contains("disconnected")
                                    || error.contains("Unavailable");
                                this.error = Some(error);
                                this.terminal_lease = None;
                                if retry {
                                    this.schedule_terminal_resync(cx);
                                }
                            }
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }
    fn schedule_terminal_resync(&mut self, cx: &mut Context<Self>) {
        self.detach();
        self.generation += 1;
        if self.terminal_retries >= 5 {
            self.error = Some("Terminal resynchronization failed. Reconnect to retry.".into());
            return;
        }
        self.terminal_retries += 1;
        let generation = self.generation;
        let delay = u64::from(self.terminal_retries) * 500;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(delay))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation
                    && this.page == Page::Terminal
                    && this
                        .host
                        .as_ref()
                        .is_some_and(|host| host.phase() == Phase::Ready)
                {
                    this.open_terminal(cx);
                }
            });
        })
        .detach();
    }
    fn install_lease(&mut self, lease: p::InputLease) {
        if let Some(task) = self.renew.take() {
            task.abort();
        }
        if lease.terminal != self.terminal || lease.run_id != self.terminal_run {
            return;
        }
        if lease.held
            && let Some(tx) = self.terminal_tx.clone()
        {
            let terminal = self.terminal.clone();
            let renewal = lease.clone();
            let task = self.global.runtime.spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(u64::from(
                        renewal.expires_in_ms.max(300) / 3,
                    )))
                    .await;
                    if tx
                        .send(p::TerminalClientEvent {
                            event: Some(p::terminal_client_event::Event::Renew(p::TerminalRenew {
                                terminal: terminal.clone(),
                                run_id: renewal.run_id.clone(),
                                fencing_token: renewal.fencing_token,
                            })),
                        })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            });
            self.renew = Some(task.abort_handle());
        }
        self.terminal_lease = Some(lease);
        self.terminal_size = (0, 0);
    }
    fn terminal_send(&mut self, data: Vec<u8>, cx: &mut Context<Self>) {
        if self.terminal_history.is_some() || data.len() > 65536 {
            return;
        }
        let Some(lease) = self
            .terminal_lease
            .as_ref()
            .filter(|l| l.held && l.run_id == self.terminal_run)
        else {
            return;
        };
        let Some(tx) = &self.terminal_tx else {
            return;
        };
        self.terminal_input_id += 1;
        if tx
            .try_send(p::TerminalClientEvent {
                event: Some(p::terminal_client_event::Event::Input(p::TerminalInput {
                    terminal: self.terminal.clone(),
                    run_id: lease.run_id.clone(),
                    fencing_token: lease.fencing_token,
                    input_id: self.terminal_input_id,
                    data,
                })),
            })
            .is_err()
        {
            self.error = Some("Terminal input queue is full; bytes were not replayed.".into());
            cx.notify();
        }
    }
    fn terminal_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        use canopy_desktop::terminal::input;
        let key = &event.keystroke;
        let mods = key.modifiers;
        if (mods.platform || (mods.control && mods.shift)) && key.key == "v" {
            if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text())
                && let Some(bytes) = input::paste_bytes(
                    &text,
                    self.terminal_frame
                        .as_ref()
                        .is_some_and(|f| f.input_modes & 4 != 0),
                )
            {
                self.terminal_send(bytes, cx);
            }
            cx.stop_propagation();
            return;
        }
        if (mods.platform || (mods.control && mods.shift)) && key.key == "c" {
            self.copy_terminal(cx);
            cx.stop_propagation();
            return;
        }
        if mods.platform
            || input::preserve_native_text_input(
                &key.key,
                mods.control,
                mods.alt,
                key.key_char.as_deref(),
                event.prefer_character_input,
                cfg!(windows),
            )
        {
            return;
        }
        if let Some(bytes) = input::key(
            &key.key,
            mods.control,
            mods.alt,
            mods.shift,
            input::KeyboardMode {
                app_cursor: self
                    .terminal_frame
                    .as_ref()
                    .is_some_and(|f| f.input_modes & 1 != 0),
            },
        ) {
            self.terminal_send(bytes, cx);
            cx.stop_propagation();
        }
    }
    fn copy_terminal(&self, cx: &mut Context<Self>) {
        let frame = self
            .terminal_history
            .as_ref()
            .or(self.terminal_frame.as_ref());
        let Some(frame) = frame else {
            return;
        };
        let Some((a, b)) = self.terminal_selection else {
            return;
        };
        let (start, end) = if a <= b { (a, b) } else { (b, a) };
        let mut text = String::new();
        for row in start.0..=end.0 {
            let mut cells = frame
                .cells
                .iter()
                .filter(|c| {
                    c.row == row
                        && c.column_span > 0
                        && (row != start.0 || c.column >= start.1)
                        && (row != end.0 || c.column <= end.1)
                })
                .collect::<Vec<_>>();
            cells.sort_by_key(|c| c.column);
            for cell in cells {
                text.push_str(&cell.text);
            }
            if row < end.0 {
                text.push('\n');
            }
        }
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }
    fn terminal_mouse(
        &mut self,
        position: Point<Pixels>,
        code: u8,
        release: bool,
        modifiers: Modifiers,
        cx: &mut Context<Self>,
    ) -> bool {
        if modifiers.shift
            || self.terminal_history.is_some()
            || !self.terminal_lease.as_ref().is_some_and(|lease| lease.held)
        {
            return false;
        }
        let modes = self
            .terminal_frame
            .as_ref()
            .map(|f| f.input_modes)
            .unwrap_or_default();
        if modes & (128 | 256 | 512) == 0 {
            return false;
        }
        let (row, column) = self.terminal_cell(position);
        let code =
            code + if modifiers.alt { 8 } else { 0 } + if modifiers.control { 16 } else { 0 };
        let bytes = if modes & 1024 != 0 {
            format!(
                "\x1b[<{};{};{}{}",
                code,
                column + 1,
                row + 1,
                if release { 'm' } else { 'M' }
            )
            .into_bytes()
        } else {
            let code = if release { 3 } else { code };
            if modes & 2048 != 0 {
                let mut text = String::from("\x1b[M");
                for value in [u32::from(code) + 32, column + 33, row + 33] {
                    if let Some(value) = char::from_u32(value) {
                        text.push(value);
                    } else {
                        return true;
                    }
                }
                text.into_bytes()
            } else {
                if column > 222 || row > 222 {
                    return true;
                }
                vec![27, b'[', b'M', code + 32, column as u8 + 33, row as u8 + 33]
            }
        };
        self.terminal_send(bytes, cx);
        true
    }
    fn terminal_cell(&self, position: Point<Pixels>) -> (u32, u32) {
        let x = f32::from(position.x - self.terminal_bounds.origin.x);
        let y = f32::from(position.y - self.terminal_bounds.origin.y);
        let (row, column) = self.terminal_geometry().cell_at(x, y);
        (row as u32, column as u32)
    }
    fn read_terminal_history(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let terminal = self.terminal.clone();
        let run_id = self.terminal_run.clone();
        self.terminal_history_offset += 50;
        let offset = self.terminal_history_offset;
        self.run(
            async move {
                let mut stream =
                    p::terminal_service_client::TerminalServiceClient::new(host.channel()?)
                        .read_history(p::TerminalHistoryRequest {
                            terminal: terminal.clone(),
                            run_id: run_id.clone(),
                            offset_from_bottom: offset,
                            rows: 50,
                            page_token: None,
                        })
                        .await
                        .map_err(rpc_error)?
                        .into_inner();
                let mut boundary = None;
                let mut frame = None;
                let mut fragment: Option<(String, u64, Vec<u8>, Vec<u8>)> = None;
                let mut complete = false;
                while let Some(event) = stream.message().await.map_err(rpc_error)? {
                    if event.terminal != terminal || event.run_id != run_id || complete {
                        return Err("Terminal history identity mismatch.".into());
                    }
                    match event.part {
                        Some(p::terminal_history_event::Part::Start(start))
                            if boundary.is_none() =>
                        {
                            boundary = start.boundary
                        }
                        Some(p::terminal_history_event::Part::Frame(value))
                            if boundary.is_some() && frame.is_none() =>
                        {
                            frame = Some(value)
                        }
                        Some(p::terminal_history_event::Part::FrameFragment(part))
                            if boundary.is_some() =>
                        {
                            if let Some(value) = assemble_frame(&mut fragment, part)? {
                                frame = Some(value);
                            }
                        }
                        Some(p::terminal_history_event::Part::End(end))
                            if boundary.as_ref() == Some(&end)
                                && frame.is_some()
                                && fragment.is_none() =>
                        {
                            complete = true
                        }
                        _ => return Err("Terminal history boundary mismatch.".into()),
                    }
                }
                if !complete {
                    return Err("Terminal history is incomplete.".into());
                }
                Ok(Update::TerminalHistory(frame.unwrap()))
            },
            cx,
        );
    }
    fn take_terminal_input(&mut self, cx: &mut Context<Self>) {
        let (Some(host), Some(terminal)) = (self.host.clone(), self.terminal.clone()) else {
            return;
        };
        let generation = self.generation;
        let release_host = host.clone();
        let release_terminal = terminal.clone();
        let runtime = self.global.runtime.clone();
        let task = self
            .global
            .runtime
            .spawn(async move { host.acquire_input(terminal, true).await });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let acquired = result
                .as_ref()
                .ok()
                .and_then(|result| result.as_ref().ok())
                .cloned();
            let installed = this
                .update(cx, |this, cx| {
                    if this.generation != generation {
                        return false;
                    }
                    match result {
                        Ok(Ok(lease)) => this.install_lease(lease),
                        Ok(Err(error)) => this.error = Some(error),
                        Err(_) => this.error = Some("Couldn’t take control of input.".into()),
                    }
                    cx.notify();
                    true
                })
                .unwrap_or(false);
            if !installed && let Some(lease) = acquired {
                runtime.spawn(
                    async move { release_host.release_input(release_terminal, lease).await },
                );
            }
        })
        .detach();
    }
    fn release_terminal_input(&mut self, cx: &mut Context<Self>) {
        if let Some(task) = self.renew.take() {
            task.abort();
        }
        if let (Some(host), Some(terminal), Some(lease)) = (
            self.host.clone(),
            self.terminal.clone(),
            self.terminal_lease.take(),
        ) {
            self.run(
                async move {
                    host.release_input(terminal, lease).await?;
                    Ok(Update::Done("Input control released.".into()))
                },
                cx,
            );
        }
    }
    fn switch_terminal(&mut self, terminal: p::TerminalRef, cx: &mut Context<Self>) {
        if self.terminal.as_ref() == Some(&terminal) {
            return;
        }
        self.navigate(Page::Terminal, cx);
        self.terminal = Some(terminal);
        self.terminal_frame = None;
        self.terminal_history = None;
        self.terminal_history_offset = 0;
        self.terminal_run.clear();
        self.terminal_selection = None;
        self.save_layout(cx);
        if self.ready() {
            self.open_terminal(cx);
        }
    }
    /// Terminals of the current workspace, shown like the local terminal tabs.
    fn render_terminal_tabs(
        &self,
        summary: Option<&p::TerminalSummary>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let terminals: Vec<p::TerminalSummary> = self
            .host
            .as_ref()
            .and_then(|host| host.shared_catalog())
            .map(|catalog| {
                catalog
                    .terminals
                    .iter()
                    .filter(|t| t.workspace == self.workspace || t.terminal == self.terminal)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let held = self.terminal_lease.as_ref().is_some_and(|l| l.held);
        let history = self.terminal_history.is_some();
        let owner = cx.entity().downgrade();
        let running = summary.is_none_or(|s| s.running);
        let ready = self.ready();
        let mut strip = tab_strip("remote-terminal-tabs");
        for summary in terminals {
            let Some(terminal) = summary.terminal.clone() else {
                continue;
            };
            let active = self.terminal.as_ref() == Some(&terminal);
            strip = strip.child(
                workspace_tab(
                    SharedString::from(format!("remote-terminal-tab:{}", terminal.terminal_id)),
                    if summary.title.is_empty() {
                        "Terminal".to_owned()
                    } else {
                        summary.title.clone()
                    },
                    active,
                    160.,
                    (!summary.running).then_some(("Exited", t::muted())),
                )
                .on_click(
                    cx.listener(move |this, _, _, cx| this.switch_terminal(terminal.clone(), cx)),
                ),
            );
        }
        let creating = self.pending == Some("new-terminal");
        strip = strip.child(
            div()
                .flex_shrink_0()
                .h_full()
                .flex()
                .items_center()
                .px_1()
                .child(
                    icon_button("terminal-new", IconName::Plus, "New terminal")
                        .loading(creating)
                        .disabled(!ready || self.workspace.is_none() || creating)
                        .on_click(cx.listener(|this, _, _, cx| this.new_shell_terminal(cx))),
                ),
        );
        strip.child(div().flex_1()).child(
            row()
                .flex_shrink_0()
                .h_full()
                .px_2()
                .gap_1()
                .child(
                    icon_button(
                        "terminal-history",
                        IconName::ArrowUp,
                        if history {
                            "Show earlier output"
                        } else {
                            "Earlier output"
                        },
                    )
                    .disabled(!ready)
                    .on_click(cx.listener(|this, _, _, cx| this.read_terminal_history(cx))),
                )
                .child(
                    icon_button("terminal-copy", IconName::Copy, "Copy selection")
                        .disabled(self.terminal_selection.is_none())
                        .on_click(cx.listener(|this, _, _, cx| this.copy_terminal(cx))),
                )
                .child(
                    icon_button("terminal-menu", IconName::Ellipsis, "Terminal actions")
                        .dropdown_menu(move |menu, _, _| {
                            let release = owner.clone();
                            let restart = owner.clone();
                            let stop = owner.clone();
                            let close = owner.clone();
                            menu.item(
                                PopupMenuItem::new("Release control")
                                    .disabled(!held)
                                    .on_click(move |_, _, cx| {
                                        let _ = release
                                            .update(cx, |this, cx| this.release_terminal_input(cx));
                                    }),
                            )
                            .separator()
                            .item(
                                PopupMenuItem::new("Restart terminal…")
                                    .disabled(!ready)
                                    .on_click(move |_, _, cx| {
                                        let _ = restart.update(cx, |this, cx| {
                                            this.terminal_lifecycle(true, cx)
                                        });
                                    }),
                            )
                            .item(
                                PopupMenuItem::new("Stop terminal…")
                                    .disabled(!ready || !running)
                                    .on_click(move |_, _, cx| {
                                        let _ = stop.update(cx, |this, cx| {
                                            this.terminal_lifecycle(false, cx)
                                        });
                                    }),
                            )
                            .separator()
                            .item(
                                PopupMenuItem::new("Close view").on_click(move |_, _, cx| {
                                    let _ = close
                                        .update(cx, |this, cx| this.navigate(Page::Catalog, cx));
                                }),
                            )
                        }),
                ),
        )
    }
    pub(super) fn render_terminal(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity();
        let surface = canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                entity.update(cx, |this, cx| this.paint_terminal(bounds, window, cx));
            },
        )
        .size_full();
        let summary = self
            .host
            .as_ref()
            .and_then(|host| host.shared_catalog())
            .and_then(|catalog| {
                catalog
                    .terminals
                    .iter()
                    .find(|t| t.terminal == self.terminal)
                    .cloned()
            });
        let held = self.terminal_lease.as_ref().is_some_and(|l| l.held);
        let running = summary.as_ref().is_none_or(|s| s.running);
        let ready = self.ready();
        let grid = div()
            .id("remote-terminal-grid")
            .size_full()
            .track_focus(&self.terminal_focus)
            .on_key_down(cx.listener(|this, event, _, cx| this.terminal_key(event, cx)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.terminal_focus.focus(window, cx);
                    if this.terminal_mouse(event.position, 0, false, event.modifiers, cx) {
                        return;
                    }
                    let cell = this.terminal_cell(event.position);
                    this.terminal_selection = Some((cell, cell));
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.terminal_mouse(event.position, 2, false, event.modifiers, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.terminal_mouse(event.position, 1, false, event.modifiers, cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, _, cx| {
                    this.terminal_mouse(event.position, 0, true, event.modifiers, cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Right,
                cx.listener(|this, event: &MouseUpEvent, _, cx| {
                    this.terminal_mouse(event.position, 2, true, event.modifiers, cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(|this, event: &MouseUpEvent, _, cx| {
                    this.terminal_mouse(event.position, 1, true, event.modifiers, cx);
                }),
            )
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                let delta = f32::from(event.delta.pixel_delta(px(this.terminal_cell_size.1)).y);
                let count = (delta.abs() / this.terminal_cell_size.1.max(1.))
                    .ceil()
                    .clamp(1., 8.) as usize;
                for _ in 0..count {
                    if !this.terminal_mouse(
                        event.position,
                        if delta > 0. { 64 } else { 65 },
                        false,
                        event.modifiers,
                        cx,
                    ) {
                        break;
                    }
                }
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                let modes = this
                    .terminal_frame
                    .as_ref()
                    .map(|f| f.input_modes)
                    .unwrap_or_default();
                if modes & 512 != 0 || modes & 256 != 0 && event.pressed_button.is_some() {
                    let code = match event.pressed_button {
                        Some(MouseButton::Left) => 32,
                        Some(MouseButton::Middle) => 33,
                        Some(MouseButton::Right) => 34,
                        _ => 35,
                    };
                    if this.terminal_mouse(event.position, code, false, event.modifiers, cx) {
                        return;
                    }
                }
                if event.pressed_button == Some(MouseButton::Left) {
                    let cell = this.terminal_cell(event.position);
                    if let Some((start, _)) = this.terminal_selection {
                        this.terminal_selection = Some((start, cell));
                        cx.notify();
                    }
                }
            }))
            .child(surface);
        let waiting = self.terminal_frame.is_none() && self.terminal_history.is_none();
        terminal_surface("remote-terminal")
            .flex_1()
            .min_h_96()
            .child(self.render_terminal_tabs(summary.as_ref(), cx))
            .child(terminal_viewport(grid).children(waiting.then(|| {
                terminal_message(if ready { "Connecting…" } else { "Offline" })
                    .absolute()
                    .left(px(12.))
                    .top(px(12.))
            })))
            .children(self.terminal_history.is_some().then(|| {
                process_status_bar(
                    "Showing earlier output. Typing is paused.",
                    [button("terminal-live", "Back to live output")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.terminal_history = None;
                            this.terminal_history_offset = 0;
                            cx.notify();
                        }))
                        .into_any_element()],
                )
            }))
            .children(
                (self.terminal_history.is_none() && running && !held).then(|| {
                    process_status_bar(
                        if summary
                            .as_ref()
                            .is_some_and(|s| s.input_device_id.is_some())
                        {
                            "Input is controlled by another device. Typing here is paused."
                        } else {
                            "Read only. Take control to type in this terminal."
                        },
                        [button("terminal-input-control", "Take control")
                            .disabled(!ready)
                            .on_click(cx.listener(|this, _, _, cx| this.take_terminal_input(cx)))
                            .into_any_element()],
                    )
                }),
            )
            .children((!running).then(|| {
                process_status_bar(
                    "Process exited",
                    [button("terminal-restart", "Restart")
                        .disabled(!ready)
                        .on_click(cx.listener(|this, _, _, cx| this.terminal_lifecycle(true, cx)))
                        .into_any_element()],
                )
            }))
            .into_any_element()
    }
    fn terminal_lifecycle(&mut self, restart: bool, cx: &mut Context<Self>) {
        let host = self.host.clone().unwrap();
        let terminal = self.terminal.clone();
        let run_id = self.terminal_run.clone();
        let title = if restart {
            "Restart terminal?"
        } else {
            "Stop terminal?"
        };
        self.apply(Update::Confirm(title.into(), "This affects the host process and every connected device. Unsaved terminal work may be lost.".into(), Box::pin(async move {
            host.mutate(None, move |channel, command| async move {
                let mut client = p::terminal_service_client::TerminalServiceClient::new(channel);
                let request = p::TerminalCommandRequest { command: Some(command), terminal, run_id };
                if restart { client.restart_terminal(request).await } else { client.stop_terminal(request).await }
            }).await?;
            Ok(Update::Done("Terminal operation completed.".into()))
        })), cx);
    }
    /// Grid placement shared by painting and pointer mapping. The host frame's
    /// own size is centered exactly like the local terminal grid.
    fn terminal_geometry(&self) -> GridGeometry {
        let (width, height) = (
            f32::from(self.terminal_bounds.size.width),
            f32::from(self.terminal_bounds.size.height),
        );
        match self
            .terminal_history
            .as_ref()
            .or(self.terminal_frame.as_ref())
        {
            Some(frame) => GridGeometry::with_size(
                width,
                height,
                Size::bounded(frame.columns as usize, frame.rows as usize),
                self.grid_cell(),
            ),
            None => GridGeometry::fit(width, height, self.grid_cell()),
        }
    }
    fn grid_cell(&self) -> CellSize {
        CellSize {
            width: self.terminal_cell_size.0,
            height: self.terminal_cell_size.1,
        }
    }
    fn paint_terminal(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.terminal_bounds = bounds;
        window.handle_input(
            &self.terminal_focus,
            ElementInputHandler::new(bounds, cx.entity()),
            cx,
        );
        // The device's own terminal font sizes the grid it asks the host for.
        let terminal_font = t::terminal_font(cx);
        let (base_font, font_size, grid) =
            (terminal_font.font, terminal_font.size, terminal_font.cell);
        self.terminal_cell_size = (grid.width, grid.height);
        let fit = GridGeometry::fit(
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
            grid,
        );
        let dimensions = (fit.size.columns as u32, fit.size.rows as u32);
        if self.terminal_size != dimensions && self.terminal_history.is_none() {
            self.terminal_size = dimensions;
            let owner = cx.entity().downgrade();
            let generation = self.generation;
            cx.defer(move |cx| {
                let _ = owner.update(cx, |this, _| {
                    if this.generation != generation || this.terminal_history.is_some() {
                        return;
                    }
                    if let (Some(lease), Some(tx)) = (&this.terminal_lease, &this.terminal_tx)
                        && lease.held
                        && lease.run_id == this.terminal_run
                    {
                        let _ = tx.try_send(p::TerminalClientEvent {
                            event: Some(p::terminal_client_event::Event::Resize(
                                p::TerminalResize {
                                    terminal: this.terminal.clone(),
                                    run_id: lease.run_id.clone(),
                                    fencing_token: lease.fencing_token,
                                    columns: dimensions.0.clamp(20, 500),
                                    rows: dimensions.1.clamp(5, 300),
                                },
                            )),
                        });
                    }
                });
            });
        }
        let geometry = self.terminal_geometry();
        let Some(frame) = self
            .terminal_history
            .as_ref()
            .or(self.terminal_frame.as_ref())
        else {
            return;
        };
        let origin = bounds.origin + point(px(geometry.left), px(geometry.top));
        let selection = self
            .terminal_selection
            .map(|(a, b)| if a <= b { (a, b) } else { (b, a) });
        let selected = |cell: &p::TerminalCell| {
            selection
                .is_some_and(|(s, e)| (cell.row, cell.column) >= s && (cell.row, cell.column) <= e)
        };
        // Incremental frames append replaced cells; paint in grid order.
        let mut cells: Vec<&p::TerminalCell> = frame.cells.iter().collect();
        cells.sort_by_key(|c| (c.row, c.column));
        let focused = self.terminal_focus.is_focused(window);
        window.paint_layer(bounds, |window| {
            let mut i = 0;
            while i < cells.len() {
                let first = cells[i];
                let mut end = i + 1;
                let mut columns = first.column_span.max(1);
                while let Some(cell) = cells.get(end) {
                    if cell.row != first.row
                        || cell.background_rgba != first.background_rgba
                        || cell.column != first.column + columns
                    {
                        break;
                    }
                    columns += cell.column_span.max(1);
                    end += 1;
                }
                let rect = geometry.background(
                    first.row as usize,
                    first.column as usize,
                    columns as usize,
                );
                window.paint_quad(fill(
                    Bounds::new(
                        bounds.origin + point(px(rect.x), px(rect.y)),
                        size(px(rect.width), px(rect.height)),
                    ),
                    Hsla::from(rgba(first.background_rgba)),
                ));
                i = end;
            }
            let mut i = 0;
            while i < cells.len() {
                let cell = cells[i];
                if cell.column_span == 0 {
                    i += 1;
                    continue;
                }
                let mut text = cell.text.clone();
                let mut byte_columns = vec![0; cell.text.len()];
                let mut columns = cell.column_span as usize;
                let mut next = i + 1;
                let is_selected = selected(cell);
                if cell.column_span == 1 {
                    while let Some(other) = cells.get(next) {
                        if other.row != cell.row
                            || other.column as usize != cell.column as usize + columns
                            || other.column_span != 1
                            || other.foreground_rgba != cell.foreground_rgba
                            || other.background_rgba != cell.background_rgba
                            || other.flags != cell.flags
                            || selected(other) != is_selected
                        {
                            break;
                        }
                        byte_columns.extend(std::iter::repeat_n(columns, other.text.len()));
                        text.push_str(&other.text);
                        columns += 1;
                        next += 1;
                    }
                }
                let position = origin
                    + point(
                        px(cell.column as f32 * grid.width),
                        px(cell.row as f32 * grid.height),
                    );
                if is_selected {
                    window.paint_quad(fill(
                        Bounds::new(
                            position,
                            size(px(columns as f32 * grid.width), px(grid.height)),
                        ),
                        t::selected(),
                    ));
                }
                if cell.flags & 512 == 0 && !text.is_empty() {
                    let mut font = base_font.clone();
                    if cell.flags & 1 != 0 {
                        font.weight = FontWeight::BOLD;
                    }
                    if cell.flags & 2 != 0 {
                        font.style = FontStyle::Italic;
                    }
                    let run = TextRun {
                        len: text.len(),
                        font,
                        color: rgba(cell.foreground_rgba).into(),
                        underline: (cell.flags & 4 != 0).then_some(UnderlineStyle {
                            thickness: px(1.),
                            color: None,
                            wavy: cell.flags & 4096 != 0,
                        }),
                        strikethrough: (cell.flags & 32 != 0).then_some(StrikethroughStyle {
                            thickness: px(1.),
                            color: None,
                        }),
                        ..Default::default()
                    };
                    let mut line =
                        window
                            .text_system()
                            .shape_line(text.into(), font_size, &[run], None);
                    align_to_grid(&mut line, &byte_columns, columns, grid.width);
                    let _ =
                        line.paint(position, px(grid.height), TextAlign::Left, None, window, cx);
                }
                i = next;
            }
            if frame.input_modes & 16 != 0
                && self.terminal_history.is_none()
                && frame.cursor_row < frame.rows.max(1)
                && frame.cursor_column < frame.columns.max(1)
            {
                let mut color = t::text();
                color.a = if focused { 0.45 } else { 0.18 };
                window.paint_quad(fill(
                    Bounds::new(
                        origin
                            + point(
                                px(frame.cursor_column as f32 * grid.width),
                                px(frame.cursor_row as f32 * grid.height),
                            ),
                        size(px(grid.width), px(grid.height)),
                    ),
                    color,
                ));
            }
            if !self.terminal_preedit.is_empty() {
                let run = TextRun {
                    len: self.terminal_preedit.len(),
                    font: base_font.clone(),
                    color: t::yellow(),
                    ..Default::default()
                };
                let line = window.text_system().shape_line(
                    self.terminal_preedit.clone().into(),
                    font_size,
                    &[run],
                    None,
                );
                let _ = line.paint(
                    origin
                        + point(
                            px(frame.cursor_column as f32 * grid.width),
                            px(frame.cursor_row as f32 * grid.height),
                        ),
                    px(grid.height),
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
        });
    }
}
/// Same grid alignment as the local terminal painter: native shaping inside a
/// cell, fixed advances between cells, without mutating GPUI's shared cache.
fn align_to_grid(line: &mut ShapedLine, byte_columns: &[usize], columns: usize, cell_width: f32) {
    let mut runs = line.runs.clone();
    let glyphs: Vec<_> = runs
        .iter()
        .flat_map(|r| r.glyphs.iter())
        .map(|g| (g.index, f32::from(g.position.x)))
        .collect();
    let positions =
        canopy_desktop::terminal::glyph_layout::positions(&glyphs, byte_columns, cell_width);
    for (glyph, x) in runs
        .iter_mut()
        .flat_map(|r| r.glyphs.iter_mut())
        .zip(positions)
    {
        glyph.position.x = px(x);
    }
    **line = std::sync::Arc::new(LineLayout {
        font_size: line.font_size,
        width: px(columns as f32 * cell_width),
        ascent: line.ascent,
        descent: line.descent,
        runs,
        len: line.len(),
    });
}
fn assemble_frame(
    fragment: &mut Option<(String, u64, Vec<u8>, Vec<u8>)>,
    part: p::TerminalFrameFragment,
) -> Result<Option<p::TerminalFrame>, String> {
    use sha2::{Digest, Sha256};
    if part.total_bytes > 32 * 1024 * 1024 || part.total_bytes == 0 || part.data.len() > 65536 {
        return Err("Terminal frame exceeds its limit.".into());
    }
    let f = fragment.get_or_insert_with(|| {
        (
            part.frame_id.clone(),
            part.total_bytes,
            part.sha256.clone(),
            vec![],
        )
    });
    if f.0 != part.frame_id
        || f.1 != part.total_bytes
        || f.2 != part.sha256
        || f.3.len() as u64 != part.offset
        || part.offset + part.data.len() as u64 > part.total_bytes
    {
        return Err("Terminal fragment conflict.".into());
    }
    f.3.extend(part.data);
    if f.3.len() as u64 != f.1 {
        return Ok(None);
    }
    let (_, _, hash, bytes) = fragment.take().unwrap();
    if Sha256::digest(&bytes).as_slice() != hash {
        return Err("Terminal checksum mismatch.".into());
    }
    Ok(Some(
        p::TerminalFrame::decode(bytes.as_slice()).map_err(|_| "Invalid terminal frame.")?,
    ))
}
impl EntityInputHandler for RemoteWindow {
    fn text_for_range(
        &mut self,
        range: std::ops::Range<usize>,
        adjusted: &mut Option<std::ops::Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let chars = self.terminal_preedit.encode_utf16().collect::<Vec<_>>();
        let end = range.end.min(chars.len());
        let start = range.start.min(end);
        *adjusted = Some(start..end);
        Some(String::from_utf16_lossy(&chars[start..end]))
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let end = self.terminal_preedit.encode_utf16().count();
        Some(UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }
    fn marked_text_range(
        &self,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<std::ops::Range<usize>> {
        (!self.terminal_preedit.is_empty()).then(|| 0..self.terminal_preedit.encode_utf16().count())
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.terminal_preedit.clear();
        cx.notify();
    }
    fn replace_text_in_range(
        &mut self,
        _: Option<std::ops::Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.terminal_preedit.clear();
        self.terminal_send(text.as_bytes().to_vec(), cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<std::ops::Range<usize>>,
        text: &str,
        _: Option<std::ops::Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.terminal_preedit = text.into();
        cx.notify();
    }
    fn bounds_for_range(
        &mut self,
        _: std::ops::Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(bounds)
    }
    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(0)
    }
    fn accepts_text_input(&self, _: &mut Window, _: &mut Context<Self>) -> bool {
        self.page == Page::Terminal
            && self.terminal_history.is_none()
            && self.terminal_lease.as_ref().is_some_and(|l| l.held)
    }
}
