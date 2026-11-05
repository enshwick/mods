//! The probe's chat: a filter that drops a line of exactly [`DROPPED`], and a
//! log line for every line that reached the chat, without its text.

use enshwick::chat::ChatVerdict;
use enshwick::{Load, Off};

/// The one line the filter drops; any other line passes as typed.
pub(crate) const DROPPED: &str = "drop me";

/// What the filter does with `text`.
pub(crate) fn verdict(text: &str) -> ChatVerdict {
    if text == DROPPED {
        ChatVerdict::Drop
    } else {
        ChatVerdict::Keep
    }
}

/// Registers the filter and the log of lines heard, each logged as `PASS`
/// or `FAIL`.
pub(crate) fn register(load: &mut Load<'_>) {
    let filter = load.on_chat(|line| {
        let verdict = verdict(&line.text);
        if verdict == ChatVerdict::Drop {
            let sender = line.player.map(|key| format!("{key:016x}"));
            enshwick::info!(
                "chat filter dropped a line from {}",
                sender.as_deref().unwrap_or("none")
            );
        }
        verdict
    });
    log("chat filter registered: it drops \"drop me\"", filter);
    let said = load.on_chat_said(|line| {
        let sender = line.player.map(|key| format!("{key:016x}"));
        enshwick::info!(
            "chat said: sender {}, {:?}, {} bytes",
            sender.as_deref().unwrap_or("none"),
            line.kind,
            line.text.len()
        );
    });
    log("chat lines heard", said);
}

fn log(what: &str, result: Result<(), Off>) {
    match result {
        Ok(()) => enshwick::info!("PASS {what}"),
        Err(off) => enshwick::warn!("FAIL {what}: {off}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_line_of_exactly_drop_me_is_dropped() {
        assert_eq!(verdict(DROPPED), ChatVerdict::Drop);
        for kept in ["drop me please", "Drop me", " drop me", "hello"] {
            assert_eq!(verdict(kept), ChatVerdict::Keep, "{kept}");
        }
    }
}
