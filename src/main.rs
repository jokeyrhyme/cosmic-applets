use gtk4::prelude::*;

mod application;
mod deref_cell;
mod mpris;
mod mpris_player;
mod notification_list;
mod notification_popover;
mod notification_widget;
mod notifications;
mod popover_container;
mod status_area;
mod status_menu;
mod status_notifier_watcher;
mod time_button;
mod wayland;
mod window;
mod x;

use application::PanelApp;

fn main() {
    //PanelApp::new().run();
    gtk4::init().unwrap();
    let window = wayland::LayerShellWindow::new();
    //let window = gtk4::Window::new();
    //let label = gtk4::Label::new(Some("foo"));
    //label.set_size_request(500, 500);
    //window.set_child(Some(&label));
    window.set_child(Some(&window::window_box()));
    window.realize();
    window.set_anchor(wayland::Anchor::Top | wayland::Anchor::Left); // TODO: how to handle centering?
                                                                     //window.set_exclusive_zone(40);
    window.show();
    gtk4::glib::MainLoop::new(None, false).run();
}
