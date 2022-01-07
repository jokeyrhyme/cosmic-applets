use gtk4::glib::{self, clone};

pub async fn create<
    F: Fn(zbus::ConnectionBuilder<'static>) -> zbus::Result<zbus::ConnectionBuilder<'static>>,
>(
    well_known_name: &'static str,
    serve_cb: F,
) -> zbus::Result<zbus::Connection> {
    use futures::prelude::*;
    use std::cell::Cell;
    use zbus::fdo::{DBusProxy, RequestNameFlags};
    use zbus_names::WellKnownName;

    let well_known_name = WellKnownName::try_from(well_known_name)?;

    let connection = serve_cb(zbus::ConnectionBuilder::session()?)?
        .build()
        .await?;
    let dbus_proxy = DBusProxy::new(&connection).await?;
    let mut name_owner_changed_stream = dbus_proxy.receive_name_owner_changed().await?;

    glib::MainContext::default().spawn_local(clone!(@strong connection => async move {
        let have_bus_name = Cell::new(false);
        let request_name = || async {
            let flags = RequestNameFlags::AllowReplacement.into();
            if let Err(err) = dbus_proxy.request_name(well_known_name.as_ref(), flags).await {
                eprintln!("Failed to claim bus name '{}': {}", well_known_name, err);
            } else {
                eprintln!("Acquired bus name: {}", well_known_name);
                have_bus_name.set(true);
            }
        };
        request_name().await;
        let unique_name = connection.unique_name().map(|x| x.as_ref());
        if let Some(evt) = name_owner_changed_stream.next().await {
            let args = evt.args().unwrap();
            if args.name().as_ref() == well_known_name {
                if args.new_owner().as_ref() != unique_name.as_ref() && have_bus_name.get() {
                    eprintln!("Lost bus name: {}", well_known_name);
                    have_bus_name.set(false);
                }

                if args.new_owner().is_none() {
                    request_name().await;
                }
            }
        }
    }));

    Ok(connection)
}
