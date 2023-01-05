use cosmic::{
    applet::CosmicAppletHelper,
    iced::{
        self,
        wayland::{
            actions::window::SctkWindowSettings,
            popup::{destroy_popup, get_popup},
            window::resize_window,
            InitialSurface, SurfaceIdWrapper,
        },
        Application, Command, Element, Settings, Subscription,
    },
    iced_native::window::Id as SurfaceId,
    iced_style::application::{self, Appearance},
};
use std::collections::BTreeMap;

use crate::{components::status_menu, subscriptions::status_notifier_watcher};

// XXX copied from libcosmic
const APPLET_PADDING: u32 = 8;

#[derive(Clone, Debug)]
pub enum Msg {
    Closed(SurfaceIdWrapper),
    // XXX don't use index (unique window id? or I guess that's created and destroyed)
    StatusMenu((usize, status_menu::Msg)),
    StatusNotifier(status_notifier_watcher::Event),
    TogglePopup(usize),
}

#[derive(Default)]
struct App {
    // TODO connect
    applet_helper: CosmicAppletHelper,
    connection: Option<zbus::Connection>,
    menus: BTreeMap<usize, status_menu::State>,
    open_menu: Option<usize>,
    max_menu_id: usize,
    max_popup_id: usize,
    popup: Option<SurfaceId>,
}

impl App {
    fn next_menu_id(&mut self) -> usize {
        self.max_menu_id += 1;
        self.max_menu_id
    }

    fn next_popup_id(&mut self) -> SurfaceId {
        self.max_popup_id += 1;
        SurfaceId::new(self.max_popup_id)
    }

    fn resize_window(&self) -> Command<Msg> {
        let icon_size = self.applet_helper.suggested_size().0 as u32 + APPLET_PADDING * 2;
        let n = self.menus.len() as u32;
        resize_window(SurfaceId::new(0), 1.max(icon_size * n), icon_size)
    }
}

impl Application for App {
    type Message = Msg;
    type Theme = cosmic::Theme;
    type Executor = iced::executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Msg>) {
        (Self::default(), Command::none())
    }

    fn title(&self) -> String {
        String::from("Status Area")
    }

    fn style(&self) -> <Self::Theme as application::StyleSheet>::Style {
        <Self::Theme as application::StyleSheet>::Style::Custom(|theme| Appearance {
            background_color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.0),
            text_color: theme.cosmic().on_bg_color().into(),
        })
    }

    fn update(&mut self, message: Msg) -> Command<Msg> {
        match message {
            Msg::Closed(surface) => {
                if matches!(surface, SurfaceIdWrapper::Popup(_)) {
                    self.popup = None;
                }
                Command::none()
            }
            Msg::StatusMenu((id, msg)) => match self.menus.get_mut(&id) {
                Some(state) => state.update(msg).map(move |msg| Msg::StatusMenu((id, msg))),
                None => Command::none(),
            },
            Msg::StatusNotifier(event) => match event {
                status_notifier_watcher::Event::Connected(connection) => {
                    self.connection = Some(connection);
                    Command::none()
                }
                status_notifier_watcher::Event::Registered(name) => {
                    let (state, cmd) = status_menu::State::new(name);
                    let id = self.next_menu_id();
                    self.menus.insert(id, state);
                    Command::batch([
                        self.resize_window(),
                        cmd.map(move |msg| Msg::StatusMenu((id, msg))),
                    ])
                }
                status_notifier_watcher::Event::Unregistered(name) => {
                    if let Some((id, _)) =
                        self.menus.iter().find(|(_id, menu)| menu.name() == &name)
                    {
                        let id = *id;
                        self.menus.remove(&id);
                        if self.open_menu == Some(id) {
                            self.open_menu = None;
                            if let Some(popup_id) = self.popup {
                                return destroy_popup(popup_id);
                            }
                        }
                    }
                    self.resize_window()
                }
                status_notifier_watcher::Event::Error(err) => Command::none(),
            },
            Msg::TogglePopup(id) => {
                self.open_menu = if self.open_menu != Some(id) {
                    Some(id)
                } else {
                    None
                };
                let mut cmds = Vec::new();
                if let Some(id) = self.popup {
                    cmds.push(destroy_popup(id));
                }
                if self.open_menu.is_some() {
                    let id = self.next_popup_id();
                    let popup_settings = self.applet_helper.get_popup_settings(
                        SurfaceId::new(0),
                        id,
                        None,
                        None,
                        None,
                    );
                    self.popup = Some(id);
                    cmds.push(get_popup(popup_settings));
                }
                Command::batch(cmds)
            }
        }
    }

    fn subscription(&self) -> Subscription<Msg> {
        let mut subscriptions = Vec::new();

        subscriptions.push(status_notifier_watcher::subscription().map(Msg::StatusNotifier));

        for (id, menu) in self.menus.iter() {
            subscriptions.push(menu.subscription().with(*id).map(Msg::StatusMenu));
        }

        iced::Subscription::batch(subscriptions)
    }

    fn view(&self, surface: SurfaceIdWrapper) -> Element<'_, Msg, iced::Renderer<Self::Theme>> {
        match surface {
            // XXX connect open event
            SurfaceIdWrapper::Window(_) => iced::widget::row(
                self.menus
                    .iter()
                    .map(|(id, menu)| {
                        self.applet_helper
                            .icon_button(menu.icon_name())
                            .on_press(Msg::TogglePopup(*id))
                            .into()
                    })
                    .collect(),
            )
            .into(),
            SurfaceIdWrapper::Popup(_) => match self.open_menu {
                Some(id) => match self.menus.get(&id) {
                    Some(menu) => self
                        .applet_helper
                        .popup_container(
                            menu.popup_view().map(move |msg| Msg::StatusMenu((id, msg))),
                        )
                        .into(),
                    None => unreachable!(),
                },
                None => iced::widget::text("").into(),
            },
            SurfaceIdWrapper::LayerSurface(_) => unreachable!(),
        }
    }

    fn close_requested(&self, surface: SurfaceIdWrapper) -> Msg {
        Msg::Closed(surface)
    }
}

pub fn main() -> iced::Result {
    let helper = CosmicAppletHelper::default();
    App::run(Settings {
        initial_surface: InitialSurface::XdgWindow(SctkWindowSettings {
            iced_settings: cosmic::iced_native::window::Settings {
                size: (1, 1),
                ..Default::default()
            },
            ..Default::default()
        }),
        ..helper.window_settings()
    })
}
