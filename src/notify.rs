use std::process::Command;

#[derive(Debug, Clone, Copy)]
pub struct NotifySettings {
    pub enabled: bool,
    pub errors_only: bool,
    pub min_ms: u64,
}

pub fn notify_completion(success: bool, model: &str, last_response_ms: Option<u64>) {
    let title = if success { "kode: response complete" } else { "kode: response failed" };
    let body = match last_response_ms {
        Some(ms) if ms > 0 => format!("model: {model} · {ms}ms"),
        _ => format!("model: {model}"),
    };
    let delivered = notify_native(title, &body);
    if !delivered {
        eprint!("\x07");
    }
}

pub fn should_notify(settings: NotifySettings, success: bool, last_response_ms: Option<u64>) -> bool {
    if !settings.enabled {
        return false;
    }
    if settings.errors_only && success {
        return false;
    }
    if settings.min_ms > 0 && success {
        let ms = last_response_ms.unwrap_or(0);
        if ms < settings.min_ms {
            return false;
        }
    }
    true
}

fn notify_native(title: &str, body: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        return Command::new("notify-send")
            .arg(title)
            .arg(body)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }

    #[cfg(target_os = "macos")]
    {
        let script = format!("display notification {:?} with title {:?}", body, title);
        return Command::new("osascript")
            .arg("-e")
            .arg(script)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }

    #[cfg(target_os = "windows")]
    {
        let escaped = body.replace('\'', "''");
        let ps = format!(
            "[void][System.Reflection.Assembly]::LoadWithPartialName('System.Windows.Forms'); \
             [System.Windows.Forms.MessageBox]::Show('{escaped}', '{title}')"
        );
        return Command::new("powershell")
            .arg("-NoProfile")
            .arg("-Command")
            .arg(ps)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }

    #[allow(unreachable_code)]
    false
}
