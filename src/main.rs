mod alerts;
mod cache;
mod diagnostics;
mod domain;
mod formatting;
mod macos_notifications;
mod menu_bar;
mod provider;
mod refresh;
mod settings;
mod storage;
mod tencent_quote;

fn main() {
    menu_bar::run();
}
