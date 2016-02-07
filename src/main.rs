extern crate rand;
extern crate ncurses;
extern crate itertools;

mod card;
mod stich;
mod combinatorics;
mod cardvectorparser;
mod hand;
mod rules;
mod rulesrufspiel;
mod rulessolo;
mod gamestate;
mod game;
mod player;
mod playercomputer;
mod playerhuman;
mod suspicion;
mod ruleset;
mod accountbalance;
mod skui;

use game::*;
use std::sync::mpsc;
use card::CCard;
use accountbalance::SAccountBalance;

fn main() {
    skui::init_ui();
    let mut accountbalance = SAccountBalance::new();
    for _igame in 0..4 { // TODO make number of rounds adjustable
        let mut game = CGame::new();
        skui::logln(&format!("Hand 0 : {}", game.m_gamestate.m_ahand[0]));
        if game.start_game(0) {
            while let Some(eplayerindex)=game.which_player_can_do_something() {
                let (txcard, rxcard) = mpsc::channel::<CCard>();
                game.m_vecplayer[eplayerindex].take_control(
                    &game.m_gamestate,
                    txcard.clone()
                );
                let card_played = rxcard.recv().unwrap();
                game.zugeben(card_played, eplayerindex);
            }
            let an_points = game.points_per_player();
            skui::logln("Results");
            for eplayerindex in 0..4 {
                skui::logln(&format!("Player {}: {} points", eplayerindex, an_points[eplayerindex]));
            }
            accountbalance.apply_payout(&game.payout());
        }
        skui::logln("Account balance:");
        for eplayerindex in 0..4 {
            skui::logln(&format!("Player {}: {}", eplayerindex, accountbalance.get(eplayerindex)));
        }
    }
    skui::end_ui();
}
