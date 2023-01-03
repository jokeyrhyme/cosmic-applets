use cosmic::iced;

use crate::subscriptions::status_notifier_item::{Layout, StatusNotifierItem};

#[derive(Clone, Debug)]
pub enum Msg {
    Layout(Result<Layout, String>),
}

pub struct State {
    item: StatusNotifierItem,
    layout: Option<Layout>,
    expanded: Option<i32>,
}

impl State {
    pub fn new(
        connection: &zbus::Connection,
        item: StatusNotifierItem,
    ) -> (Self, iced::Command<Msg>) {
        (
            Self {
                item,
                layout: None,
                expanded: None,
            },
            iced::Command::none(),
        )
    }

    pub fn update(&mut self, message: Msg) -> iced::Command<Msg> {
        match message {
            Msg::Layout(layout) => {
                match layout {
                    Ok(layout) => {
                        self.layout = Some(layout);
                    }
                    Err(err) => eprintln!("Error getting layout from icon: {}", err),
                }
                iced::Command::none()
            }
        }
    }

    pub fn name(&self) -> &str {
        self.item.name()
    }

    pub fn icon_name(&self) -> &str {
        self.item.icon_name()
    }

    pub fn popup_view(&self) -> cosmic::Element<Msg> {
        if let Some(layout) = self.layout.as_ref() {
            layout_view(layout)
        } else {
            iced::widget::text("").into()
        }
    }

    pub fn subscription(&self) -> iced::Subscription<Msg> {
        self.item.layout_subscription().map(Msg::Layout)
    }
}

fn layout_view(layout: &Layout) -> cosmic::Element<Msg> {
    iced::widget::column(
        layout
            .children()
            .iter()
            .filter_map(|i| {
                if i.type_().as_deref() == Some("separator") {
                    Some(iced::widget::horizontal_rule(2).into())
                } else if let Some(label) = i.label() {
                    if let Some(toggle_state) = i.toggle_state() {}

                    if let Some(icon_data) = i.icon_data() {}

                    if i.children_display().as_deref() == Some("submenu") {
                        layout_view(i);
                    }

                    // XXX
                    Some(iced::widget::text(label).into())
                } else {
                    None
                }
            })
            .collect(),
    )
    .into()
}
