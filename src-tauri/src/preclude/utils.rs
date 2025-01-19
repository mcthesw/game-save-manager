use log::error;

pub fn show_notification<T1: AsRef<str>, T2: AsRef<str>>(title: T1, body: T2) {
    if let Err(e) = notify_rust::Notification::new()
        .summary(title.as_ref())
        .body(body.as_ref())
        .timeout(6000) // milliseconds
        .show()
    {
        error!(target:"rgsm::quick_action", "Failed to show notification: {}", e);
    }
}
