//! Toast notifications - the only visible feedback for the background daemon
//! (no window on screen) and a nice-to-have for the launch-triggered path.
//! Fire-and-forget: a failure to show a notification (no notification daemon
//! present, etc.) must never affect syncing itself, so every call here only
//! ever logs on error, never propagates one.

use crate::log;
use notify_rust::Notification;

pub fn pulled(game: &str) {
    show(&format!("{game} updated"), "Pulled a newer save from another PC.");
}

pub fn pushed(game: &str) {
    show(&format!("{game} backed up"), "Local changes pushed to your sync folder.");
}

pub fn restore_failed(game: &str, reason: &str) {
    show(&format!("{game}: restore failed"), reason);
}

pub fn backup_failed(game: &str, reason: &str) {
    show(&format!("{game}: backup failed"), reason);
}

pub fn info(summary: &str, body: &str) {
    show(summary, body);
}

fn show(summary: &str, body: &str) {
    log::line(&format!("notify: {summary} - {body}"));
    if let Err(e) = Notification::new().summary(summary).body(body).appname("Nimbus").show() {
        log::line(&format!("notify: failed to show notification: {e}"));
    }
}
