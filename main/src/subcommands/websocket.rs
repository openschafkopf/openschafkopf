// adapted from https://github.com/sdroege/async-tungstenite/blob/master/examples/server.rs

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use crate::util::*;
use crate::game::*;
use crate::rules::*;
use crate::rules::ruleset::{SRuleSet, allowed_rules};

use futures::prelude::*;
use futures::{
    channel::mpsc::{unbounded, UnboundedSender},
    future, pin_mut,
};
use serde::{Serialize, Deserialize};
use std::task::{Context, Poll, Waker};

use async_std::{
    net::{TcpListener, TcpStream},
    task,
};
use async_tungstenite::tungstenite::protocol::Message;
use crate::primitives::*;
use rand::prelude::*;
use itertools::Itertools;

#[derive(Debug, Serialize, Deserialize, Clone)]
enum VGamePhaseGeneric<DealCards, GamePreparations, DetermineRules, Game, GameResult> {
    DealCards(DealCards),
    GamePreparations(GamePreparations),
    DetermineRules(DetermineRules),
    Game(Game),
    GameResult(GameResult),
}

type VGamePhase = VGamePhaseGeneric<
    SDealCards,
    SGamePreparations,
    SDetermineRules,
    SGame,
    SGameResult,
>;
type VGamePhaseActivePlayerInfo<'a> = VGamePhaseGeneric<
    (&'a SDealCards, <SDealCards as TGamePhase>::ActivePlayerInfo),
    (&'a SGamePreparations, <SGamePreparations as TGamePhase>::ActivePlayerInfo),
    (&'a SDetermineRules, <SDetermineRules as TGamePhase>::ActivePlayerInfo),
    (&'a SGame, <SGame as TGamePhase>::ActivePlayerInfo),
    (&'a SGameResult, <SGameResult as TGamePhase>::ActivePlayerInfo),
>;
type SActivelyPlayableRulesIdentifier = String;
#[derive(Debug, Serialize, Deserialize, Clone)]
enum VGameAction {
    Stoss,
    Zugeben(SCard),
}
type VGamePhaseAction = VGamePhaseGeneric<
    /*DealCards announce_doubling*/ /*b_doubling*/bool,
    /*GamePreparations announce_game*/Option<SActivelyPlayableRulesIdentifier>,
    /*DetermineRules*/Option<SActivelyPlayableRulesIdentifier>,
    /*Game*/VGameAction,
    /*GameResult*/(), // TODO? should players be able to "accept" result?
>;

impl VGamePhase {
    fn which_player_can_do_something(&self) -> Option<VGamePhaseActivePlayerInfo> {
        use VGamePhaseGeneric::*;
        fn internal<GamePhase: TGamePhase>(gamephase: &GamePhase) -> Option<(&GamePhase, GamePhase::ActivePlayerInfo)> {
            gamephase.which_player_can_do_something()
                .map(|activeplayerinfo| (gamephase, activeplayerinfo))
        }
        match self {
            DealCards(dealcards) => internal(dealcards).map(DealCards),
            GamePreparations(gamepreparations) => internal(gamepreparations).map(GamePreparations),
            DetermineRules(determinerules) => internal(determinerules).map(DetermineRules),
            Game(game) => internal(game).map(Game),
            GameResult(gameresult) => internal(gameresult).map(GameResult),
        }
    }
}

#[derive(Debug)]
struct STimeoutCmd {
    gamephaseaction: VGamePhaseAction,
    aborthandle: future::AbortHandle,
}

#[derive(Debug)]
struct SPeer {
    sockaddr: SocketAddr,
    txmsg: UnboundedSender<Message>,
    n_money: isize,
}

fn static_ruleset() -> SRuleSet {
    debug_verify!(SRuleSet::from_string(
        r"
        base-price=10
        solo-price=50
        lauf-min=3
        [rufspiel]
        [solo]
        [wenz]
        lauf-min=2
        [stoss]
        max=3
        ",
    )).unwrap()
}

#[derive(Debug, Default)]
struct SActivePeer {
    opeer: Option<SPeer>,
    otimeoutcmd: Option<STimeoutCmd>,
}

#[derive(Default, Debug)]
struct SPeers0 {
    mapepiopeer: EnumMap<EPlayerIndex, SActivePeer>, // active
    vecpeer: Vec<SPeer>, // inactive
}
#[derive(Default, Debug)]
struct SPeers1 {
    ogamephase: Option<VGamePhase>,
    n_stock: isize, // TODO would that be better within VGamePhase?
}
#[derive(Default, Debug)]
struct SPeers(SPeers0, SPeers1);

impl SPeers {
    fn insert(&mut self, self_mutex: Arc<Mutex<Self>>, peer: SPeer) {
        match self.0.mapepiopeer
            .iter_mut()
            .find(|opeer| opeer.opeer.is_none())
        {
            Some(opeer) if self.1.ogamephase.is_none() => {
                assert!(opeer.opeer.is_none());
                *opeer = SActivePeer{
                    opeer: Some(peer),
                    otimeoutcmd: None,
                }
            },
            _ => {
                self.0.vecpeer.push(peer);
            }
        }
        if self.1.ogamephase.is_none()
            && self.0.mapepiopeer
                .iter()
                .all(|opeer| opeer.opeer.is_some())
        {
            self.1.ogamephase = Some(VGamePhase::DealCards(SDealCards::new(
                static_ruleset(),
                self.1.n_stock,
            )));
            self.send_msg(
                self_mutex,
                /*oepi*/None,
                /*ogamephaseaction*/None,
            ); // To trigger game logic. TODO beautify instead of dummy msg.
        }
    }

    fn remove(&mut self, sockaddr: &SocketAddr) {
        for epi in EPlayerIndex::values() {
            if self.0.mapepiopeer[epi].opeer.as_ref().map(|peer| peer.sockaddr)==Some(*sockaddr) {
                self.0.mapepiopeer[epi].opeer = None;
                // TODO should we reset timeout command?
            }
        }
        self.0.vecpeer.retain(|peer| peer.sockaddr!=*sockaddr);
    }
}

impl SPeers0 {
    fn for_each(
        &mut self,
        ostich_current: Option<&SStich>,
        ostich_prev: Option<&SStich>,
        orules: Option<&dyn TRules>,
        f_cards: impl Fn(EPlayerIndex) -> Vec<SCard>,
        mut f_active: impl FnMut(EPlayerIndex, &mut Option<STimeoutCmd>)->VMessage,
        mut f_inactive: impl FnMut(&mut SPeer)->VMessage,
    ) {
        let communicate = |oepi: Option<EPlayerIndex>, veccard: Vec<SCard>, msg, peer: &mut SPeer| {
            let i_epi_relative = oepi.unwrap_or(EPlayerIndex::EPI0).to_usize();
            let serialize_stich = |ostich: Option<&SStich>| {
                if let Some(stich)=ostich {
                    EPlayerIndex::map_from_fn(|epi| {
                        stich.get(epi.wrapping_add(i_epi_relative))
                            .map(SCard::to_string)
                    }).into_raw()
                } else {
                    [None, None, None, None]
                }
            };
            debug_verify!(peer.txmsg.unbounded_send(
                debug_verify!(serde_json::to_string(&(
                    oepi,
                    veccard.into_iter()
                        .map(|card| (card.to_string(), VGamePhaseAction::Game(VGameAction::Zugeben(card))))
                        .collect::<Vec<_>>(),
                    msg,
                    serialize_stich(ostich_current),
                    serialize_stich(ostich_prev),
                    ostich_current.map(|stich| stich.first_playerindex().wrapping_add(EPlayerIndex::SIZE - i_epi_relative)), // winner index of ostich_prev // TODO should be part of ostich_prev
                ))).unwrap().into()
            )).unwrap();
        };
        for epi in EPlayerIndex::values() {
            let ref mut activepeer = self.mapepiopeer[epi];
            let msg = f_active(epi, &mut activepeer.otimeoutcmd);
            if let Some(ref mut peer) = activepeer.opeer.as_mut() {
                let mut veccard = f_cards(epi);
                if let Some(rules) = orules {
                    rules.sort_cards_first_trumpf_then_farbe(&mut veccard);
                } else {
                    rulesramsch::SRulesRamsch::new( // TODO rules dummy is ugly
                        /*n_price*/0, // irrelevant
                        rulesramsch::VDurchmarsch::None, // irrelevant
                    ).sort_cards_first_trumpf_then_farbe(&mut veccard);
                }
                communicate(Some(epi), veccard, msg, peer);
            }
        }
        for peer in self.vecpeer.iter_mut() {
            let msg = f_inactive(peer);
            communicate(None, vec![], msg, peer);
        }
    }
}

impl SPeers {
    fn send_msg(&mut self, /*TODO avoid this parameter*/self_mutex: Arc<Mutex<Self>>, oepi: Option<EPlayerIndex>, ogamephaseaction: Option<VGamePhaseAction>) {
        println!("send_msg({:?}, {:?})", oepi, ogamephaseaction);
        if self.1.ogamephase.is_some() {
            if let Some(epi) = oepi {
                fn handle_err<T, E: std::fmt::Display>(res: Result<T, E>) {
                    match res {
                        Ok(_) => {},
                        Err(e) => println!("Error {}", e),
                    };
                }
                if let Some(gamephaseaction) = ogamephaseaction {
                    use std::mem::discriminant;
                    match self.0.mapepiopeer[epi].otimeoutcmd.as_ref() {
                        None => (),
                        Some(timeoutcmd) => {
                            if discriminant(&gamephaseaction)==discriminant(&timeoutcmd.gamephaseaction) {
                                timeoutcmd.aborthandle.abort();
                                self.0.mapepiopeer[epi].otimeoutcmd = None;
                            }
                        },
                    }
                    if let Some(ref mut gamephase) = debug_verify!(self.1.ogamephase.as_mut()) {
                        match (gamephase, gamephaseaction) {
                            (VGamePhase::DealCards(ref mut dealcards), VGamePhaseAction::DealCards(b_doubling)) => {
                                handle_err(dealcards.announce_doubling(epi, b_doubling));
                            },
                            (VGamePhase::GamePreparations(ref mut gamepreparations), VGamePhaseAction::GamePreparations(ref orulesid)) => {
                                if let Some(orules) = {
                                    let oorules = allowed_rules(
                                        &gamepreparations.ruleset.avecrulegroup[epi],
                                        gamepreparations.fullhand(epi),
                                    )
                                        .find(|orules|
                                            &orules.map(TActivelyPlayableRules::to_string)==orulesid
                                        )
                                        .map(|orules| orules.map(TActivelyPlayableRulesBoxClone::box_clone));
                                    oorules.clone() // TODO needed?
                                } {
                                    handle_err(gamepreparations.announce_game(epi, orules));
                                }
                            },
                            (VGamePhase::DetermineRules(ref mut determinerules), VGamePhaseAction::DetermineRules(ref orulesid)) => {
                                if let Some((_epi_active, vecrulegroup)) = determinerules.which_player_can_do_something() {
                                    if let Some(orules) = {
                                        let oorules = allowed_rules(
                                            &vecrulegroup,
                                            determinerules.fullhand(epi),
                                        )
                                            .find(|orules|
                                                &orules.map(TActivelyPlayableRules::to_string)==orulesid
                                            );
                                        oorules.clone() // TODO clone needed?
                                    } {
                                        handle_err(if let Some(rules) = orules {
                                            determinerules.announce_game(epi, TActivelyPlayableRulesBoxClone::box_clone(rules))
                                        } else {
                                            determinerules.resign(epi)
                                        });
                                    }
                                }
                            },
                            (VGamePhase::Game(ref mut game), VGamePhaseAction::Game(ref gameaction)) => {
                                handle_err(match gameaction {
                                    VGameAction::Stoss => game.stoss(epi),
                                    VGameAction::Zugeben(card) => game.zugeben(*card, epi),
                                });
                            },
                            (VGamePhase::GameResult(gameresult), VGamePhaseAction::GameResult(())) => {
                                gameresult.confirm(epi);
                            },
                            (_gamephase, _cmd) => {
                            },
                        };
                    }
                }
            }
            while self.1.ogamephase.as_ref().map_or(false, |gamephase| gamephase.which_player_can_do_something().is_none()) {
                use VGamePhaseGeneric::*;
                fn next_game(peers: &mut SPeers) -> Option<VGamePhase> {
                    /*           E2
                     * E1                      E3
                     *    E0 SN SN-1 ... S1 S0
                     *
                     * E0 E1 E2 E3 [S0 S1 S2 ... SN]
                     * E1 E2 E3 S0 [S1 S2 ... SN E0]
                     * E2 E3 S0 S1 [S2 ... SN E0 E1]
                     */
                    // Players: E0 E1 E2 E3 [S0 S1 S2 ... SN] (S0 is longest waiting inactive player)
                    peers.0.mapepiopeer.as_raw_mut().rotate_left(1);
                    // Players: E1 E2 E3 E0 [S0 S1 S2 ... SN]
                    if let Some(peer_epi3) = peers.0.mapepiopeer[EPlayerIndex::EPI3].opeer.take() {
                        peers.0.vecpeer.push(peer_epi3);
                    }
                    // Players: E1 E2 E3 -- [S0 S1 S2 ... SN E0] (E1, E2, E3 may be None)
                    // Fill up players one after another
                    assert!(peers.0.mapepiopeer[EPlayerIndex::EPI3].opeer.is_none());
                    for epi in EPlayerIndex::values() {
                        if peers.0.mapepiopeer[epi].opeer.is_none() && !peers.0.vecpeer.is_empty() {
                            peers.0.mapepiopeer[epi].opeer = Some(peers.0.vecpeer.remove(0));
                        }
                    }
                    // Players: E1 E2 E3 S0 [S1 S2 ... SN E0] (E1, E2, E3 may be None)
                    if_then_some!(peers.0.mapepiopeer.iter().all(|activepeer| activepeer.opeer.is_some()),
                        VGamePhase::DealCards(SDealCards::new(static_ruleset(), peers.1.n_stock))
                    )
                    // TODO should we clear timeouts?
                };
                if let Some(gamephase) = self.1.ogamephase.take() {
                    self.1.ogamephase = match gamephase {
                        DealCards(dealcards) => Some(match dealcards.finish() {
                            Ok(gamepreparations) => GamePreparations(gamepreparations),
                            Err(dealcards) => DealCards(dealcards),
                        }),
                        GamePreparations(gamepreparations) => match gamepreparations.finish() {
                            Ok(VGamePreparationsFinish::DetermineRules(determinerules)) => Some(DetermineRules(determinerules)),
                            Ok(VGamePreparationsFinish::DirectGame(game)) => Some(Game(game)),
                            Ok(VGamePreparationsFinish::Stock(gameresult)) => {
                                let mapepiopeer = &mut self.0.mapepiopeer;
                                gameresult.apply_payout(&mut self.1.n_stock, |epi, n_payout| {
                                    if let Some(ref mut peer) = mapepiopeer[epi].opeer {
                                        peer.n_money += n_payout;
                                    }
                                });
                                next_game(self)
                            },
                            Err(gamepreparations) => Some(GamePreparations(gamepreparations)),
                        }
                        DetermineRules(determinerules) => Some(match determinerules.finish() {
                            Ok(game) => Game(game),
                            Err(determinerules) => DetermineRules(determinerules),
                        }),
                        Game(game) => Some(match game.finish() {
                            Ok(gameresult) => GameResult(gameresult),
                            Err(game) => Game(game),
                        }),
                        GameResult(gameresult) => match gameresult.finish() {
                            Ok(gameresult) | Err(gameresult) => {
                                for epi in EPlayerIndex::values() {
                                    if let Some(ref mut peer) = self.0.mapepiopeer[epi].opeer {
                                        peer.n_money += gameresult.an_payout[epi];
                                    }
                                }
                                let n_pay_into_stock = -gameresult.an_payout.iter().sum::<isize>();
                                assert!(
                                    n_pay_into_stock >= 0 // either pay into stock...
                                    || n_pay_into_stock == -self.1.n_stock // ... or exactly empty it (assume that this is always possible)
                                );
                                self.1.n_stock += n_pay_into_stock;
                                assert!(0 <= self.1.n_stock);
                                next_game(self)
                            },
                        },
                    };
                }
            }
            if let Some(ref gamephase) = self.1.ogamephase {
                if let Some(whichplayercandosomething) = verify!(gamephase.which_player_can_do_something()) {
                    fn ask_with_timeout(
                        otimeoutcmd: &mut Option<STimeoutCmd>,
                        epi: EPlayerIndex,
                        str_question: String,
                        itgamephaseaction: impl Iterator<Item=(String, VGamePhaseAction)>,
                        peers_mutex: Arc<Mutex<SPeers>>,
                        gamephaseaction_timeout: VGamePhaseAction,
                    ) -> VMessage {
                        let (timerfuture, aborthandle) = future::abortable(STimerFuture::new(
                            /*n_secs*/2,
                            peers_mutex,
                            epi,
                        ));
                        assert!({
                            use std::mem::discriminant;
                            otimeoutcmd.as_ref().map_or(true, |timeoutcmd|
                                discriminant(&timeoutcmd.gamephaseaction)==discriminant(&gamephaseaction_timeout)
                            )
                        }); // only one active timeout cmd
                        *otimeoutcmd = Some(STimeoutCmd{
                            gamephaseaction: gamephaseaction_timeout,
                            aborthandle,
                        });
                        task::spawn(timerfuture);
                        VMessage::Ask{
                            str_question,
                            vecstrgamephaseaction: itgamephaseaction.collect(),
                        }
                    }
                    use VGamePhaseGeneric::*;
                    match whichplayercandosomething {
                        DealCards((dealcards, epi_doubling)) => {
                            self.0.for_each(
                                None,
                                None,
                                None,
                                |epi| dealcards.first_hand_for(epi).into(),
                                |epi, otimeoutcmd| {
                                    if epi_doubling==epi {
                                        ask_with_timeout(
                                            otimeoutcmd,
                                            epi_doubling,
                                            "Doppeln?".into(),
                                            [(true, "Doppeln"), (false, "Nicht doppeln")]
                                                .iter()
                                                .map(|(b_doubling, str_doubling)| 
                                                    (str_doubling.to_string(), VGamePhaseAction::DealCards(*b_doubling))
                                                ),
                                            self_mutex.clone(),
                                            VGamePhaseAction::DealCards(/*b_doubling*/false),
                                        )
                                    } else {
                                        VMessage::Info(format!("Asking {:?} for doubling", epi_doubling))
                                    }
                                },
                                |_peer| VMessage::Info(format!("Asking {:?} for doubling", epi_doubling)),
                            );
                        },
                        GamePreparations((gamepreparations, epi_announce_game)) => {
                            self.0.for_each(
                                None,
                                None,
                                None,
                                |epi| gamepreparations.fullhand(epi).get().cards().to_vec(),
                                |epi, otimeoutcmd| {
                                    if epi_announce_game==epi {
                                        let itgamephaseaction_rules = allowed_rules(
                                            &gamepreparations.ruleset.avecrulegroup[epi_announce_game],
                                            gamepreparations.fullhand(epi_announce_game),
                                        )
                                            .map(|orules|
                                                (
                                                    if let Some(rules) = orules {
                                                        rules.to_string()
                                                    } else {
                                                        "Weiter".to_string()
                                                    },
                                                    VGamePhaseAction::GamePreparations(orules.map(TActivelyPlayableRules::to_string)),
                                                )
                                            );
                                        let gamephaseaction_rules_default = debug_verify!(itgamephaseaction_rules.clone().next()).unwrap().1.clone();
                                        ask_with_timeout(
                                            otimeoutcmd,
                                            epi_announce_game,
                                            format!("Du bist an {}. Stelle. {}",
                                                epi_announce_game.to_usize() + 1, // EPlayerIndex is 0-based
                                                {
                                                    // TODO inform about player names
                                                    let vectplepirules = gamepreparations.gameannouncements
                                                        .iter()
                                                        .filter_map(|(epi, orules)| orules.as_ref().map(|rules| (epi, rules)))
                                                        .collect::<Vec<_>>();
                                                    if epi==EPlayerIndex::EPI0 {
                                                        assert!(vectplepirules.is_empty());
                                                        "".to_string()
                                                    } else if vectplepirules.is_empty() {
                                                        "Bisher will niemand spielen. Spielst Du?".to_string()
                                                    } else {
                                                        match vectplepirules.iter().exactly_one() {
                                                            Ok((epi_announced, _rules)) => {
                                                                format!(
                                                                    "Vor Dir spielt an {}. Stelle. Spielst Du auch?",
                                                                    epi_announced.to_usize() + 1, // EPlayerIndex is 0-based
                                                                )
                                                            },
                                                            Err(ittplepirules) => {
                                                                format!(
                                                                    "Vor Dir spielen: An {}. Spielst Du auch?",
                                                                    ittplepirules
                                                                        .map(|(epi_announced, _rules)| {
                                                                            format!(
                                                                                "{}. Stelle",
                                                                                epi_announced.to_usize() + 1, // EPlayerIndex is 0-based
                                                                            )
                                                                        })
                                                                        .join(", ")
                                                                )
                                                            },
                                                        }
                                                    }
                                                }
                                                    
                                            ),
                                            itgamephaseaction_rules,
                                            self_mutex.clone(),
                                            gamephaseaction_rules_default,
                                        )
                                    } else {
                                        VMessage::Info(format!("Asking {:?} for game", epi_announce_game))
                                    }
                                },
                                |_peer| VMessage::Info(format!("Asking {:?} for game", epi_announce_game)),
                            );
                        },
                        DetermineRules((determinerules, (epi_determine, vecrulegroup))) => {
                            self.0.for_each(
                                None,
                                None,
                                None,
                                |epi| determinerules.fullhand(epi).get().cards().to_vec(),
                                |epi, otimeoutcmd| {
                                    if epi_determine==epi {
                                        let itgamephaseaction_rules = allowed_rules(
                                            &vecrulegroup,
                                            determinerules.fullhand(epi_determine),
                                        )
                                            .map(|orules|
                                                (
                                                    if let Some(rules) = orules {
                                                        rules.to_string()
                                                    } else {
                                                        "Weiter".to_string()
                                                    },
                                                    VGamePhaseAction::DetermineRules(orules.map(TActivelyPlayableRules::to_string)),
                                                )
                                            );
                                        let gamephaseaction_rules_default = debug_verify!(itgamephaseaction_rules.clone().next()).unwrap().1.clone();
                                        ask_with_timeout(
                                            otimeoutcmd,
                                            epi_determine,
                                            format!(
                                                "Du bist an {}. Stelle. Von {}. Stelle wird {} geboten. Spielst Du etwas staerkeres?", // TODO umlaut-tactics?
                                                epi.to_usize() + 1, // EPlayerIndex is 0-based
                                                determinerules.pairepirules_current_bid.0.to_usize() + 1, // EPlayerIndex is 0-based
                                                determinerules.pairepirules_current_bid.1.to_string(),
                                            ),
                                            itgamephaseaction_rules,
                                            self_mutex.clone(),
                                            gamephaseaction_rules_default,
                                        )
                                    } else {
                                        VMessage::Info(format!("Re-Asking {:?} for game", epi_determine))
                                    }
                                },
                                |_peer| VMessage::Info(format!("Re-Asking {:?} for game", epi_determine)),
                            );
                        },
                        Game((game, (epi_card, vecepi_stoss))) => {
                            self.0.for_each(
                                Some(game.stichseq.current_stich()),
                                game.stichseq.completed_stichs().last(),
                                Some(game.rules.as_ref()),
                                |epi| game.ahand[epi].cards().to_vec(),
                                |epi, otimeoutcmd| {
                                    let mut vecmessage = Vec::new();
                                    if vecepi_stoss.contains(&epi) {
                                        vecmessage.push(("Stoss".into(), VGamePhaseAction::Game(VGameAction::Stoss)));
                                    }
                                    if epi_card==epi {
                                        ask_with_timeout(
                                            otimeoutcmd,
                                            epi_card,
                                            "".into(),
                                            vecmessage.into_iter(),
                                            self_mutex.clone(),
                                            VGamePhaseAction::Game(VGameAction::Zugeben(
                                                *debug_verify!(game.rules.all_allowed_cards(
                                                    &game.stichseq,
                                                    &game.ahand[epi_card],
                                                ).choose(&mut rand::thread_rng())).unwrap()
                                            )),
                                        )
                                    } else if vecmessage.is_empty() {
                                        VMessage::Info(format!("Asking {:?} for card", epi_card))
                                    } else {
                                        VMessage::Ask{
                                            str_question: "".into(),
                                            vecstrgamephaseaction: vecmessage,
                                        }
                                    }
                                },
                                |_peer| VMessage::Info(format!("Asking {:?} for card", epi_card)),
                            );
                        },
                        GameResult((gameresult, mapepib_confirmed)) => {
                            let slcstich = gameresult.game.stichseq.completed_stichs();
                            self.0.for_each(
                                debug_verify!(slcstich.last()),
                                debug_verify!(slcstich.split_last())
                                    .and_then(|(_stich_last, slcstich_up_to_last)|
                                        debug_verify!(slcstich_up_to_last.last())
                                    ),
                                Some(gameresult.game.rules.as_ref()),
                                |_epi| vec![],
                                |epi, otimeoutcmd| {
                                    if !mapepib_confirmed[epi] {
                                        ask_with_timeout(
                                            otimeoutcmd,
                                            epi,
                                            format!("Spiel beendet. {}", if gameresult.an_payout[epi] < 0 {
                                                format!("Verlust: {}", -gameresult.an_payout[epi])
                                            } else {
                                                format!("Gewinn: {}", gameresult.an_payout[epi])
                                            }),
                                            std::iter::once(("Ok".into(), VGamePhaseAction::GameResult(()))),
                                            self_mutex.clone(),
                                            VGamePhaseAction::GameResult(()),
                                        )
                                    } else {
                                        VMessage::Info("Game finished".into())
                                    }
                                },
                                |_peer| VMessage::Info("Game finished".into()),
                            );
                        },
                    }
                }
            }
        } else {
            self.0.for_each(
                None,
                None,
                None,
                |_epi| vec![],
                |_oepi, _otimeoutcmd| VMessage::Info("Waiting for more players.".into()),
                |_peer| VMessage::Info("Waiting for more players.".into()),
            );
        }
    }
}

#[derive(Serialize)]
enum VMessage {
    Info(String),
    Ask{
        str_question: String,
        vecstrgamephaseaction: Vec<(String, VGamePhaseAction)>,
    },
}

// timer adapted from https://rust-lang.github.io/async-book/02_execution/03_wakeups.html
struct STimerFuture {
    state: Arc<Mutex<STimerFutureState>>,
    peers: Arc<Mutex<SPeers>>,
    epi: EPlayerIndex,
}

struct STimerFutureState {
    b_completed: bool,
    owaker: Option<Waker>,
}

impl Future for STimerFuture {
    type Output = ();
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.state.lock().unwrap();
        if state.b_completed {
            let peers_mutex = self.peers.clone();
            let mut peers = debug_verify!(self.peers.lock()).unwrap();
            if let Some(timeoutcmd) = peers.0.mapepiopeer[self.epi].otimeoutcmd.take() {
                peers.send_msg(peers_mutex, Some(self.epi), Some(timeoutcmd.gamephaseaction));
            }
            Poll::Ready(())
        } else {
            state.owaker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

impl STimerFuture {
    fn new(n_secs: u64, peers: Arc<Mutex<SPeers>>, epi: EPlayerIndex) -> Self {
        let state = Arc::new(Mutex::new(STimerFutureState {
            b_completed: false,
            owaker: None,
        }));
        let thread_shared_state = state.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::new(n_secs, /*nanos*/0));
            let mut state = thread_shared_state.lock().unwrap();
            state.b_completed = true;
            if let Some(waker) = state.owaker.take() {
                waker.wake()
            }
        });
        Self {state, peers, epi}
    }
}

async fn handle_connection(peers: Arc<Mutex<SPeers>>, tcpstream: TcpStream, sockaddr: SocketAddr) {
    println!("Incoming TCP connection from: {}", sockaddr);
    let wsstream = debug_verify!(async_tungstenite::accept_async(tcpstream).await).unwrap();
    println!("WebSocket connection established: {}", sockaddr);
    // Insert the write part of this peer to the peer map.
    let (txmsg, rxmsg) = unbounded();
    let peers_mutex = peers.clone();
    debug_verify!(peers.lock()).unwrap().insert(peers_mutex.clone(), SPeer{
        sockaddr,
        txmsg,
        n_money: 0,
    });
    let (sink_ws_out, stream_ws_in) = wsstream.split();
    let broadcast_incoming = stream_ws_in
        .try_filter(|msg| {
            // Broadcasting a Close message from one client
            // will close the other clients.
            future::ready(!msg.is_close())
        })
        .try_for_each(|msg| {
            let str_msg = debug_verify!(msg.to_text()).unwrap();
            let mut peers = debug_verify!(peers.lock()).unwrap();
            let oepi = EPlayerIndex::values()
                .find(|epi| peers.0.mapepiopeer[*epi].opeer.as_ref().map(|peer| peer.sockaddr)==Some(sockaddr));
            println!(
                "Received a message from {} ({:?}): {}",
                sockaddr,
                oepi,
                str_msg,
            );
            match serde_json::from_str(str_msg) {
                Ok(gamephaseaction) => peers.send_msg(peers_mutex.clone(), oepi, Some(gamephaseaction)),
                Err(e) => println!("Error: {}", e),
            }
            future::ok(())
        });
    let receive_from_others = rxmsg.map(Ok).forward(sink_ws_out);
    pin_mut!(broadcast_incoming, receive_from_others); // TODO Is this really needed?
    future::select(broadcast_incoming, receive_from_others).await;
    println!("{} disconnected", &sockaddr);
    debug_verify!(peers.lock()).unwrap().remove(&sockaddr);
}

async fn internal_run() -> Result<(), Error> {
    let str_addr = "127.0.0.1:8080";
    let peers = Arc::new(Mutex::new(SPeers::default()));
    // Create the event loop and TCP listener we'll accept connections on.
    let listener = debug_verify!(TcpListener::bind(&str_addr).await).unwrap();
    println!("Listening on: {}", str_addr);
    // Let's spawn the handling of each connection in a separate task.
    while let Ok((tcpstream, sockaddr)) = listener.accept().await {
        task::spawn(handle_connection(peers.clone(), tcpstream, sockaddr));
    }
    Ok(())
}

pub fn run() -> Result<(), Error> {
    task::block_on(internal_run())
}

