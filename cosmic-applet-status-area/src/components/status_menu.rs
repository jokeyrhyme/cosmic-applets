use cosmic::iced;

use crate::subscriptions::status_notifier_item::{Layout, StatusNotifierItem};

#[derive(Clone, Debug)]
pub enum Msg {
    Layout(Result<Layout, String>),
    Click(i32, bool),
}

pub struct State {
    item: StatusNotifierItem,
    layout: Option<Layout>,
    expanded: Option<i32>,
}

impl State {
    pub fn new(item: StatusNotifierItem) -> (Self, iced::Command<Msg>) {
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
            Msg::Click(id, is_submenu) => {
                let menu_proxy = self.item.menu_proxy().clone();
                tokio::spawn(async move {
                    let _ = menu_proxy.event(id, "clicked", &0.into(), 0).await;
                });
                if is_submenu {
                    self.expanded = if self.expanded != Some(id) {
                        Some(id)
                    } else {
                        None
                    };
                } else {
                    // TODO: Close menu?
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
            layout_view(layout, self.expanded)
        } else {
            iced::widget::text("").into()
        }
    }

    pub fn subscription(&self) -> iced::Subscription<Msg> {
        self.item.layout_subscription().map(Msg::Layout)
    }
}

fn layout_view(layout: &Layout, expanded: Option<i32>) -> cosmic::Element<Msg> {
    iced::widget::column(
        layout
            .children()
            .iter()
            .filter_map(|i| {
                if i.type_().as_deref() == Some("separator") {
                    Some(cosmic::widget::horizontal_rule(2).into())
                } else if let Some(label) = i.label() {
                    let mut label = label.to_string();
                    if let Some(toggle_state) = i.toggle_state() {
                        if toggle_state != 0 {
                            label = format!("✓ {}", label);
                        }
                    }

                    let is_submenu = i.children_display().as_deref() == Some("submenu");

                    let text = iced::widget::text(label);

                    let children = if let Some(icon_data) = i.icon_data() {
                        let handle = iced::widget::image::Handle::from_memory(icon_data.to_vec());
                        let image = iced::widget::Image::new(handle);
                        vec![text.into(), image.into()]
                    } else {
                        vec![text.into()]
                    };
                    let button = cosmic::widget::button(cosmic::theme::Button::Link)
                        .custom(children)
                        .on_press(Msg::Click(i.id(), is_submenu));

                    if is_submenu && expanded == Some(i.id()) {
                        Some(
                            iced::widget::column![
                                button,
                                layout_view(i, None), // XXX nested?
                            ]
                            .into(),
                        )
                    } else {
                        Some(button.into())
                    }
                } else {
                    None
                }
            })
            .collect(),
    )
    .into()
}
