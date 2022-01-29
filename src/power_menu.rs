// TODO use upower

use cascade::cascade;
use gtk4::{
    glib::{self, clone},
    pango,
    prelude::*,
    subclass::prelude::*,
};

use crate::deref_cell::DerefCell;
use crate::popover_container::PopoverContainer;

#[derive(Default)]
pub struct PowerMenuInner {
    button: DerefCell<gtk4::ToggleButton>,
}

#[glib::object_subclass]
impl ObjectSubclass for PowerMenuInner {
    const NAME: &'static str = "S76PowerMenu";
    type ParentType = gtk4::Widget;
    type Type = PowerMenu;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BinLayout>();
    }
}

impl ObjectImpl for PowerMenuInner {
    fn constructed(&self, obj: &PowerMenu) {
        let label = cascade! {
            gtk4::Label::new(None);
            ..set_attributes(Some(&cascade! {
                pango::AttrList::new();
                ..insert(pango::Attribute::new_weight(pango::Weight::Bold));
            }));
        };

        let button = cascade! {
            gtk4::ToggleButton::new();
            ..set_has_frame(false);
            ..set_icon_name("ac-adapter-symbolic"); // TODO: update depending on battery state
            ..set_child(Some(&label));
        };

        cascade! {
            PopoverContainer::new(&button);
            ..set_parent(obj);
            ..popover().bind_property("visible", &button, "active").flags(glib::BindingFlags::BIDIRECTIONAL).build();
        };
    }

    fn dispose(&self, _obj: &PowerMenu) {
        self.button.unparent();
    }
}

impl WidgetImpl for PowerMenuInner {}

glib::wrapper! {
    pub struct PowerMenu(ObjectSubclass<PowerMenuInner>)
        @extends gtk4::Widget;
}

impl PowerMenu {
    pub fn new() -> Self {
        let obj = glib::Object::new::<Self>(&[]).unwrap();
        obj
    }

    fn inner(&self) -> &PowerMenuInner {
        PowerMenuInner::from_instance(self)
    }
}
