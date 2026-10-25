use super::*;
use canopy_desktop::remote::client::rpc_error;
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
        let x = f32::from(position.x - self.terminal_bounds.origin.x).max(0.);
        let y = f32::from(position.y - self.terminal_bounds.origin.y).max(0.);
        (
            (y / self.terminal_cell_size.1.max(1.)) as u32,
            (x / self.terminal_cell_size.0.max(1.)) as u32,
        )
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
    pub(super) fn render_terminal(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity();
        let surface = canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                entity.update(cx, |this, cx| this.paint_terminal(bounds, window, cx));
            },
        )
        .w_full()
        .h_96();
        let mut body = column().gap_2().child(
            row()
                .gap_2()
                .child(
                    button("terminal-input-control", "Take input control")
                        .disabled(!self.ready())
                        .on_click(cx.listener(|this, _, _, cx| {
                            let host = this.host.clone().unwrap();
                            let terminal = this.terminal.clone();
                            let generation = this.generation;
                            let release_host = host.clone();
                            let release_terminal = terminal.clone();
                            let runtime = this.global.runtime.clone();
                            let task = this.global.runtime.spawn(async move {
                                host.acquire_input(terminal.ok_or("Missing terminal.")?, true)
                                    .await
                            });
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
                                            Err(_) => {
                                                this.error = Some("Lease worker stopped.".into())
                                            }
                                        }
                                        cx.notify();
                                        true
                                    })
                                    .unwrap_or(false);
                                if !installed
                                    && let (Some(terminal), Some(lease)) =
                                        (release_terminal, acquired)
                                {
                                    runtime.spawn(async move {
                                        release_host.release_input(terminal, lease).await
                                    });
                                }
                            })
                            .detach();
                        })),
                )
                .child(
                    button("terminal-history", "Earlier output")
                        .disabled(!self.ready())
                        .on_click(cx.listener(|this, _, _, cx| this.read_terminal_history(cx))),
                )
                .child(button("terminal-live", "Live output").on_click(cx.listener(
                    |this, _, _, cx| {
                        this.terminal_history = None;
                        this.terminal_history_offset = 0;
                        cx.notify();
                    },
                )))
                .child(
                    button("terminal-copy", "Copy selection")
                        .on_click(cx.listener(|this, _, _, cx| this.copy_terminal(cx))),
                ),
        );
        body = body.child(
            row()
                .gap_2()
                .child(
                    button("terminal-release", "Release input control")
                        .disabled(!self.terminal_lease.as_ref().is_some_and(|l| l.held))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(task) = this.renew.take() {
                                task.abort();
                            }
                            if let (Some(host), Some(terminal), Some(lease)) = (
                                this.host.clone(),
                                this.terminal.clone(),
                                this.terminal_lease.take(),
                            ) {
                                this.run(
                                    async move {
                                        host.release_input(terminal, lease).await?;
                                        Ok(Update::Done("Input control released.".into()))
                                    },
                                    cx,
                                );
                            }
                        })),
                )
                .child(
                    button("terminal-stop", "Stop terminal…")
                        .disabled(!self.ready())
                        .on_click(cx.listener(|this, _, _, cx| this.terminal_lifecycle(false, cx))),
                )
                .child(
                    button("terminal-restart", "Restart terminal…")
                        .disabled(!self.ready())
                        .on_click(cx.listener(|this, _, _, cx| this.terminal_lifecycle(true, cx))),
                )
                .child(
                    button("terminal-detach", "Close view")
                        .on_click(cx.listener(|this, _, _, cx| this.navigate(Page::Catalog, cx))),
                ),
        );
        body = body.child(
            div()
                .id("remote-terminal-grid")
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
                .child(surface),
        );
        body.child(if self.terminal_history.is_some() {
            "Earlier output · read only"
        } else if self.terminal_lease.as_ref().is_some_and(|l| l.held) {
            "You have input control"
        } else {
            "Read only · take input control to type"
        })
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
        let scale = f32::from(window.rem_size()) / 16.;
        let cw = canopy_desktop::terminal::geometry::CELL_WIDTH * scale;
        let ch = canopy_desktop::terminal::geometry::CELL_HEIGHT * scale;
        self.terminal_cell_size = (cw, ch);
        let dimensions = (
            (f32::from(bounds.size.width) / cw) as u32,
            (f32::from(bounds.size.height) / ch) as u32,
        );
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
        let Some(frame) = self
            .terminal_history
            .as_ref()
            .or(self.terminal_frame.as_ref())
        else {
            return;
        };
        window.paint_layer(bounds, |window| {
            for cell in &frame.cells {
                let position =
                    bounds.origin + point(px(cell.column as f32 * cw), px(cell.row as f32 * ch));
                let foreground = rgba(cell.foreground_rgba);
                let background = rgba(cell.background_rgba);
                let selected = self.terminal_selection.is_some_and(|(a, b)| {
                    let (s, e) = if a <= b { (a, b) } else { (b, a) };
                    (cell.row, cell.column) >= s && (cell.row, cell.column) <= e
                });
                window.paint_quad(fill(
                    Bounds::new(
                        position,
                        size(px(cw * cell.column_span.max(1) as f32), px(ch)),
                    ),
                    if selected {
                        t::selected()
                    } else {
                        background.into()
                    },
                ));
                if cell.column_span == 0 || cell.flags & 512 != 0 {
                    continue;
                }
                let mut font = t::terminal_font();
                if cell.flags & 1 != 0 {
                    font.weight = FontWeight::BOLD;
                }
                if cell.flags & 2 != 0 {
                    font.style = FontStyle::Italic;
                }
                let run = TextRun {
                    len: cell.text.len(),
                    font,
                    color: foreground.into(),
                    underline: (cell.flags & 4 != 0).then_some(UnderlineStyle {
                        thickness: px(scale),
                        color: None,
                        wavy: cell.flags & 4096 != 0,
                    }),
                    strikethrough: (cell.flags & 32 != 0).then_some(StrikethroughStyle {
                        thickness: px(scale),
                        color: None,
                    }),
                    ..Default::default()
                };
                let line = window.text_system().shape_line(
                    cell.text.clone().into(),
                    px(13. * scale),
                    &[run],
                    None,
                );
                let _ = line.paint(position, px(ch), TextAlign::Left, None, window, cx);
            }
            if frame.input_modes & 16 != 0 && self.terminal_history.is_none() {
                let mut color = t::text();
                color.a = 0.4;
                window.paint_quad(fill(
                    Bounds::new(
                        bounds.origin
                            + point(
                                px(frame.cursor_column as f32 * cw),
                                px(frame.cursor_row as f32 * ch),
                            ),
                        size(px(cw), px(ch)),
                    ),
                    color,
                ));
            }
            if !self.terminal_preedit.is_empty() {
                let run = TextRun {
                    len: self.terminal_preedit.len(),
                    font: t::terminal_font(),
                    color: t::yellow(),
                    ..Default::default()
                };
                let line = window.text_system().shape_line(
                    self.terminal_preedit.clone().into(),
                    px(13. * scale),
                    &[run],
                    None,
                );
                let _ = line.paint(
                    bounds.origin
                        + point(
                            px(frame.cursor_column as f32 * cw),
                            px(frame.cursor_row as f32 * ch),
                        ),
                    px(ch),
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
        });
    }
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
