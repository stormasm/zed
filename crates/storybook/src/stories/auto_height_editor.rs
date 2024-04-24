use editor::Editor;
use gpui::{
    black, div, white, IntoElement, KeyBinding, ParentElement, Render, Styled, View, ViewContext,
    VisualContext, WindowContext,
};

pub struct AutoHeightEditorStory {
    editor: View<Editor>,
}

impl AutoHeightEditorStory {
    pub fn new(cx: &mut WindowContext) -> View<Self> {
        cx.bind_keys([KeyBinding::new(
            "enter",
            editor::actions::Newline,
            Some("Editor"),
        )]);
        cx.new_view(|cx| Self {
            editor: cx.new_view(|cx| {
                let mut editor = Editor::auto_height(3, cx);
                editor.set_soft_wrap_mode(language::language_settings::SoftWrap::EditorWidth, cx);
                editor
            }),
        })
    }
}

impl Render for AutoHeightEditorStory {
    fn render(&mut self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(black())
            .text_lg()
            .child(div().w_80().bg(white()).child(self.editor.clone()))
    }
}
