use crate::{
    pane::{self, Pane},
    persistence::model::ItemId,
    searchable::SearchableItemHandle,
    workspace_settings::{AutosaveSetting, WorkspaceSettings},
    DelayedDebouncedEditAction, FollowableItemBuilders, ItemNavHistory, ToolbarItemLocation,
    ViewId, Workspace, WorkspaceId,
};
use anyhow::Result;
use client::{
    proto::{self, PeerId},
    Client,
};
use futures::{channel::mpsc, StreamExt};
use gpui::{
    AnyElement, AnyView, AppContext, Entity, EntityId, EventEmitter, FocusHandle, FocusableView,
    HighlightStyle, Model, Pixels, Point, SharedString, Task, View, ViewContext, WeakView,
    WindowContext,
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use settings::Settings;
use smallvec::SmallVec;
use std::{
    any::{Any, TypeId},
    cell::RefCell,
    ops::Range,
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::Duration,
};
use theme::Theme;
use ui::Element as _;

pub const LEADER_UPDATE_THROTTLE: Duration = Duration::from_millis(200);

#[derive(Deserialize)]
pub struct ItemSettings {
    pub git_status: bool,
    pub close_position: ClosePosition,
}

#[derive(Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ClosePosition {
    Left,
    #[default]
    Right,
}

impl ClosePosition {
    pub fn right(&self) -> bool {
        match self {
            ClosePosition::Left => false,
            ClosePosition::Right => true,
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ItemSettingsContent {
    /// Whether to show the Git file status on a tab item.
    ///
    /// Default: true
    git_status: Option<bool>,
    /// Position of the close button in a tab.
    ///
    /// Default: right
    close_position: Option<ClosePosition>,
}

impl Settings for ItemSettings {
    const KEY: Option<&'static str> = Some("tabs");

    type FileContent = ItemSettingsContent;

    fn load(
        default_value: &Self::FileContent,
        user_values: &[&Self::FileContent],
        _: &mut AppContext,
    ) -> Result<Self> {
        Self::load_via_json_merge(default_value, user_values)
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub enum ItemEvent {
    CloseItem,
    UpdateTab,
    UpdateBreadcrumbs,
    Edit,
}

// TODO: Combine this with existing HighlightedText struct?
pub struct BreadcrumbText {
    pub text: String,
    pub highlights: Option<Vec<(Range<usize>, HighlightStyle)>>,
}

pub trait Item: FocusableView + EventEmitter<Self::Event> {
    type Event;
    fn tab_content(
        &self,
        _detail: Option<usize>,
        _selected: bool,
        _cx: &WindowContext,
    ) -> AnyElement {
        gpui::Empty.into_any()
    }
    fn to_item_events(_event: &Self::Event, _f: impl FnMut(ItemEvent)) {}

    fn deactivated(&mut self, _: &mut ViewContext<Self>) {}
    fn workspace_deactivated(&mut self, _: &mut ViewContext<Self>) {}
    fn navigate(&mut self, _: Box<dyn Any>, _: &mut ViewContext<Self>) -> bool {
        false
    }
    fn tab_tooltip_text(&self, _: &AppContext) -> Option<SharedString> {
        None
    }
    fn tab_description(&self, _: usize, _: &AppContext) -> Option<SharedString> {
        None
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        None
    }

    fn is_singleton(&self, _cx: &AppContext) -> bool {
        false
    }
    fn set_nav_history(&mut self, _: ItemNavHistory, _: &mut ViewContext<Self>) {}
    fn clone_on_split(
        &self,
        _workspace_id: WorkspaceId,
        _: &mut ViewContext<Self>,
    ) -> Option<View<Self>>
    where
        Self: Sized,
    {
        None
    }
    fn is_dirty(&self, _: &AppContext) -> bool {
        false
    }
    fn has_conflict(&self, _: &AppContext) -> bool {
        false
    }
    fn can_save(&self, _cx: &AppContext) -> bool {
        false
    }

    fn act_as_type<'a>(
        &'a self,
        type_id: TypeId,
        self_handle: &'a View<Self>,
        _: &'a AppContext,
    ) -> Option<AnyView> {
        if TypeId::of::<Self>() == type_id {
            Some(self_handle.clone().into())
        } else {
            None
        }
    }

    fn as_searchable(&self, _: &View<Self>) -> Option<Box<dyn SearchableItemHandle>> {
        None
    }

    fn breadcrumb_location(&self) -> ToolbarItemLocation {
        ToolbarItemLocation::Hidden
    }

    fn breadcrumbs(&self, _theme: &Theme, _cx: &AppContext) -> Option<Vec<BreadcrumbText>> {
        None
    }

    fn added_to_workspace(&mut self, _workspace: &mut Workspace, _cx: &mut ViewContext<Self>) {}

    fn serialized_item_kind() -> Option<&'static str> {
        None
    }

    fn deserialize(
        _workspace: WeakView<Workspace>,
        _workspace_id: WorkspaceId,
        _item_id: ItemId,
        _cx: &mut ViewContext<Pane>,
    ) -> Task<Result<View<Self>>> {
        unimplemented!(
            "deserialize() must be implemented if serialized_item_kind() returns Some(_)"
        )
    }

    fn show_toolbar(&self) -> bool {
        true
    }
    fn pixel_position_of_cursor(&self, _: &AppContext) -> Option<Point<Pixels>> {
        None
    }
}

pub trait ItemHandle: 'static + Send {
    fn subscribe_to_item_events(
        &self,
        cx: &mut WindowContext,
        handler: Box<dyn Fn(ItemEvent, &mut WindowContext)>,
    ) -> gpui::Subscription;
    fn focus_handle(&self, cx: &WindowContext) -> FocusHandle;
    fn tab_tooltip_text(&self, cx: &AppContext) -> Option<SharedString>;
    fn tab_description(&self, detail: usize, cx: &AppContext) -> Option<SharedString>;
    fn tab_content(&self, detail: Option<usize>, selected: bool, cx: &WindowContext) -> AnyElement;
    fn telemetry_event_text(&self, cx: &WindowContext) -> Option<&'static str>;
    fn dragged_tab_content(&self, detail: Option<usize>, cx: &WindowContext) -> AnyElement;

    fn is_singleton(&self, cx: &AppContext) -> bool;
    fn boxed_clone(&self) -> Box<dyn ItemHandle>;
    fn clone_on_split(
        &self,
        workspace_id: WorkspaceId,
        cx: &mut WindowContext,
    ) -> Option<Box<dyn ItemHandle>>;
    fn added_to_pane(
        &self,
        workspace: &mut Workspace,
        pane: View<Pane>,
        cx: &mut ViewContext<Workspace>,
    );
    fn deactivated(&self, cx: &mut WindowContext);
    fn workspace_deactivated(&self, cx: &mut WindowContext);
    fn navigate(&self, data: Box<dyn Any>, cx: &mut WindowContext) -> bool;
    fn item_id(&self) -> EntityId;
    fn to_any(&self) -> AnyView;
    fn is_dirty(&self, cx: &AppContext) -> bool;
    fn has_conflict(&self, cx: &AppContext) -> bool;

    fn act_as_type(&self, type_id: TypeId, cx: &AppContext) -> Option<AnyView>;
    fn to_followable_item_handle(&self, cx: &AppContext) -> Option<Box<dyn FollowableItemHandle>>;
    fn on_release(
        &self,
        cx: &mut AppContext,
        callback: Box<dyn FnOnce(&mut AppContext) + Send>,
    ) -> gpui::Subscription;
    fn to_searchable_item_handle(&self, cx: &AppContext) -> Option<Box<dyn SearchableItemHandle>>;
    fn breadcrumb_location(&self, cx: &AppContext) -> ToolbarItemLocation;
    fn breadcrumbs(&self, theme: &Theme, cx: &AppContext) -> Option<Vec<BreadcrumbText>>;
    fn serialized_item_kind(&self) -> Option<&'static str>;
    fn show_toolbar(&self, cx: &AppContext) -> bool;
    fn pixel_position_of_cursor(&self, cx: &AppContext) -> Option<Point<Pixels>>;
}

pub trait WeakItemHandle: Send + Sync {
    fn id(&self) -> EntityId;
    fn upgrade(&self) -> Option<Box<dyn ItemHandle>>;
}

impl dyn ItemHandle {
    pub fn downcast<V: 'static>(&self) -> Option<View<V>> {
        self.to_any().downcast().ok()
    }

    pub fn act_as<V: 'static>(&self, cx: &AppContext) -> Option<View<V>> {
        self.act_as_type(TypeId::of::<V>(), cx)
            .and_then(|t| t.downcast().ok())
    }
}

impl<T: Item> ItemHandle for View<T> {
    fn subscribe_to_item_events(
        &self,
        cx: &mut WindowContext,
        handler: Box<dyn Fn(ItemEvent, &mut WindowContext)>,
    ) -> gpui::Subscription {
        cx.subscribe(self, move |_, event, cx| {
            T::to_item_events(event, |item_event| handler(item_event, cx));
        })
    }

    fn focus_handle(&self, cx: &WindowContext) -> FocusHandle {
        self.focus_handle(cx)
    }

    fn tab_tooltip_text(&self, cx: &AppContext) -> Option<SharedString> {
        self.read(cx).tab_tooltip_text(cx)
    }

    fn telemetry_event_text(&self, cx: &WindowContext) -> Option<&'static str> {
        self.read(cx).telemetry_event_text()
    }

    fn tab_description(&self, detail: usize, cx: &AppContext) -> Option<SharedString> {
        self.read(cx).tab_description(detail, cx)
    }

    fn tab_content(&self, detail: Option<usize>, selected: bool, cx: &WindowContext) -> AnyElement {
        self.read(cx).tab_content(detail, selected, cx)
    }

    fn dragged_tab_content(&self, detail: Option<usize>, cx: &WindowContext) -> AnyElement {
        self.read(cx).tab_content(detail, true, cx)
    }

    fn is_singleton(&self, cx: &AppContext) -> bool {
        self.read(cx).is_singleton(cx)
    }

    fn boxed_clone(&self) -> Box<dyn ItemHandle> {
        Box::new(self.clone())
    }

    fn clone_on_split(
        &self,
        workspace_id: WorkspaceId,
        cx: &mut WindowContext,
    ) -> Option<Box<dyn ItemHandle>> {
        self.update(cx, |item, cx| item.clone_on_split(workspace_id, cx))
            .map(|handle| Box::new(handle) as Box<dyn ItemHandle>)
    }

    fn added_to_pane(
        &self,
        workspace: &mut Workspace,
        pane: View<Pane>,
        cx: &mut ViewContext<Workspace>,
    ) {
        let weak_item = self.downgrade();
        let history = pane.read(cx).nav_history_for_item(self);
        self.update(cx, |this, cx| {
            this.set_nav_history(history, cx);
            this.added_to_workspace(workspace, cx);
        });

        if workspace
            .panes_by_item
            .insert(self.item_id(), pane.downgrade())
            .is_none()
        {
            let mut pending_autosave = DelayedDebouncedEditAction::new();
            let (pending_update_tx, mut pending_update_rx) = mpsc::unbounded();
            let pending_update = Rc::new(RefCell::new(None));

            let mut event_subscription =
                Some(cx.subscribe(self, move |workspace, item, event, cx| {
                    let pane = if let Some(pane) = workspace
                        .panes_by_item
                        .get(&item.item_id())
                        .and_then(|pane| pane.upgrade())
                    {
                        pane
                    } else {
                        log::error!("unexpected item event after pane was dropped");
                        return;
                    };

                    if let Some(item) = item.to_followable_item_handle(cx) {
                        let leader_id = workspace.leader_for_pane(&pane);
                        let follow_event = item.to_follow_event(event);
                        if leader_id.is_some()
                            && matches!(follow_event, Some(FollowEvent::Unfollow))
                        {
                            workspace.unfollow(&pane, cx);
                        }

                        if item.focus_handle(cx).contains_focused(cx) {
                            item.add_event_to_update_proto(
                                event,
                                &mut pending_update.borrow_mut(),
                                cx,
                            );
                            pending_update_tx.unbounded_send(leader_id).ok();
                        }
                    }

                    T::to_item_events(event, |event| match event {
                        ItemEvent::CloseItem => {
                            pane.update(cx, |pane, cx| {
                                pane.close_item_by_id(item.item_id(), crate::SaveIntent::Close, cx)
                            })
                            .detach_and_log_err(cx);
                            return;
                        }

                        ItemEvent::UpdateTab => {
                            pane.update(cx, |_, cx| {
                                cx.emit(pane::Event::ChangeItemTitle);
                                cx.notify();
                            });
                        }

                        _ => {}
                    });
                }));

            let item_id = self.item_id();
            cx.observe_release(self, move |workspace, _, _| {
                workspace.panes_by_item.remove(&item_id);
                event_subscription.take();
            })
            .detach();
        }

        cx.defer(|workspace, cx| {
            workspace.serialize_workspace(cx).detach();
        });
    }

    fn deactivated(&self, cx: &mut WindowContext) {
        self.update(cx, |this, cx| this.deactivated(cx));
    }

    fn workspace_deactivated(&self, cx: &mut WindowContext) {
        self.update(cx, |this, cx| this.workspace_deactivated(cx));
    }

    fn navigate(&self, data: Box<dyn Any>, cx: &mut WindowContext) -> bool {
        self.update(cx, |this, cx| this.navigate(data, cx))
    }

    fn item_id(&self) -> EntityId {
        self.entity_id()
    }

    fn to_any(&self) -> AnyView {
        self.clone().into()
    }

    fn is_dirty(&self, cx: &AppContext) -> bool {
        self.read(cx).is_dirty(cx)
    }

    fn has_conflict(&self, cx: &AppContext) -> bool {
        self.read(cx).has_conflict(cx)
    }

    fn act_as_type<'a>(&'a self, type_id: TypeId, cx: &'a AppContext) -> Option<AnyView> {
        self.read(cx).act_as_type(type_id, self, cx)
    }

    fn to_followable_item_handle(&self, cx: &AppContext) -> Option<Box<dyn FollowableItemHandle>> {
        let builders = cx.try_global::<FollowableItemBuilders>()?;
        let item = self.to_any();
        Some(builders.get(&item.entity_type())?.1(&item))
    }

    fn on_release(
        &self,
        cx: &mut AppContext,
        callback: Box<dyn FnOnce(&mut AppContext) + Send>,
    ) -> gpui::Subscription {
        cx.observe_release(self, move |_, cx| callback(cx))
    }

    fn to_searchable_item_handle(&self, cx: &AppContext) -> Option<Box<dyn SearchableItemHandle>> {
        self.read(cx).as_searchable(self)
    }

    fn breadcrumb_location(&self, cx: &AppContext) -> ToolbarItemLocation {
        self.read(cx).breadcrumb_location()
    }

    fn breadcrumbs(&self, theme: &Theme, cx: &AppContext) -> Option<Vec<BreadcrumbText>> {
        self.read(cx).breadcrumbs(theme, cx)
    }

    fn serialized_item_kind(&self) -> Option<&'static str> {
        T::serialized_item_kind()
    }

    fn show_toolbar(&self, cx: &AppContext) -> bool {
        self.read(cx).show_toolbar()
    }

    fn pixel_position_of_cursor(&self, cx: &AppContext) -> Option<Point<Pixels>> {
        self.read(cx).pixel_position_of_cursor(cx)
    }
}

impl From<Box<dyn ItemHandle>> for AnyView {
    fn from(val: Box<dyn ItemHandle>) -> Self {
        val.to_any()
    }
}

impl From<&Box<dyn ItemHandle>> for AnyView {
    fn from(val: &Box<dyn ItemHandle>) -> Self {
        val.to_any()
    }
}

impl Clone for Box<dyn ItemHandle> {
    fn clone(&self) -> Box<dyn ItemHandle> {
        self.boxed_clone()
    }
}

impl<T: Item> WeakItemHandle for WeakView<T> {
    fn id(&self) -> EntityId {
        self.entity_id()
    }

    fn upgrade(&self) -> Option<Box<dyn ItemHandle>> {
        self.upgrade().map(|v| Box::new(v) as Box<dyn ItemHandle>)
    }
}

#[derive(Debug)]
pub enum FollowEvent {
    Unfollow,
}

pub trait FollowableItem: Item {
    fn remote_id(&self) -> Option<ViewId>;

    fn to_state_proto(&self, cx: &WindowContext) -> Option<proto::view::Variant>;
    fn from_state_proto(
        pane: View<Pane>,
        id: ViewId,
        state: &mut Option<proto::view::Variant>,
        cx: &mut WindowContext,
    ) -> Option<Task<Result<View<Self>>>>;

    fn to_follow_event(event: &Self::Event) -> Option<FollowEvent>;
    fn add_event_to_update_proto(
        &self,
        event: &Self::Event,
        update: &mut Option<proto::update_view::Variant>,
        cx: &WindowContext,
    ) -> bool;
    fn set_leader_peer_id(&mut self, leader_peer_id: Option<PeerId>, cx: &mut ViewContext<Self>);
}

pub trait FollowableItemHandle: ItemHandle {
    fn remote_id(&self, client: &Arc<Client>, cx: &WindowContext) -> Option<ViewId>;
    fn downgrade(&self) -> Box<dyn WeakFollowableItemHandle>;
    fn set_leader_peer_id(&self, leader_peer_id: Option<PeerId>, cx: &mut WindowContext);
    fn to_state_proto(&self, cx: &WindowContext) -> Option<proto::view::Variant>;
    fn add_event_to_update_proto(
        &self,
        event: &dyn Any,
        update: &mut Option<proto::update_view::Variant>,
        cx: &WindowContext,
    ) -> bool;
    fn to_follow_event(&self, event: &dyn Any) -> Option<FollowEvent>;
}

impl<T: FollowableItem> FollowableItemHandle for View<T> {
    fn remote_id(&self, client: &Arc<Client>, cx: &WindowContext) -> Option<ViewId> {
        self.read(cx).remote_id().or_else(|| {
            client.peer_id().map(|creator| ViewId {
                creator,
                id: self.item_id().as_u64(),
            })
        })
    }

    fn downgrade(&self) -> Box<dyn WeakFollowableItemHandle> {
        Box::new(self.downgrade())
    }

    fn set_leader_peer_id(&self, leader_peer_id: Option<PeerId>, cx: &mut WindowContext) {
        self.update(cx, |this, cx| this.set_leader_peer_id(leader_peer_id, cx))
    }

    fn to_state_proto(&self, cx: &WindowContext) -> Option<proto::view::Variant> {
        self.read(cx).to_state_proto(cx)
    }

    fn add_event_to_update_proto(
        &self,
        event: &dyn Any,
        update: &mut Option<proto::update_view::Variant>,
        cx: &WindowContext,
    ) -> bool {
        if let Some(event) = event.downcast_ref() {
            self.read(cx).add_event_to_update_proto(event, update, cx)
        } else {
            false
        }
    }

    fn to_follow_event(&self, event: &dyn Any) -> Option<FollowEvent> {
        T::to_follow_event(event.downcast_ref()?)
    }
}

pub trait WeakFollowableItemHandle: Send + Sync {
    fn upgrade(&self) -> Option<Box<dyn FollowableItemHandle>>;
}

impl<T: FollowableItem> WeakFollowableItemHandle for WeakView<T> {
    fn upgrade(&self) -> Option<Box<dyn FollowableItemHandle>> {
        Some(Box::new(self.upgrade()?))
    }
}
