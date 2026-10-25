//! F1 help and focus return, including inputs with no explicit widget ID.
use crate::{
    state::{App, Message},
    theme::Palette,
    ui::{font_medium, font_regular},
};
use design_tokens::scale::{SPACE, TYPE};
use iced::advanced::widget::{
    operation::{self, Focusable},
    Operation,
};
use iced::widget::{button, column, container, row, scrollable, space, text, text_input};
use iced::{Element, Fill, Rectangle, Task};

const TOPICS: [(&str, &str); 4] = [
    ("Ping idle", "Round-trip time while the link is not running a throughput test. Lower values mean a faster response."),
    ("Ping under load", "Round-trip time while download or upload is running. A rise from idle ping shows delay while the link is busy."),
    ("Jitter", "Variation between successive ping times, in milliseconds. Lower jitter means more consistent response times."),
    ("Shortcuts", "F1 opens Help. Ctrl+, opens Settings. Escape clears Help search first, then closes Help and returns focus. With Help closed, Escape dismisses Undo. Ctrl+Z outside text inputs restores a removed target while its 10-second toast is shown."),
];

pub(crate) fn search_id() -> iced::widget::Id {
    iced::widget::Id::new("help-search")
}

pub fn view(app: &App, palette: Palette) -> Element<'_, Message> {
    let head = row![
        text("Help").size(TYPE.xl).font(font_medium()),
        space::horizontal(),
        button(text("Close (Esc)").size(TYPE.sm))
            .padding([SPACE.s2, SPACE.s3])
            .style(palette.btn_secondary())
            .on_press(Message::HelpClosed)
    ]
    .align_y(iced::Center);
    let search = text_input("Search help", &app.help_query)
        .id(search_id())
        .size(TYPE.sm)
        .padding([SPACE.s2, SPACE.s3])
        .style(palette.text_input())
        .on_input(Message::HelpSearchChanged);
    let query = app.help_query.trim().to_lowercase();
    let topics: Vec<Element<'_, Message>> = TOPICS
        .iter()
        .filter(|(title, body)| format!("{title} {body}").to_lowercase().contains(&query))
        .map(|(title, body)| {
            column![
                text(*title).size(TYPE.md).font(font_medium()),
                text(*body).size(TYPE.sm).font(font_regular())
            ]
            .spacing(SPACE.s2)
            .into()
        })
        .collect();
    let body: Element<'_, Message> = if topics.is_empty() {
        text("No matching help topic.").size(TYPE.sm).into()
    } else {
        column(topics).spacing(SPACE.s6).into()
    };
    let panel = container(column![head, search, scrollable(body).height(Fill)].spacing(SPACE.s4))
        .padding(SPACE.s6)
        .max_width(560.0)
        .height(Fill)
        .style(move |_| {
            container::Style::default()
                .background(palette.raised())
                .color(palette.text())
                .border(iced::Border {
                    color: palette.border(),
                    width: 1.0,
                    ..Default::default()
                })
        });
    container(panel)
        .padding(SPACE.s4)
        .align_right(Fill)
        .height(Fill)
        .style(move |_| container::Style::default().background(palette.bg()))
        .into()
}

// Ordinals also identify existing inputs which were built without a widget ID.
// The underlying screen stays mounted while the panel is open.
pub(crate) fn capture_focus() -> Task<Option<usize>> {
    iced::advanced::widget::operate(operation::focusable::count()).map(|count| count.focused)
}

// iced visits a text input immediately before its Focusable state, even without an ID.
#[derive(Default)]
struct TextInputFocus {
    input_bounds: Option<Rectangle>,
    focused: bool,
}
impl Operation<bool> for TextInputFocus {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<bool>)) {
        operate(self);
    }
    fn text_input(
        &mut self,
        _: Option<&iced::widget::Id>,
        bounds: Rectangle,
        _: &mut dyn operation::TextInput,
    ) {
        self.input_bounds = Some(bounds);
    }
    fn focusable(
        &mut self,
        _: Option<&iced::widget::Id>,
        bounds: Rectangle,
        state: &mut dyn Focusable,
    ) {
        if self.input_bounds.take() == Some(bounds) && state.is_focused() {
            self.focused = true;
        }
    }
    fn finish(&self) -> operation::Outcome<bool> {
        operation::Outcome::Some(self.focused)
    }
}
pub(crate) fn text_input_focused() -> Task<bool> {
    iced::advanced::widget::operate(TextInputFocus::default())
}

struct RestoreFocus {
    index: usize,
    target: Option<usize>,
}
impl<T> Operation<T> for RestoreFocus {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<T>)) {
        operate(self);
    }
    fn focusable(&mut self, _: Option<&iced::widget::Id>, _: Rectangle, state: &mut dyn Focusable) {
        if self.target == Some(self.index) {
            state.focus();
        } else {
            state.unfocus();
        }
        self.index += 1;
    }
}
pub(crate) fn restore_focus(target: Option<usize>) -> Task<Message> {
    iced::advanced::widget::operate(RestoreFocus { index: 0, target })
}

/// A focusable Help opener. iced 0.14 buttons have no Focusable state.
pub(crate) fn opener(palette: Palette) -> Element<'static, Message> {
    let child = button(text("Help (F1)").size(TYPE.xs))
        .padding([SPACE.s2, SPACE.s3])
        .style(palette.btn_ghost())
        .on_press(Message::HelpRequested);
    Element::new(HelpOpener {
        child: child.into(),
        palette,
    })
}

#[derive(Default)]
struct OpenerFocus(bool);
impl Focusable for OpenerFocus {
    fn is_focused(&self) -> bool {
        self.0
    }
    fn focus(&mut self) {
        self.0 = true;
    }
    fn unfocus(&mut self) {
        self.0 = false;
    }
}

struct HelpOpener {
    child: Element<'static, Message>,
    palette: Palette,
}
impl iced::advanced::Widget<Message, iced::Theme, iced::Renderer> for HelpOpener {
    fn tag(&self) -> iced::advanced::widget::tree::Tag {
        iced::advanced::widget::tree::Tag::of::<OpenerFocus>()
    }
    fn state(&self) -> iced::advanced::widget::tree::State {
        iced::advanced::widget::tree::State::new(OpenerFocus::default())
    }
    fn children(&self) -> Vec<iced::advanced::widget::Tree> {
        vec![iced::advanced::widget::Tree::new(&self.child)]
    }
    fn diff(&self, tree: &mut iced::advanced::widget::Tree) {
        tree.diff_children(&[&self.child]);
    }
    fn size(&self) -> iced::Size<iced::Length> {
        self.child.as_widget().size()
    }
    fn layout(
        &mut self,
        tree: &mut iced::advanced::widget::Tree,
        renderer: &iced::Renderer,
        limits: &iced::advanced::layout::Limits,
    ) -> iced::advanced::layout::Node {
        self.child
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }
    fn operate(
        &mut self,
        tree: &mut iced::advanced::widget::Tree,
        layout: iced::advanced::Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.focusable(
            Some(&iced::widget::Id::new("help-opener")),
            layout.bounds(),
            tree.state.downcast_mut::<OpenerFocus>(),
        );
        self.child
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }
    fn update(
        &mut self,
        tree: &mut iced::advanced::widget::Tree,
        event: &iced::Event,
        layout: iced::advanced::Layout<'_>,
        cursor: iced::mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn iced::advanced::Clipboard,
        shell: &mut iced::advanced::Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let focus = tree.state.downcast_mut::<OpenerFocus>();
        if let iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)) =
            event
        {
            focus.0 = cursor.is_over(layout.bounds());
        }
        if focus.0
            && matches!(
                event,
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(
                        iced::keyboard::key::Named::Enter | iced::keyboard::key::Named::Space
                    ),
                    ..
                })
            )
        {
            shell.publish(Message::HelpRequested);
            shell.capture_event();
            return;
        }
        self.child.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }
    fn draw(
        &self,
        tree: &iced::advanced::widget::Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &iced::advanced::renderer::Style,
        layout: iced::advanced::Layout<'_>,
        cursor: iced::mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.child.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
        if tree.state.downcast_ref::<OpenerFocus>().0 {
            use iced::advanced::Renderer as _;
            renderer.fill_quad(
                iced::advanced::renderer::Quad {
                    bounds: layout.bounds(),
                    border: iced::Border {
                        width: 2.0,
                        color: self.palette.theme.focus.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                iced::Color::TRANSPARENT,
            );
        }
    }
    fn mouse_interaction(
        &self,
        tree: &iced::advanced::widget::Tree,
        layout: iced::advanced::Layout<'_>,
        cursor: iced::mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> iced::mouse::Interaction {
        self.child.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Oracle: Escape returns to the opener, including widgets without an ID.
    #[test]
    fn escape_focus_operation_restores_the_recorded_control_without_ids() {
        let mut previous = OpenerFocus(true);
        let mut opener = OpenerFocus(false);
        let mut restore = RestoreFocus {
            index: 0,
            target: Some(1),
        };
        let operation: &mut dyn Operation<()> = &mut restore;
        operation.focusable(None, Rectangle::default(), &mut previous);
        operation.focusable(None, Rectangle::default(), &mut opener);
        assert!(!previous.is_focused());
        assert!(opener.is_focused());
    }

    // Oracle: opening Help without an opener must not focus a random control.
    #[test]
    fn escape_without_an_opener_leaves_controls_unfocused() {
        let mut control = OpenerFocus(true);
        let mut restore = RestoreFocus {
            index: 0,
            target: None,
        };
        let operation: &mut dyn Operation<()> = &mut restore;
        operation.focusable(None, Rectangle::default(), &mut control);
        assert!(!control.is_focused());
    }
}

#[cfg(test)]
mod shortcut_focus_tests {
    use super::*;
    type InputState = iced::widget::text_input::State<
        <iced::Renderer as iced::advanced::text::Renderer>::Paragraph,
    >;

    #[test]
    fn focused_text_input_without_id_is_detected() {
        let mut input = InputState::new();
        Focusable::focus(&mut input);
        let mut check = TextInputFocus::default();
        check.text_input(None, Rectangle::default(), &mut input);
        check.focusable(None, Rectangle::default(), &mut input);
        assert!(matches!(check.finish(), operation::Outcome::Some(true)));
    }

    #[test]
    fn unfocused_text_input_does_not_block_undo() {
        let mut input = InputState::new();
        let mut check = TextInputFocus::default();
        check.text_input(None, Rectangle::default(), &mut input);
        check.focusable(None, Rectangle::default(), &mut input);
        assert!(matches!(check.finish(), operation::Outcome::Some(false)));
    }

    #[test]
    fn focused_non_input_control_does_not_block_undo() {
        let mut control = OpenerFocus(true);
        let mut check = TextInputFocus::default();
        check.focusable(None, Rectangle::default(), &mut control);
        assert!(matches!(check.finish(), operation::Outcome::Some(false)));
    }
}
