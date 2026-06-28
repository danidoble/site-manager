use libadwaita::prelude::AdwDialogExt;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct AppInfo {
    name: String,
    version: String,
    author: String,
    license: String,
}

const APP_INFO_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/app-info.json"
));

pub fn present(parent: &libadwaita::ApplicationWindow) {
    let info = serde_json::from_str::<AppInfo>(APP_INFO_JSON).unwrap_or_else(|_| AppInfo {
        name: "Local Site Manager".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        author: "Local Site Manager contributors".into(),
        license: "MIT".into(),
    });

    let dialog = libadwaita::AboutDialog::builder()
        .application_name(&info.name)
        .application_icon("local-site-manager")
        .version(&info.version)
        .developer_name(&info.author)
        .license_type(gtk4::License::MitX11)
        .comments("Native GNOME app for local development sites.")
        .build();
    dialog.set_license(&info.license);
    dialog.present(Some(parent));
}
