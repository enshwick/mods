//! The client's own calls: unavailable on a dedicated server, answered on a
//! client that hosts, where a command is queued and cancelled before any
//! frame runs it; and the loader's list of what it turned off, each entry
//! with its reason and time.

use enshwick::abi::Side;
use enshwick::input::{ClientCommand, CommandAction, CommandState, Input};
use enshwick::{Load, Off, WorldError};

use crate::checks::{fail, pass};

/// The client control checks.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ClientChecks {
    input: Input,
    dedicated: bool,
}

impl ClientChecks {
    pub(crate) fn new(load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(Self {
            input: load.input()?,
            dedicated: load.host().build().side == Side::SERVER,
        })
    }

    /// Runs the checks on the game thread.
    pub(crate) fn run(self) {
        if self.dedicated {
            unavailable("client snapshot", self.input.client_snapshot());
            unavailable("client sampler status", self.input.local_status());
            // Never queued on a dedicated server, so it cannot move anyone.
            let walk = CommandAction::Move {
                direction: [0.0, 1.0],
                hold_ms: 250,
            };
            unavailable(
                "client command",
                self.input.client_command(&ClientCommand::new(walk, 1)),
            );
            return;
        }
        answered("client sampler status", self.input.local_status());
        match self.input.client_snapshot() {
            Ok(snapshot) => {
                pass(&format!("client snapshot: {snapshot:?}"));
                self.cancelled_command(snapshot.session());
            }
            Err(error) => fail(&format!("client snapshot: {error:?} on a client")),
        }
    }

    /// Queues a still look and cancels it before a frame takes it, so the
    /// player's camera and character stay where they are.
    fn cancelled_command(self, session: u64) {
        let still = CommandAction::Look {
            yaw: 0.0,
            pitch: 0.0,
        };
        let id = match self
            .input
            .client_command(&ClientCommand::new(still, session))
        {
            Ok(id) => id,
            Err(error) => return fail(&format!("client command: {error:?} on a client")),
        };
        if let Err(error) = self.input.cancel_command(id) {
            return fail(&format!("client command: cancel {id:?}: {error:?}"));
        }
        match self.input.command_status(id) {
            Ok(status) if matches!(status.state, CommandState::Cancelled(_)) => {
                pass(&format!("client command: {id:?} queued and cancelled"));
            }
            other => fail(&format!("client command: {id:?} after cancel: {other:?}")),
        }
    }
}

/// Checks each part or feature the loader lists off names itself, why and
/// when; it needs nothing but the loader, so it runs on its own.
pub(crate) fn loader_off() {
    let off = enshwick::mods::loader_off();
    match off
        .iter()
        .find(|part| part.name.is_empty() || part.reason.is_empty() || part.at == 0)
    {
        None => pass(&format!(
            "loader off: {} parts and features, each with its reason",
            off.len()
        )),
        Some(part) => fail(&format!(
            "loader off: {part:?} lacks its name, reason or time"
        )),
    }
}

/// Logs `PASS` when a client call answers unavailable, as it does on a
/// dedicated server.
fn unavailable<T: core::fmt::Debug>(what: &str, got: Result<T, WorldError>) {
    match got {
        Err(WorldError::Unavailable) => pass(&format!("{what}: unavailable on a server")),
        other => fail(&format!("{what}: {other:?} on a server")),
    }
}

/// Logs `PASS` with the answer when a client call answers on a client.
fn answered<T: core::fmt::Debug>(what: &str, got: Result<T, WorldError>) {
    match got {
        Ok(answer) => pass(&format!("{what}: {answer:?}")),
        Err(error) => fail(&format!("{what}: {error:?} on a client")),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{ClientSnapshot, Status};
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Mod, Off};

    use super::*;

    static CHECKS: Mutex<Option<ClientChecks>> = Mutex::new(None);

    struct Client;

    impl Mod for Client {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            *CHECKS.lock().unwrap() = Some(ClientChecks::new(load)?);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Client,
        r#"
        id = "probe.client"
        side = "server"
        "#
    );

    fn started(host: FakeHost) -> Installed {
        let loader = host.player_input().install();
        // SAFETY: the three functions are the exports of this module's
        // `declare_mod!(Client)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
        loader
    }

    fn logged(loader: &Installed, prefix: &str) -> bool {
        loader
            .logs()
            .iter()
            .any(|(_, line)| line.starts_with(prefix))
    }

    #[test]
    fn on_a_server_each_client_call_answers_unavailable_and_passes() {
        let loader = started(FakeHost::new());
        CHECKS.lock().unwrap().take().unwrap().run();
        for line in [
            "PASS client snapshot: unavailable on a server",
            "PASS client sampler status: unavailable on a server",
            "PASS client command: unavailable on a server",
        ] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
    }

    #[test]
    fn on_a_hosting_client_the_calls_answer_and_the_command_ends_cancelled() {
        let loader = started(FakeHost::new().side(Side::CLIENT).hosting(true));
        loader.set_client_snapshot(ClientSnapshot {
            state: ClientSnapshot::FRESH,
            session: 1,
            ..ClientSnapshot::EMPTY
        });
        CHECKS.lock().unwrap().take().unwrap().run();
        for line in [
            "PASS client snapshot: ",
            "PASS client sampler status: ",
            "PASS client command: ",
        ] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
        assert!(!logged(&loader, "FAIL"), "{:?}", loader.logs());
    }

    #[test]
    fn each_part_the_loader_lists_off_passes_with_its_reason() {
        let loader = started(FakeHost::new());
        loader.turn_loader_off("screen reads", "this build lacks ui.hud_screen");
        loader_off();
        assert!(
            logged(&loader, "PASS loader off: 1 parts and features"),
            "{:?}",
            loader.logs()
        );
    }
}
