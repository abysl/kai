use super::*;

#[derive(Message)]
pub enum Command {
    Request { actions: u32, revision: u64 },
    Vote { id: u64, accept: bool },
}

pub(super) fn publish_result(
    session: &mut HostSession,
    conns: &Conns,
    info: &mut SessionInfo,
    result: Result<bool, SessionError>,
    requester: Option<u64>,
) -> bool {
    match result {
        Ok(true) => {
            for (&conn, &seat) in &conns.seats {
                send_to(
                    conn,
                    HostMsg::RolledBack {
                        next_seq: session.state().next_seq,
                        faces: session.faces_owed_to(seat),
                    },
                );
            }
            info.status = "rollback accepted".into();
            info.undo_generation += 1;
            true
        }
        Ok(false) => {
            if session.undo_status().proposal.is_none() {
                let text = "undo request declined".to_string();
                info.status = text.clone();
                for &conn in conns.seats.keys() {
                    send_to(conn, HostMsg::Notice { text: text.clone() });
                }
            }
            false
        }
        Err(error) => {
            if let Some(conn) = requester {
                send_to(
                    conn,
                    HostMsg::Notice {
                        text: format!("undo refused: {error}"),
                    },
                );
            } else {
                info.notices.push(format!("undo refused: {error}"));
            }
            if error.is_engine_fault() {
                session_failed(info, "undo", &error);
            }
            false
        }
    }
}

pub fn route(
    mut commands: MessageReader<Command>,
    mut host: ResMut<HostState>,
    mut info: ResMut<SessionInfo>,
    mut table: ResMut<GameTable>,
    mut mirror: ResMut<Mirror>,
    mut generation: ResMut<DealGeneration>,
    mut seen_rollback: Local<u64>,
    mut secrets: ResMut<crate::table::plugin_ui::RollSecrets>,
    mut history: ResMut<crate::table::history::History>,
) {
    for command in commands.read() {
        let msg = match *command {
            Command::Request { actions, revision } => ClientMsg::RequestUndo { actions, revision },
            Command::Vote { id, accept } => ClientMsg::VoteUndo { id, accept },
        };
        match info.role {
            SessionRole::Host => {
                let HostState { session, conns, .. } = &mut *host;
                let Some(session) = session.as_mut() else {
                    continue;
                };
                let result = match msg {
                    ClientMsg::RequestUndo { actions, revision } => {
                        session.request_undo(0, actions, revision)
                    }
                    ClientMsg::VoteUndo { id, accept } => session.vote_undo(0, id, accept),
                    _ => unreachable!(),
                };
                if publish_result(session, conns, &mut info, result, None) {
                    refresh(&mut table, &mut mirror, session.table(), session.view());
                }
            }
            SessionRole::Client => bridge::send_to_host(msg),
            _ => {}
        }
    }
    if info.role == SessionRole::Host {
        if let Some(session) = host.session.as_ref() {
            let status = session.undo_status();
            if host.undo_sent.as_ref() != Some(&status) {
                for &conn in host.conns.seats.keys() {
                    send_to(
                        conn,
                        HostMsg::Undo {
                            status: status.clone(),
                        },
                    );
                }
                host.undo_sent = Some(status.clone());
            }
            if info.undo != status {
                info.undo = status;
            }
        }
    } else {
        host.undo_sent = None;
        if info.role != SessionRole::Client && info.role != SessionRole::Joining {
            if info.undo != Default::default() {
                info.undo = Default::default();
            }
        }
    }
    if *seen_rollback != info.undo_generation {
        *seen_rollback = info.undo_generation;
        generation.0 += 1;
        secrets.rearm();
        history.clear();
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn undo_peer_votes_are_bound_to_the_authenticated_connection_seat() {
        let mut session = HostSession::new("host");
        session.join("guest").unwrap();
        session
            .deal(0, vec![agni_core::CardFace::named("card")])
            .unwrap();
        let before = session.state().clone();
        session
            .intent(
                0,
                WireIntent::Move {
                    card: 0,
                    to: Zone::Board,
                    seat: 0,
                    index: 0,
                },
            )
            .unwrap();
        session
            .request_undo(0, 1, session.undo_status().revision)
            .unwrap();
        let id = session.undo_status().proposal.unwrap().id;
        let mut host = HostState::hosted(session);
        host.conns.seats.insert(7, 1);
        let mut info = SessionInfo {
            role: SessionRole::Host,
            ..Default::default()
        };
        assert!(!host.peer_message(&mut info, 9, ClientMsg::VoteUndo { id, accept: true }));
        assert_ne!(host.session().state(), &before);
        assert!(host.peer_message(&mut info, 7, ClientMsg::VoteUndo { id, accept: true }));
        assert_eq!(host.session().state(), &before);
        assert_eq!(info.undo_generation, 1);
        assert!(!host.peer_message(&mut info, 7, ClientMsg::VoteUndo { id, accept: true }));
    }

    #[test]
    fn undo_queued_during_module_loading_replays_in_order() {
        let mut host = HostSession::new("host");
        host.join("guest").unwrap();
        host.deal(0, vec![agni_core::CardFace::named("card")])
            .unwrap();
        let mut pending = PendingWelcome::new(1, host.roster(), host.log().to_vec());
        for entry in host
            .intent(
                0,
                WireIntent::Move {
                    card: 0,
                    to: Zone::Board,
                    seat: 0,
                    index: 0,
                },
            )
            .unwrap()
        {
            assert!(pending.absorb(HostMsg::Entry { entry }).is_none());
        }
        host.request_undo(0, 1, host.undo_status().revision)
            .unwrap();
        let id = host.undo_status().proposal.unwrap().id;
        host.vote_undo(1, id, true).unwrap();
        assert!(pending
            .absorb(HostMsg::RolledBack {
                next_seq: host.state().next_seq,
                faces: host.faces_owed_to(1)
            })
            .is_none());
        let (client, _) = pending
            .seat_replica(Box::new(agni_sim::engine::NativeEngine::new()), None)
            .unwrap();
        assert_eq!(client.state(), host.state());
        assert!(client.table().cards()[0].face.is_hidden());
    }
}
