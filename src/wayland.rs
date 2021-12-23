use gdk4_wayland::prelude::*;
use gtk4::{
    cairo, gdk,
    glib::{
        self,
        subclass::prelude::*,
        translate::{FromGlibPtrFull, ToGlibPtr},
    },
    gsk::{self, traits::RendererExt},
    prelude::*,
    subclass::prelude::*,
};
use std::{cell::RefCell, os::raw::c_int, ptr, rc::Rc};
use wayland_client::{
    event_enum,
    protocol::{wl_display, wl_output},
    Attached, Filter, GlobalManager, Main,
};
use wayland_protocols::wlr::unstable::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{self, Layer},
    zwlr_layer_surface_v1::{self, Anchor, KeyboardInteractivity},
};

use crate::deref_cell::DerefCell;

event_enum!(
    Events |
    LayerSurface => zwlr_layer_surface_v1::ZwlrLayerSurfaceV1
);

// XXX possibly won't be able to use GtkWindow for wayland.
// - How to detect whether or not to use Wayland? I guess gdk4::Display::default() downcasting
//   * Returns None if no default display
pub fn get_window_wayland<T: IsA<gtk4::Window>>(
    window: &T,
) -> Option<(gdk4_wayland::WaylandDisplay, gdk4_wayland::WaylandSurface)> {
    let surface = window
        .upcast_ref()
        .surface()?
        .downcast::<gdk4_wayland::WaylandSurface>()
        .ok()?;
    let display = surface
        .display()?
        .downcast::<gdk4_wayland::WaylandDisplay>()
        .ok()?;
    Some((display, surface))
}

struct CosmicWaylandDisplay {
    attached_display: Attached<wl_display::WlDisplay>,
    event_queue: RefCell<wayland_client::EventQueue>,
    wayland_display: wayland_client::Display,
    wlr_layer_shell: Option<Main<zwlr_layer_shell_v1::ZwlrLayerShellV1>>,
}

impl CosmicWaylandDisplay {
    fn for_display(display: &gdk4_wayland::WaylandDisplay) -> Rc<Self> {
        const DATA_KEY: &str = "cosmic-wayland-display";

        // `GdkWaylandDisplay` already associated with a `CosmicWaylandDisplay`
        if let Some(data) = unsafe { display.data::<Rc<Self>>(DATA_KEY) } {
            return unsafe { data.as_ref() }.clone();
        }

        let wayland_display = unsafe {
            wayland_client::Display::from_external_display(
                display.wl_display().as_ref().c_ptr() as *mut _
            )
        }; // XXX?

        let mut event_queue = wayland_display.create_event_queue();
        // XXX: I guess this is wrong, because it can't attach to multiple queues?
        // I guess if wayland-client uses `wl_proxy_create_wrapper` that doesn't happen?
        let attached_display = wayland_display.attach(event_queue.token());
        let globals = GlobalManager::new(&attached_display);

        event_queue.sync_roundtrip(&mut (), |_, _, _| {}).unwrap();

        let wlr_layer_shell = globals
            .instantiate_exact::<zwlr_layer_shell_v1::ZwlrLayerShellV1>(1)
            .ok();

        let wl_seat = globals
            .instantiate_exact::<wayland_client::protocol::wl_seat::WlSeat>(1)
            .ok()
            .unwrap();
        let wl_pointer = wl_seat.get_pointer();

        event_queue.sync_roundtrip(&mut (), |_, _, _| {}).unwrap();

        let cosmic_wayland_display = Rc::new(Self {
            attached_display,
            event_queue: RefCell::new(event_queue),
            wayland_display,
            wlr_layer_shell,
        });

        // XXX should some things here not be freed? attached_display?

        unsafe { display.set_data(DATA_KEY, cosmic_wayland_display.clone()) };

        /*
        // XXX
        glib::idle_add_local(move || {
            wayland_display.flush();
            if let Some(guard) = event_queue.prepare_read() {
                guard.read_events();
            }
            event_queue.dispatch_pending(&mut (), |_, _, _| unreachable!()).unwrap();
            Continue(true)
        });
        */

        cosmic_wayland_display
    }
}

// TODO: store properties, set when mapping?
#[derive(Default)]
pub struct LayerShellWindowInner {
    display: DerefCell<gdk::Display>,
    surface: RefCell<Option<gdk::Surface>>,
    renderer: RefCell<Option<gsk::Renderer>>,
    wlr_layer_surface: RefCell<Option<Main<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>>>, // TODO: set
    constraint_solver: DerefCell<ConstraintSolver>,
    child: RefCell<Option<gtk4::Widget>>,
}

#[glib::object_subclass]
impl ObjectSubclass for LayerShellWindowInner {
    const NAME: &'static str = "S76CosmicLayerShellWindow";
    type ParentType = gtk4::Widget;
    type Interfaces = (gtk4::Native, gtk4::Root);
    type Type = LayerShellWindow;
}

impl ObjectImpl for LayerShellWindowInner {
    fn constructed(&self, obj: &Self::Type) {
        self.display.set(gdk::Display::default().unwrap()); // XXX any issue unwrapping?
        self.constraint_solver.set(glib::Object::new(&[]).unwrap());

        obj.add_css_class("background");
    }
}

fn layer_shell_init(surface: &WaylandCustomSurface, display: &gdk4_wayland::WaylandDisplay) {
    // XXX needed for wl_surface to exist
    unsafe { gdk_wayland_custom_surface_present(surface.to_glib_none().0, 500, 500) };

    // XXX
    let output = None;
    let layer = Layer::Top;
    let namespace = String::new();

    let wl_surface = surface.wl_surface();

    let cosmic_wayland_display = CosmicWaylandDisplay::for_display(display);
    let wlr_layer_shell = match cosmic_wayland_display.wlr_layer_shell.as_ref() {
        Some(wlr_layer_shell) => wlr_layer_shell,
        None => {
            eprintln!("Error: Layer shell not supported by compositor");
            return;
        }
    };

    let wlr_layer_surface =
        wlr_layer_shell.get_layer_surface(&wl_surface, output, layer, namespace);

    let filter = Filter::new(|event, _, _| match event {
        Events::LayerSurface { event, object } => match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width: _,
                height: _,
            } => {
                println!("ack_configure");
                object.ack_configure(serial);
            }
            zwlr_layer_surface_v1::Event::Closed => {}
            _ => {}
        },
    });
    wlr_layer_surface.assign(filter);

    wl_surface.commit(); // Hm...

    let x = cosmic_wayland_display
        .event_queue
        .borrow_mut()
        .sync_roundtrip(&mut (), |_, _, _| {})
        .unwrap();

    // XXX
}

impl WidgetImpl for LayerShellWindowInner {
    fn realize(&self, widget: &Self::Type) {
        let surface = WaylandCustomSurface::new(&*self.display); // TODO
        let display = self
            .display
            .downcast_ref::<gdk4_wayland::WaylandDisplay>()
            .unwrap();
        layer_shell_init(&surface, display);
        let surface = surface.upcast::<gdk::Surface>();

        //let surface = gdk::Surface::new_toplevel(&*self.display); // TODO: change surface type
        let widget_ptr: *mut Self::Instance = widget.to_glib_none().0;
        unsafe { gdk_surface_set_widget(surface.to_glib_none().0, widget_ptr as *mut _) };
        *self.surface.borrow_mut() = Some(surface.clone());
        surface.connect_render(move |surface, region| {
            println!("RENDER");
            unsafe {
                gtk_widget_render(
                    widget_ptr as *mut _,
                    surface.to_glib_none().0,
                    region.to_glib_none().0,
                )
            };
            true
        });
        surface.connect_event(|_, event| {
            //unsafe { gtk_main_do_event(event.to_glib_none().0) };
            true
        });
        /*
        let toplevel = surface.downcast_ref::<gdk::Toplevel>().unwrap(); // XXX
        toplevel.connect_compute_size(move |toplevel, size| {
            // XXX
            size.set_min_size(500, 500);
            size.set_size(500, 500);
            unsafe { gtk_widget_ensure_resize(widget_ptr as *mut _); }
        });
        */
        // XXX
        unsafe {
            gtk_widget_ensure_resize(widget_ptr as *mut _);
        }

        self.parent_realize(widget);

        *self.renderer.borrow_mut() = Some(gsk::Renderer::for_surface(&surface).unwrap()); // XXX unwrap?
                                                                                           // XXX

        unsafe { gtk4::ffi::gtk_native_realize(widget_ptr as *mut _) };
    }

    fn unrealize(&self, widget: &Self::Type) {
        let widget_ptr: *mut Self::Instance = widget.to_glib_none().0;

        unsafe { gtk4::ffi::gtk_native_unrealize(widget_ptr as *mut _) };

        self.parent_unrealize(widget);

        if let Some(renderer) = self.renderer.borrow_mut().take() {
            renderer.unrealize();
        }

        if let Some(surface) = self.surface.borrow().as_ref() {
            unsafe { gdk_surface_set_widget(surface.to_glib_none().0, ptr::null_mut()) };
        }
        // XXX
    }

    fn map(&self, widget: &Self::Type) {
        // TODO: what does `gtk_drag_icon_move_resize` do?

        if let Some(surface) = self.surface.borrow().as_ref() {
            /*
            let layout = gdk::ToplevelLayout::new(); // XXX?
            surface
                .downcast_ref::<gdk::Toplevel>()
                .unwrap()
                .present(&layout); // TODO not toplevel
            */
        }

        self.parent_map(widget);

        if let Some(child) = self.child.borrow().as_ref() {
            child.map();
        }

        // XXX
    }

    fn unmap(&self, widget: &Self::Type) {
        self.parent_unmap(widget);

        if let Some(surface) = self.surface.borrow().as_ref() {
            surface.hide();
        }

        if let Some(child) = self.child.borrow().as_ref() {
            child.unmap();
        }
        // XXX
    }

    fn measure(
        &self,
        widget: &Self::Type,
        orientation: gtk4::Orientation,
        for_size: i32,
    ) -> (i32, i32, i32, i32) {
        if let Some(child) = self.child.borrow().as_ref() {
            child.measure(orientation, for_size)
        } else {
            (0, 0, 0, 0)
        }
    }

    fn size_allocate(&self, widget: &Self::Type, width: i32, height: i32, baseline: i32) {
        if let Some(child) = self.child.borrow().as_ref() {
            child.allocate(width, height, baseline, None)
        }
    }

    fn show(&self, widget: &Self::Type) {
        let widget_ptr: *mut Self::Instance = widget.to_glib_none().0;
        // XXX unsafe { _gtk_widget_set_visible_flag(widget_ptr as *mut _, 1) };
        // TODO? gtk_css_node_validate
        widget.realize();
        if let Some(surface) = self.surface.borrow().as_ref() {
            /*
            let layout = gdk::ToplevelLayout::new(); // XXX?
            surface
                .downcast_ref::<gdk::Toplevel>()
                .unwrap()
                .present(&layout); // TODO not toplevel
            */
        }
        self.parent_show(widget);
        widget.map();
    }

    fn hide(&self, widget: &Self::Type) {
        //let widget_ptr: *mut Self::Instance = widget.to_glib_none().0;
        // XXX unsafe { _gtk_widget_set_visible_flag(widget_ptr as *mut _, 0) };
        self.parent_hide(widget);
        widget.unmap();
    }
}

// TODO: Move into gtk4-rs when support merged/released in gtk
unsafe impl IsImplementable<LayerShellWindowInner> for gtk4::Native {
    fn interface_init(iface: &mut glib::Interface<Self>) {
        let iface = unsafe { &mut *(iface as *mut _ as *mut GtkNativeInterface) };
        iface.get_surface = Some(get_surface);
        iface.get_renderer = Some(get_renderer);
        iface.get_surface_transform = Some(get_surface_transform);
        iface.layout = Some(layout);
    }

    fn instance_init(_instance: &mut glib::subclass::InitializingObject<LayerShellWindowInner>) {}
}

// TODO: Move into gtk4-rs when support merged/released in gtk
unsafe impl IsImplementable<LayerShellWindowInner> for gtk4::Root {
    fn interface_init(iface: &mut glib::Interface<Self>) {
        let iface = unsafe { &mut *(iface as *mut _ as *mut GtkRootInterface) };
        iface.get_display = Some(get_display);
        iface.get_constraint_solver = Some(get_constraint_solver);
        // XXX?
    }

    fn instance_init(_instance: &mut glib::subclass::InitializingObject<LayerShellWindowInner>) {}
}

glib::wrapper! {
    pub struct LayerShellWindow(ObjectSubclass<LayerShellWindowInner>)
        @extends gtk4::Widget, @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}
// TODO handle configure/destroy
// TODO presumably call destroy() when appropriate?
// What do wayland-client types do when associated connection is gone? Panic? UB?
impl LayerShellWindow {
    pub fn new() -> Self {
        glib::Object::new(&[]).unwrap()
    }

    fn inner(&self) -> &LayerShellWindowInner {
        LayerShellWindowInner::from_instance(self)
    }

    pub fn set_child<T: IsA<gtk4::Widget>>(&self, w: Option<&T>) {
        let mut child = self.inner().child.borrow_mut();
        if let Some(child) = child.take() {
            child.unparent();
        }
        if let Some(w) = w {
            w.set_parent(self);
        }
        *child = w.map(|x| x.clone().upcast());
    }

    fn get_popup(&self, popup: &gdk4_wayland::WaylandPopup) {
        if let Some(wlr_layer_surface) = self.inner().wlr_layer_surface.borrow().as_ref() {
            // xdg_popup = popup.get_xdg_popup(); XXX
            //wlr_layer_surface.get_popup(xdg_popup);
        }
    }

    pub fn set_size(&self, width: u32, height: u32) {
        if let Some(wlr_layer_surface) = self.inner().wlr_layer_surface.borrow().as_ref() {
            wlr_layer_surface.set_size(width, height);
        };
    }

    pub fn set_anchor(&self, anchor: Anchor) {
        if let Some(wlr_layer_surface) = self.inner().wlr_layer_surface.borrow().as_ref() {
            wlr_layer_surface.set_anchor(anchor);
        };
    }

    pub fn set_exclusive_zone(&self, zone: i32) {
        if let Some(wlr_layer_surface) = self.inner().wlr_layer_surface.borrow().as_ref() {
            wlr_layer_surface.set_exclusive_zone(zone);
        };
    }

    pub fn set_margin(&self, top: i32, right: i32, bottom: i32, left: i32) {
        if let Some(wlr_layer_surface) = self.inner().wlr_layer_surface.borrow().as_ref() {
            wlr_layer_surface.set_margin(top, right, bottom, left);
        };
    }

    pub fn set_keyboard_interactivity(&self, interactivity: KeyboardInteractivity) {
        if let Some(wlr_layer_surface) = self.inner().wlr_layer_surface.borrow().as_ref() {
            wlr_layer_surface.set_keyboard_interactivity(interactivity);
        };
    }

    pub fn set_layer(&self, layer: Layer) {
        if let Some(wlr_layer_surface) = self.inner().wlr_layer_surface.borrow().as_ref() {
            wlr_layer_surface.set_layer(layer);
        };
    }
}

// Comment on gtk-layer-shell: Since the API would be different, and I'd need to wrap the C library
// to use in my project, and it shouldn't require to much code, re-implemented to test this

// TODO: where is GtkRoot used?

pub struct GtkConstraintSolver {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GdkWaylandCustomSurface {
    _private: [u8; 0],
}

// XXX needs to be public
#[link(name = "gtk-4")]
extern "C" {
    pub fn gtk_constraint_solver_get_type() -> glib::ffi::GType;

    pub fn gdk_surface_set_widget(surface: *mut gdk::ffi::GdkSurface, widget: glib::ffi::gpointer);

    pub fn _gtk_widget_set_visible_flag(
        widget: *mut gtk4::ffi::GtkWidget,
        visible: glib::ffi::gboolean,
    );

    pub fn gtk_widget_render(
        widget: *mut gtk4::ffi::GtkWidget,
        surface: *mut gdk::ffi::GdkSurface,
        region: *const cairo::ffi::cairo_region_t,
    );

    pub fn gtk_widget_ensure_resize(widget: *mut gtk4::ffi::GtkWidget);

    pub fn gtk_main_do_event(event: *mut gdk::ffi::GdkEvent);

    pub fn _gdk_frame_clock_idle_new() -> *mut gdk::ffi::GdkFrameClock;

    // Added API
    pub fn gdk_wayland_custom_surface_get_type() -> glib::ffi::GType;

    pub fn gdk_wayland_custom_surface_present(
        surface: *mut GdkWaylandCustomSurface,
        width: c_int,
        height: c_int,
    ) -> glib::ffi::gboolean;
}

// XXX needs to be public in gtk
glib::wrapper! {
    pub struct ConstraintSolver(Object<GtkConstraintSolver>);

    match fn {
        type_ => || gtk_constraint_solver_get_type(),
    }
}

pub struct GtkNativeInterface {
    pub g_iface: gobject_sys::GTypeInterface,
    pub get_surface:
        Option<unsafe extern "C" fn(self_: *mut gtk4::ffi::GtkNative) -> *mut gdk::ffi::GdkSurface>,
    pub get_renderer: Option<
        unsafe extern "C" fn(self_: *mut gtk4::ffi::GtkNative) -> *mut gsk::ffi::GskRenderer,
    >,
    pub get_surface_transform:
        Option<unsafe extern "C" fn(self_: *mut gtk4::ffi::GtkNative, x: *mut f64, y: *mut f64)>,
    pub layout:
        Option<unsafe extern "C" fn(self_: *mut gtk4::ffi::GtkNative, width: c_int, height: c_int)>,
}

pub struct GtkRootInterface {
    pub g_iface: gobject_sys::GTypeInterface,
    pub get_display:
        Option<unsafe extern "C" fn(self_: *mut gtk4::ffi::GtkRoot) -> *mut gdk::ffi::GdkDisplay>,
    pub get_constraint_solver:
        Option<unsafe extern "C" fn(self_: *mut gtk4::ffi::GtkRoot) -> *mut GtkConstraintSolver>,
    pub get_focus:
        Option<unsafe extern "C" fn(self_: *mut gtk4::ffi::GtkRoot) -> *mut gtk4::ffi::GtkWidget>,
    pub set_focus: Option<
        unsafe extern "C" fn(self_: *mut gtk4::ffi::GtkRoot, focus: *mut gtk4::ffi::GtkWidget),
    >,
}

unsafe extern "C" fn get_surface(native: *mut gtk4::ffi::GtkNative) -> *mut gdk::ffi::GdkSurface {
    let instance = &*(native as *mut <LayerShellWindowInner as ObjectSubclass>::Instance);
    let imp = instance.impl_();
    imp.surface
        .borrow()
        .as_ref()
        .map_or(ptr::null_mut(), |x| x.to_glib_none().0)
}

unsafe extern "C" fn get_renderer(native: *mut gtk4::ffi::GtkNative) -> *mut gsk::ffi::GskRenderer {
    let instance = &*(native as *mut <LayerShellWindowInner as ObjectSubclass>::Instance);
    let imp = instance.impl_();
    imp.renderer
        .borrow()
        .as_ref()
        .map_or(ptr::null_mut(), |x| x.to_glib_none().0)
}

unsafe extern "C" fn get_surface_transform(
    native: *mut gtk4::ffi::GtkNative,
    x: *mut f64,
    y: *mut f64,
) {
    // XXX

    /*
    let mut css_boxes = gtk4::ffi::GtkCssBoxes;
    gtk4::ffi::gtk_css_boxes_init(&mut css_boxes, native as *mut _);

    let margin_rect = gtk4::ffi::gtk_css_boxes_get_margin_rect(&mut css_boxes);

    *x = - (*margin_rect).origin.x;
    *y = - (*margin_rect).origin.y;
    */

    *x = 0.;
    *y = 0.;
}

unsafe extern "C" fn layout(native: *mut gtk4::ffi::GtkNative, width: c_int, height: c_int) {
    // XXX
    gtk4::ffi::gtk_widget_allocate(native as *mut _, width, height, -1, ptr::null_mut());
}

unsafe extern "C" fn get_display(root: *mut gtk4::ffi::GtkRoot) -> *mut gdk::ffi::GdkDisplay {
    let instance = &*(root as *mut <LayerShellWindowInner as ObjectSubclass>::Instance);
    let imp = instance.impl_();
    imp.display.to_glib_none().0
}

unsafe extern "C" fn get_constraint_solver(
    root: *mut gtk4::ffi::GtkRoot,
) -> *mut GtkConstraintSolver {
    let instance = &*(root as *mut <LayerShellWindowInner as ObjectSubclass>::Instance);
    let imp = instance.impl_();
    imp.constraint_solver.to_glib_none().0
}

glib::wrapper! {
    pub struct WaylandCustomSurface(Object<GdkWaylandCustomSurface>)
        @extends gdk4_wayland::WaylandSurface, @implements gdk::Surface;

    match fn {
        type_ => || gdk_wayland_custom_surface_get_type(),
    }
}

impl WaylandCustomSurface {
    pub fn new(display: &gdk::Display) -> Self {
        let frame_clock = unsafe { gdk::FrameClock::from_glib_full(_gdk_frame_clock_idle_new()) };
        glib::Object::new(&[("display", display), ("frame-clock", &frame_clock)]).unwrap()
    }
}
