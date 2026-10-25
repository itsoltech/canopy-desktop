//! Access to the live page item stays with the app's chat owner. Only typed
//! retained page content crosses this bounded bridge, never an executable URL.
use crate::app_state::AppState;
use canopy_desktop::{
    chat::{
        page::Page,
        transcript::{Body, ItemId},
    },
    remote::discovery::PageSource,
};
use futures_channel::oneshot;
use gpui_kit::*;
use tonic::Status;
type PageRequest = (
    String,
    u64,
    u64,
    oneshot::Sender<Result<Option<Page>, Status>>,
);

#[derive(Clone)]
pub struct Pages {
    sender: async_channel::Sender<PageRequest>,
}
impl Pages {
    pub fn install(cx: &mut App) -> Self {
        let (sender, receiver) = async_channel::bounded::<PageRequest>(32);
        cx.spawn(async move |cx| {
            while let Ok((chat, epoch, item, reply)) = receiver.recv().await {
                let page = cx.update(|cx| {
                    let registry = cx.global::<AppState>().chats.read(cx);
                    let runtime = registry.runtime_for_chat(&chat)?.read(cx);
                    if runtime.epoch() != epoch {
                        return None;
                    }
                    let item = runtime
                        .transcript()?
                        .items()
                        .iter()
                        .find(|i| i.id == ItemId(item))?;
                    if item.trimmed {
                        return None;
                    }
                    let Body::Tool(tool) = &item.body else {
                        return None;
                    };
                    (tool.name == canopy_desktop::chat::page::SHOW_TOOL)
                        .then(|| Page::from_input(&tool.input).ok())
                        .flatten()
                });
                let _ = reply.send(Ok(page));
            }
        })
        .detach();
        Self { sender }
    }
}
#[tonic::async_trait]
impl PageSource for Pages {
    async fn page(&self, chat: String, epoch: u64, item: u64) -> Result<Option<Page>, Status> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .try_send((chat, epoch, item, tx))
            .map_err(|_| Status::resource_exhausted("Live page reads are busy."))?;
        rx.await
            .map_err(|_| Status::unavailable("The page owner stopped."))?
    }
}
