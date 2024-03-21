pub mod dock;
pub mod item;
mod modal_layer;
pub mod notifications;
pub mod pane;
pub mod pane_group;
mod persistence;
pub mod searchable;
pub mod shared_screen;
mod status_bar;
mod toolbar;
mod workspace_settings;

use gpui::*;
use item::{FollowableItem, FollowableItemHandle, Item, ItemHandle, ItemSettings, ProjectItem};
pub use pane::*;
pub use pane_group::*;
pub use toolbar::{Toolbar, ToolbarItemEvent, ToolbarItemLocation, ToolbarItemView};

pub use workspace_settings::{AutosaveSetting, WorkspaceSettings};

#[derive(Clone, Render)]
struct DraggedDock(DockPosition);

pub enum Event {
    PaneAdded(View<Pane>),
    ContactRequestedJoin(u64),
    WorkspaceCreated(WeakView<Workspace>),
    SpawnTask(SpawnInTerminal),
}

pub enum OpenVisible {
    All,
    None,
    OnlyFiles,
    OnlyDirectories,
}

actions!(
    workspace,
    [
        Open,
        NewFile,
        NewWindow,
        CloseWindow,
        AddFolderToProject,
        Unfollow,
        SaveAs,
        SaveWithoutFormat,
        ReloadActiveItem,
        ActivatePreviousPane,
        ActivateNextPane,
        FollowNextCollaborator,
        NewTerminal,
        NewCenterTerminal,
        NewSearch,
        Feedback,
        Restart,
        Welcome,
        ToggleZoom,
        ToggleLeftDock,
        ToggleRightDock,
        ToggleBottomDock,
        CloseAllDocks,
    ]
);

pub struct Workspace {}

pub struct AppState {
    //pub languages: Arc<LanguageRegistry>,
    //pub client: Arc<Client>,
    //pub user_store: Model<UserStore>,
    //pub workspace_store: Model<WorkspaceStore>,
    //pub fs: Arc<dyn fs::Fs>,
    pub build_window_options: fn(Option<Uuid>, &mut AppContext) -> WindowOptions,
    //pub node_runtime: Arc<dyn NodeRuntime>,
}

#[derive(Deserialize)]
pub struct Toast {
    id: usize,
    msg: Cow<'static, str>,
    #[serde(skip)]
    on_click: Option<(Cow<'static, str>, Arc<dyn Fn(&mut WindowContext)>)>,
}

impl Toast {
    pub fn new<I: Into<Cow<'static, str>>>(id: usize, msg: I) -> Self {
        Toast {
            id,
            msg: msg.into(),
            on_click: None,
        }
    }

    pub fn on_click<F, M>(mut self, message: M, on_click: F) -> Self
    where
        M: Into<Cow<'static, str>>,
        F: Fn(&mut WindowContext) + 'static,
    {
        self.on_click = Some((message.into(), Arc::new(on_click)));
        self
    }
}

impl PartialEq for Toast {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.msg == other.msg
            && self.on_click.is_some() == other.on_click.is_some()
    }
}

impl Clone for Toast {
    fn clone(&self) -> Self {
        Toast {
            id: self.id,
            msg: self.msg.clone(),
            on_click: self.on_click.clone(),
        }
    }
}
