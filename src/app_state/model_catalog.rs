//! The model picker's lists and hearts (DEC-097…099), shared by every chat: kept in
//! SQLite so a picker shows them at once, and listed again in the background by each
//! account's CLI every time a picker opens; a list that changed replaces the old one.
use super::AppState;
use canopy_desktop::chat::{
    config::{ChatSettings, ModelInfo},
    launch::{prepare_claude, prepare_codex},
    models::{self, Catalog, Favorite},
    provider::Provider,
};
use canopy_desktop::settings::SettingsClient;
use canopy_desktop::state::workspace::Pane;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct ModelCatalog {
    client: Option<SettingsClient>,
    catalog: Catalog,
    /// The saved catalog was read; saving before that would lose its hearts.
    loaded: bool,
    unsaved: bool,
    /// Accounts (profile ids) whose CLI is listing its models now.
    fetching: HashSet<String>,
    /// Why an account's last listing failed.
    errors: HashMap<String, String>,
    /// The latest write; the next one waits for it, so the newest catalog wins.
    save: Option<Task<()>>,
}

impl ModelCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn initialize(&mut self, client: SettingsClient, cx: &mut Context<Self>) {
        self.client = Some(client.clone());
        cx.spawn(async move |this, cx| {
            let saved = match client.load_model_catalog().await {
                Ok(saved) => saved
                    .map(|json| Catalog::from_json(&json))
                    .unwrap_or_default(),
                Err(error) => {
                    eprintln!("Model catalog: {error}");
                    Catalog::default()
                }
            };
            let _ = this.update(cx, |this, cx| {
                // Lists reported meanwhile are newer than the saved ones.
                for (profile, list) in saved.models {
                    this.catalog.models.entry(profile).or_insert(list);
                }
                let toggled = std::mem::replace(&mut this.catalog.favorites, saved.favorites);
                for favorite in toggled {
                    this.catalog
                        .toggle_favorite(&favorite.profile, favorite.model.as_deref());
                }
                this.loaded = true;
                if this.unsaved {
                    this.save(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The account's models as last listed.
    pub fn models(&self, profile: &str) -> Option<&[ModelInfo]> {
        self.catalog.models.get(profile).map(Vec::as_slice)
    }
    pub fn fetching(&self, profile: &str) -> bool {
        self.fetching.contains(profile)
    }
    pub fn error(&self, profile: &str) -> Option<&str> {
        self.errors.get(profile).map(String::as_str)
    }
    pub fn favorites(&self) -> &[Favorite] {
        &self.catalog.favorites
    }
    pub fn is_favorite(&self, profile: &str, model: Option<&str>) -> bool {
        self.catalog.is_favorite(profile, model)
    }

    /// A list the account reported (a chat's session or a background listing).
    pub fn set_models(&mut self, profile: &str, list: Vec<ModelInfo>, cx: &mut Context<Self>) {
        if self.catalog.set_models(profile, list) {
            self.save(cx);
            cx.notify();
        }
    }

    pub fn toggle_favorite(&mut self, profile: &str, model: Option<&str>, cx: &mut Context<Self>) {
        self.catalog.toggle_favorite(profile, model);
        self.save(cx);
        cx.notify();
    }

    /// Lists `profile`'s models again with its CLI, off the UI thread, in `pane`'s
    /// folder; one listing per account at a time.
    pub fn refresh(
        &mut self,
        provider: Provider,
        profile: String,
        pane: &Pane,
        cx: &mut Context<Self>,
    ) {
        if self.fetching.contains(&profile) {
            return;
        }
        let tools = cx.global::<AppState>().tools.read(cx);
        let Some(Ok(env)) = tools.environment.clone() else {
            return;
        };
        let catalog = tools.catalog.clone();
        let mut pane = pane.clone();
        pane.tool = provider.tool().to_owned();
        pane.metadata.profile_id = Some(profile.clone());
        pane.metadata.resume_id = None;
        self.fetching.insert(profile.clone());
        cx.spawn(async move |this, cx| {
            let listed = cx
                .background_spawn(async move {
                    let launch = match provider {
                        Provider::Claude => {
                            prepare_claude(&catalog, &pane, &env, &ChatSettings::default(), None)?
                        }
                        Provider::Codex => prepare_codex(&catalog, &pane, &env)?,
                    };
                    models::fetch(launch, provider)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.fetching.remove(&profile);
                match listed {
                    Ok(list) => {
                        this.errors.remove(&profile);
                        this.set_models(&profile, list, cx);
                    }
                    Err(error) => {
                        this.errors.insert(profile, error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if !self.loaded {
            self.unsaved = true;
            return;
        }
        self.unsaved = false;
        let (Some(client), Some(payload)) = (self.client.clone(), self.catalog.to_json()) else {
            return;
        };
        let previous = self.save.take();
        self.save = Some(cx.background_spawn(async move {
            if let Some(previous) = previous {
                previous.await;
            }
            if let Err(error) = client.save_model_catalog(payload).await {
                eprintln!("Model catalog: {error}");
            }
        }));
    }
}
